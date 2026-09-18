//! Download + verify + install published packs from a navi-server DocumentRoot.
//!
//! Contract: [navi-server `docs/client-fetch.md`](https://github.com/Supermagnum/navi-server/blob/main/docs/client-fetch.md).
//! Bake stems (`europe_monaco-latest`) are remapped to Geofabrik leaf stems
//! (`monaco-latest`) so existing Download / planning / pill paths keep working.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::acquisition::{leaf_stem_for_region_id, normalize_region_id};
use super::{http_get_text, PackServerError, ReadyRegion};
use crate::download::http::{
    progress_label_for_resume, stream_get_to_file_blocking, StreamDownloadOpts,
};
use crate::download::phase_timing;
use crate::download::progress as download_progress;
use crate::routing::indexed::{manifest_path, server_install_path, NaviManifest, PackStatus};

/// Sidecar under `.pack-fetch-{leaf}.partial/` so a relaunch can resume the same
/// generation. Stale generations are discarded.
///
/// Resume covers app close / next open only — not OS background kills while the
/// process is dead (that would need WorkManager / a foreground service).
#[derive(Debug, Serialize, Deserialize)]
struct PackFetchState {
    schema: u32,
    region_id: String,
    generation: String,
    base_url: String,
    leaf_stem: String,
}

impl PackFetchState {
    const SCHEMA: u32 = 1;
    const FILE: &'static str = "fetch-state.json";
}

#[derive(Debug, Deserialize)]
struct ClientManifest {
    #[serde(default)]
    stem: Option<String>,
    #[serde(default)]
    bake_id: Option<String>,
    #[serde(default)]
    navi_manifest: Option<String>,
    files: BTreeMap<String, ClientFileMeta>,
}

#[derive(Debug, Deserialize)]
struct ClientFileMeta {
    sha256: String,
    #[serde(default)]
    bytes: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerInstallStamp {
    pub schema: u32,
    pub region_id: String,
    pub generation: Option<String>,
    pub bake_stem: String,
    pub leaf_stem: String,
    pub base_url: String,
}

impl ServerInstallStamp {
    pub const SCHEMA: u32 = 1;

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| format!("parse server install stamp: {e}"))
    }

    pub fn load_for_leaf(data_dir: &Path, leaf_stem: &str) -> Result<Self, String> {
        Self::load(&server_install_path(data_dir, leaf_stem))
    }
}

fn join_url(base: &str, path_or_url: &str) -> String {
    let p = path_or_url.trim();
    if p.starts_with("http://") || p.starts_with("https://") {
        return p.to_string();
    }
    let base = base.trim().trim_end_matches('/');
    if p.starts_with('/') {
        format!("{base}{p}")
    } else {
        format!("{base}/{p}")
    }
}

fn bake_stem_from_manifest(client: &ClientManifest, ready: &ReadyRegion) -> String {
    if let Some(stem) = client
        .stem
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return stem.to_string();
    }
    if let Some(bake) = client
        .bake_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return format!("{bake}-latest");
    }
    // Fallback: first file that looks like `{stem}.navi-manifest.json`.
    for name in client.files.keys() {
        if let Some(stem) = name.strip_suffix(".navi-manifest.json") {
            return stem.to_string();
        }
    }
    leaf_stem_for_region_id(&ready.region_id)
}

fn remap_filename(name: &str, bake_stem: &str, leaf_stem: &str) -> String {
    if bake_stem == leaf_stem {
        return name.to_string();
    }
    if let Some(rest) = name.strip_prefix(bake_stem) {
        format!("{leaf_stem}{rest}")
    } else {
        name.to_string()
    }
}

fn rewrite_navi_manifest_bytes(
    raw: &[u8],
    bake_stem: &str,
    leaf_stem: &str,
) -> Result<Vec<u8>, String> {
    let mut man: NaviManifest =
        serde_json::from_slice(raw).map_err(|e| format!("navi-manifest parse: {e}"))?;
    man.stem = leaf_stem.to_string();
    man.pbf_filename = format!("{leaf_stem}.osm.pbf");
    // Drop absolute server elev paths; device uses data_dir/elevation when present.
    man.elev_dir = None;
    if bake_stem != leaf_stem {
        man.poi_barrier_file = remap_filename(&man.poi_barrier_file, bake_stem, leaf_stem);
        if let Some(w) = man.wetland_file.take() {
            man.wetland_file = Some(remap_filename(&w, bake_stem, leaf_stem));
        }
        man.graph_files = man
            .graph_files
            .into_iter()
            .map(|(k, v)| (k, remap_filename(&v, bake_stem, leaf_stem)))
            .collect();
        for tiles in man.graph_tiles.values_mut() {
            for t in tiles.iter_mut() {
                t.file = remap_filename(&t.file, bake_stem, leaf_stem);
            }
        }
        for t in man.wetland_tiles.iter_mut() {
            t.file = remap_filename(&t.file, bake_stem, leaf_stem);
        }
    }
    serde_json::to_vec_pretty(&man).map_err(|e| format!("navi-manifest serialize: {e}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn verify_hex(got: &str, expect: &str) -> Result<(), String> {
    if got.eq_ignore_ascii_case(expect.trim()) {
        Ok(())
    } else {
        Err(format!(
            "sha256 mismatch: got {got}, expect {}",
            expect.trim()
        ))
    }
}

/// Stream GET → file with sha256 verify. Resumes via `.partial` + HTTP Range
/// (same path as Geofabrik PBF downloads). Apache pack CDN advertises
/// `Accept-Ranges: bytes`.
///
/// Resume covers app close / next open when staging + partials remain on disk.
/// It does **not** continue while the process is dead (no WorkManager / FGS).
fn http_download_verified(
    url: &str,
    dest: &Path,
    expect_sha256: &str,
    expect_bytes: Option<u64>,
    progress_base: u64,
    progress_total: Option<u64>,
    progress_label: &str,
) -> Result<u64, String> {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    // Already complete and verified — skip (file-level resume across relaunches).
    if dest.is_file() {
        if let Ok(meta) = dest.metadata() {
            let len = meta.len();
            let size_ok = expect_bytes.map(|n| n == len).unwrap_or(true);
            if size_ok {
                match file_sha256_hex(dest) {
                    Ok(got) if got.eq_ignore_ascii_case(expect_sha256.trim()) => {
                        download_progress::set(
                            progress_base.saturating_add(len),
                            progress_total,
                            progress_label,
                        );
                        return Ok(len);
                    }
                    _ => {
                        let _ = fs::remove_file(dest);
                    }
                }
            } else {
                let _ = fs::remove_file(dest);
            }
        }
    }

    let partial = {
        let mut p = dest.as_os_str().to_owned();
        p.push(".partial");
        std::path::PathBuf::from(p)
    };
    let resume_from = partial.metadata().map(|m| m.len()).unwrap_or(0);
    let label = progress_label_for_resume(progress_label, resume_from);
    download_progress::set(
        progress_base.saturating_add(resume_from),
        progress_total,
        &label,
    );

    // Shared Range downloader keeps `.partial` across interrupts and sends
    // `Range: bytes={n}-` when resuming. On a full 200 it rewrites from scratch.
    let _ = stream_get_to_file_blocking(StreamDownloadOpts {
        url,
        dest,
        headers: Default::default(),
        resume_from: 0, // auto-detect from sibling .partial
        expected_bytes: expect_bytes,
        retries: crate::download::http::DEFAULT_RETRIES,
        progress_label: &label,
        allow_not_found: false,
    })
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("GET {url}: not found"))?;

    let written = dest.metadata().map(|m| m.len()).unwrap_or(0);
    if let Some(n) = expect_bytes {
        if written != n {
            let _ = fs::remove_file(dest);
            return Err(format!(
                "size mismatch for {url}: got {written}, expect {n}"
            ));
        }
    }
    let got = file_sha256_hex(dest).map_err(|e| e.to_string())?;
    if let Err(e) = verify_hex(&got, expect_sha256) {
        let _ = fs::remove_file(dest);
        return Err(e);
    }
    download_progress::set(
        progress_base.saturating_add(written),
        progress_total,
        progress_label,
    );
    Ok(written)
}

fn file_sha256_hex(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 256];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn write_stub_pbf(path: &Path) -> Result<(), String> {
    // plan_car_route / resolvePbf require a file; packs carry the graph. 16 KiB
    // clears the Android >10_000 length gate without implying a real extract.
    const STUB: usize = 16 * 1024;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut f = File::create(path).map_err(|e| e.to_string())?;
    f.write_all(&vec![0u8; STUB]).map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn confirm_usable(data_dir: &Path, leaf_stem: &str) -> Result<(), String> {
    let man_path = manifest_path(data_dir, leaf_stem);
    let man = NaviManifest::load(&man_path).map_err(|e| format!("load installed manifest: {e}"))?;
    match man.status_pack_files(data_dir) {
        PackStatus::Ready => Ok(()),
        other => Err(format!(
            "installed packs not Ready for {leaf_stem}: {other:?}"
        )),
    }
}

/// Ensure staged (not yet promoted) packs match this client's format versions.
fn assert_staged_pack_format_compatible(out_dir: &Path, leaf_stem: &str) -> Result<(), String> {
    use crate::routing::indexed::{
        GRAPH_FORMAT_VERSION, POI_BARRIER_FORMAT_VERSION, WETLAND_FORMAT_VERSION,
    };
    let man_path = out_dir.join(format!("{leaf_stem}.navi-manifest.json"));
    let man = NaviManifest::load(&man_path)
        .map_err(|e| format!("load staged navi-manifest for format check: {e}"))?;
    if man.graph_format_version != GRAPH_FORMAT_VERSION {
        return Err(format!(
            "server pack graph_format_version={} (client needs {GRAPH_FORMAT_VERSION}) — not installing",
            man.graph_format_version
        ));
    }
    if man.poi_barrier_format_version != POI_BARRIER_FORMAT_VERSION {
        return Err(format!(
            "server pack poi_barrier_format_version={} (client needs {POI_BARRIER_FORMAT_VERSION}) — not installing",
            man.poi_barrier_format_version
        ));
    }
    if man.wetland_format_version != WETLAND_FORMAT_VERSION {
        return Err(format!(
            "server pack wetland_format_version={} (client needs {WETLAND_FORMAT_VERSION}) — not installing",
            man.wetland_format_version
        ));
    }
    if man.schema != NaviManifest::SCHEMA {
        return Err(format!(
            "server pack manifest schema={} (client needs {}) — not installing",
            man.schema,
            NaviManifest::SCHEMA
        ));
    }
    Ok(())
}

/// Fetch, verify, remap to leaf stem, and install into `data_dir`.
pub fn try_fetch_region_packs(
    ready: &ReadyRegion,
    base_url: &str,
    data_dir: Option<&Path>,
) -> Result<(), String> {
    let total_t0 = phase_timing::start("pack_fetch.total");
    let data_dir = data_dir.ok_or_else(|| "pack fetch needs data_dir".to_string())?;
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;

    let region_id = normalize_region_id(&ready.region_id);
    let generation = ready
        .generation
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "ready region missing generation".to_string())?;

    let manifest_t0 = phase_timing::start("pack_fetch.manifest");
    let manifest_rel = ready
        .manifest_url
        .clone()
        .unwrap_or_else(|| format!("/packs/{region_id}/{generation}/manifest.json"));
    let manifest_url = join_url(base_url, &manifest_rel);
    let body = http_get_text(&manifest_url, Duration::from_secs(60)).map_err(|e| match e {
        PackServerError::Http(code) => format!("manifest HTTP {code}"),
        PackServerError::Timeout => "manifest timeout".into(),
        PackServerError::Other(s) => s,
    })?;
    let client: ClientManifest =
        serde_json::from_str(&body).map_err(|e| format!("manifest.json parse: {e}"))?;
    if client.files.is_empty() {
        return Err("manifest.json has empty files map".into());
    }
    phase_timing::end_detail(
        "pack_fetch.manifest",
        manifest_t0,
        &format!("files={}", client.files.len()),
    );

    let bake_stem = bake_stem_from_manifest(&client, ready);
    let leaf_stem = leaf_stem_for_region_id(&region_id);
    let pack_base = join_url(base_url, &format!("/packs/{region_id}/{generation}"));

    let navi_name = client
        .navi_manifest
        .clone()
        .unwrap_or_else(|| format!("{bake_stem}.navi-manifest.json"));
    let navi_meta = client
        .files
        .get(&navi_name)
        .ok_or_else(|| format!("manifest.json missing navi-manifest file entry ({navi_name})"))?;

    let staging = data_dir.join(format!(".pack-fetch-{leaf_stem}.partial"));
    let state_path = staging.join(PackFetchState::FILE);
    let base_norm = base_url.trim().trim_end_matches('/').to_string();
    let resume_ok = fs::read_to_string(&state_path)
        .ok()
        .and_then(|s| serde_json::from_str::<PackFetchState>(&s).ok())
        .is_some_and(|st| {
            st.schema == PackFetchState::SCHEMA
                && st.region_id == region_id
                && st.generation == generation
                && st.base_url == base_norm
                && st.leaf_stem == leaf_stem
        });
    if staging.exists() && !resume_ok {
        // Different generation / URL / leaf — discard stale partials.
        let _ = fs::remove_dir_all(&staging);
    }
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let out = staging.join("out");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let state = PackFetchState {
        schema: PackFetchState::SCHEMA,
        region_id: region_id.clone(),
        generation: generation.to_string(),
        base_url: base_norm,
        leaf_stem: leaf_stem.clone(),
    };
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // Early format gate: pull only the navi-manifest JSON before multi-GB binaries.
    // Live duckdns packs may still be an older graph_format_version; rejecting
    // after a full download would waste bandwidth and delay the local rebuild fallback.
    download_progress::set(0, Some(1), "Checking pack format on server…");
    let format_t0 = phase_timing::start("pack_fetch.format_gate");
    {
        let url = join_url(&pack_base, &navi_name);
        let bytes = super::http_get_bytes(&url, Duration::from_secs(120))
            .map_err(|e| format!("GET {navi_name}: {e}"))?;
        if let Some(n) = navi_meta.bytes {
            if bytes.len() as u64 != n {
                return Err(format!(
                    "size mismatch {navi_name}: got {}, expect {n}",
                    bytes.len()
                ));
            }
        }
        verify_hex(&sha256_hex(&bytes), &navi_meta.sha256)?;
        fs::write(staging.join(&navi_name), &bytes).map_err(|e| e.to_string())?;
        let rewritten = rewrite_navi_manifest_bytes(&bytes, &bake_stem, &leaf_stem)?;
        fs::write(
            out.join(format!("{leaf_stem}.navi-manifest.json")),
            rewritten,
        )
        .map_err(|e| e.to_string())?;
        if let Err(e) = assert_staged_pack_format_compatible(&out, &leaf_stem) {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    }
    phase_timing::end("pack_fetch.format_gate", format_t0);

    let file_count = client.files.len() as u64;
    let known_bytes: u64 = client.files.values().filter_map(|m| m.bytes).sum();
    let progress_total = if known_bytes > 0 {
        Some(known_bytes)
    } else if file_count > 0 {
        Some(file_count)
    } else {
        None
    };
    let use_byte_progress = known_bytes > 0;
    download_progress::set(
        navi_meta.bytes.unwrap_or(0),
        progress_total,
        &format!("Fetching packs ({file_count} files)…"),
    );

    let mut done_bytes: u64 = navi_meta.bytes.unwrap_or(0);
    let mut done_files: u64 = 1;
    let files_t0 = phase_timing::start("pack_fetch.download_files");
    for (remote_name, meta) in &client.files {
        if remote_name == &navi_name {
            continue;
        }
        done_files += 1;
        let label = format!("Fetching packs ({done_files}/{file_count}): {remote_name}");
        let url = join_url(&pack_base, remote_name);
        let staged = staging.join(remote_name);
        let file_t0 = Instant::now();
        // Prefer streaming for large binaries; small JSON can use RAM path.
        let is_json = remote_name.ends_with(".json");
        let got_bytes = if is_json {
            download_progress::set(
                if use_byte_progress {
                    done_bytes
                } else {
                    done_files.saturating_sub(1)
                },
                progress_total,
                &label,
            );
            let bytes = super::http_get_bytes(&url, Duration::from_secs(120))
                .map_err(|e| format!("GET {remote_name}: {e}"))?;
            if let Some(n) = meta.bytes {
                if bytes.len() as u64 != n {
                    return Err(format!(
                        "size mismatch {remote_name}: got {}, expect {n}",
                        bytes.len()
                    ));
                }
            }
            verify_hex(&sha256_hex(&bytes), &meta.sha256)?;
            fs::write(&staged, &bytes).map_err(|e| e.to_string())?;
            let n = bytes.len() as u64;
            if use_byte_progress {
                done_bytes = done_bytes.saturating_add(n);
                download_progress::set(done_bytes, progress_total, &label);
            } else {
                download_progress::set(done_files, progress_total, &label);
            }
            n
        } else {
            let got = http_download_verified(
                &url,
                &staged,
                &meta.sha256,
                meta.bytes,
                if use_byte_progress {
                    done_bytes
                } else {
                    done_files.saturating_sub(1)
                },
                progress_total,
                &label,
            )?;
            if use_byte_progress {
                done_bytes = done_bytes.saturating_add(got);
            } else {
                download_progress::set(done_files, progress_total, &label);
            }
            got
        };
        let file_ms = file_t0.elapsed().as_secs_f64() * 1000.0;
        let rate = if file_ms > 0.0 {
            (got_bytes as f64 / 1_000_000.0) / (file_ms / 1000.0)
        } else {
            0.0
        };
        log::info!(
            target: "PHASE_TIMING",
            "END phase=pack_fetch.file elapsed_ms={file_ms:.1} file={remote_name} \
             bytes={got_bytes} done_files={done_files}/{file_count} mb_per_s={rate:.2}"
        );
    }
    phase_timing::end_detail(
        "pack_fetch.download_files",
        files_t0,
        &format!("files={done_files} bytes={done_bytes}"),
    );

    download_progress::set(
        progress_total.unwrap_or(done_files),
        progress_total,
        "Installing packs…",
    );

    let install_t0 = phase_timing::start("pack_fetch.install_promote");
    // Remap bake → leaf into final names under staging/out (navi-manifest already there).
    for remote_name in client.files.keys() {
        if remote_name == &navi_name || remote_name.ends_with(".navi-manifest.json") {
            continue;
        }
        let src = staging.join(remote_name);
        let dest_name = remap_filename(remote_name, &bake_stem, &leaf_stem);
        let dest = out.join(&dest_name);
        fs::rename(&src, &dest)
            .or_else(|_| {
                fs::copy(&src, &dest)
                    .map(|_| ())
                    .and_then(|_| fs::remove_file(&src))
            })
            .map_err(|e| format!("stage {remote_name} → {dest_name}: {e}"))?;
    }

    // Promote out/* into data_dir.
    for ent in fs::read_dir(&out).map_err(|e| e.to_string())? {
        let ent = ent.map_err(|e| e.to_string())?;
        let name = ent.file_name();
        let dest = data_dir.join(&name);
        let tmp = data_dir.join(format!("{}.promoting", name.to_string_lossy()));
        let _ = fs::remove_file(&tmp);
        fs::rename(ent.path(), &tmp)
            .or_else(|_| fs::copy(ent.path(), &tmp).map(|_| ()))
            .map_err(|e| e.to_string())?;
        fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    }

    let stub = data_dir.join(format!("{leaf_stem}.osm.pbf"));
    if !stub.is_file() || stub.metadata().map(|m| m.len()).unwrap_or(0) < 10_000 {
        write_stub_pbf(&stub)?;
    }

    let stamp = ServerInstallStamp {
        schema: ServerInstallStamp::SCHEMA,
        region_id: region_id.clone(),
        generation: Some(generation.to_string()),
        bake_stem: bake_stem.clone(),
        leaf_stem: leaf_stem.clone(),
        base_url: base_url.trim().trim_end_matches('/').to_string(),
    };
    let stamp_path = server_install_path(data_dir, &leaf_stem);
    let stamp_tmp = stamp_path.with_extension("json.partial");
    fs::write(
        &stamp_tmp,
        serde_json::to_vec_pretty(&stamp).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&stamp_tmp, &stamp_path).map_err(|e| e.to_string())?;

    confirm_usable(data_dir, &leaf_stem)?;
    let _ = fs::remove_dir_all(&staging);
    phase_timing::end("pack_fetch.install_promote", install_t0);
    phase_timing::end("pack_fetch.total", total_t0);
    log::info!(
        target: "NaviPack",
        "installed packs region={region_id} leaf={leaf_stem} bake={bake_stem} gen={generation}"
    );
    Ok(())
}

/// Best-effort cleanup of a failed staging tree (callers may ignore errors).
#[allow(dead_code)]
pub fn cleanup_partial_fetch(data_dir: &Path, leaf_stem: &str) {
    let staging = data_dir.join(format!(".pack-fetch-{leaf_stem}.partial"));
    let _ = fs::remove_dir_all(staging);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::indexed::{
        GRAPH_FORMAT_VERSION, POI_BARRIER_FORMAT_VERSION, WETLAND_FORMAT_VERSION,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn sha(s: &[u8]) -> String {
        sha256_hex(s)
    }

    fn serve_files(files: BTreeMap<String, Vec<u8>>) -> String {
        serve_files_recording(files, None)
    }

    fn serve_files_recording(
        files: BTreeMap<String, Vec<u8>>,
        hits: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        thread::spawn(move || {
            for _ in 0..128 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/");
                let key = path.trim_start_matches('/');
                let range = req
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                    .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()));
                if let Some(h) = &hits {
                    if let Ok(mut g) = h.lock() {
                        let tag = match &range {
                            Some(r) => format!("{key}|{r}"),
                            None => key.to_string(),
                        };
                        g.push(tag);
                    }
                }
                let body = files.get(key).cloned().or_else(|| {
                    key.rsplit('/')
                        .next()
                        .and_then(|leaf| files.get(leaf).cloned())
                });
                if let Some(body) = body {
                    if let Some(r) = range.as_deref() {
                        // bytes=START- or bytes=START-END
                        let start = r
                            .strip_prefix("bytes=")
                            .and_then(|s| s.split('-').next())
                            .and_then(|s| s.parse::<usize>().ok())
                            .unwrap_or(0)
                            .min(body.len());
                        let slice = &body[start..];
                        let resp = format!(
                            "HTTP/1.1 206 Partial Content\r\n\
                             Accept-Ranges: bytes\r\n\
                             Content-Range: bytes {start}-{}/{}\r\n\
                             Content-Length: {}\r\n\
                             Connection: close\r\n\r\n",
                            body.len().saturating_sub(1).max(start),
                            body.len(),
                            slice.len()
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.write_all(slice);
                    } else {
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.write_all(&body);
                    }
                } else {
                    let resp =
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(resp.as_bytes());
                }
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn leaf_stem_from_path() {
        assert_eq!(leaf_stem_for_region_id("europe/monaco"), "monaco-latest");
        assert_eq!(
            leaf_stem_for_region_id("europe/norway/ostlandet"),
            "ostlandet-latest"
        );
    }

    #[test]
    fn remap_prefixes_bake_stem() {
        assert_eq!(
            remap_filename(
                "europe_monaco-latest.navi-graph-car.rkyv",
                "europe_monaco-latest",
                "monaco-latest"
            ),
            "monaco-latest.navi-graph-car.rkyv"
        );
    }

    #[test]
    fn fetch_installs_tiny_region_with_checksum() {
        let car = vec![0u8; 64];
        let foot = vec![1u8; 64];
        let poi = vec![2u8; 64];
        let wet = vec![3u8; 48];
        let bake = "europe_monaco-latest";
        let navi = serde_json::json!({
            "schema": 1,
            "stem": bake,
            "pbf_filename": format!("{bake}.osm.pbf"),
            "pbf_size_bytes": 100,
            "pbf_modified_unix_secs": 1,
            "graph_files": {
                "car": format!("{bake}.navi-graph-car.rkyv"),
                "foot": format!("{bake}.navi-graph-foot.rkyv")
            },
            "graph_tiles": {},
            "graph_format_version": GRAPH_FORMAT_VERSION,
            "poi_barrier_file": format!("{bake}.navi-poi-barrier.rkyv"),
            "poi_barrier_format_version": POI_BARRIER_FORMAT_VERSION,
            "wetland_file": format!("{bake}.navi-wetland.rkyv"),
            "wetland_tiles": [],
            "wetland_format_version": WETLAND_FORMAT_VERSION,
            "has_delta_h": false
        });
        let navi_bytes = serde_json::to_vec_pretty(&navi).unwrap();
        let car_name = format!("{bake}.navi-graph-car.rkyv");
        let foot_name = format!("{bake}.navi-graph-foot.rkyv");
        let poi_name = format!("{bake}.navi-poi-barrier.rkyv");
        let wet_name = format!("{bake}.navi-wetland.rkyv");
        let man_name = format!("{bake}.navi-manifest.json");

        let mut blob = BTreeMap::new();
        blob.insert(car_name.clone(), car.clone());
        blob.insert(foot_name.clone(), foot.clone());
        blob.insert(poi_name.clone(), poi.clone());
        blob.insert(wet_name.clone(), wet.clone());
        blob.insert(man_name.clone(), navi_bytes.clone());

        let client = serde_json::json!({
            "schema": 1,
            "generation": "g1",
            "region_id": "europe/monaco",
            "bake_id": "europe_monaco",
            "stem": bake,
            "navi_manifest": man_name,
            "files": {
                car_name.clone(): {"sha256": sha(&car), "bytes": 64},
                foot_name.clone(): {"sha256": sha(&foot), "bytes": 64},
                poi_name.clone(): {"sha256": sha(&poi), "bytes": 64},
                wet_name.clone(): {"sha256": sha(&wet), "bytes": 48},
                man_name.clone(): {"sha256": sha(&navi_bytes), "bytes": navi_bytes.len()},
            }
        });
        let client_bytes = serde_json::to_vec_pretty(&client).unwrap();
        blob.insert(
            "packs/europe/monaco/g1/manifest.json".into(),
            client_bytes.clone(),
        );
        blob.insert("manifest.json".into(), client_bytes);

        let base = serve_files(blob);
        let dir = tempfile::tempdir().unwrap();
        let ready = ReadyRegion {
            region_id: "europe/monaco".into(),
            generation: Some("g1".into()),
            bytes: Some(1000),
            manifest_url: Some("/packs/europe/monaco/g1/manifest.json".into()),
        };
        try_fetch_region_packs(&ready, &base, Some(dir.path())).expect("fetch");
        assert!(dir
            .path()
            .join("monaco-latest.navi-manifest.json")
            .is_file());
        assert!(dir
            .path()
            .join("monaco-latest.navi-graph-car.rkyv")
            .is_file());
        assert!(dir
            .path()
            .join("monaco-latest.navi-server-install.json")
            .is_file());
        assert!(dir.path().join("monaco-latest.osm.pbf").is_file());
        let man = NaviManifest::load(&dir.path().join("monaco-latest.navi-manifest.json")).unwrap();
        assert_eq!(man.stem, "monaco-latest");
        assert_eq!(man.status_pack_files(dir.path()), PackStatus::Ready);
    }

    #[test]
    fn http_download_verified_resumes_partial_with_range() {
        let payload: Vec<u8> = (0u8..200).collect();
        let mut blob = BTreeMap::new();
        blob.insert("big.bin".into(), payload.clone());
        let hits = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let base = serve_files_recording(blob, Some(hits.clone()));
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("big.bin");
        // Simulate interrupt: leave a .partial with the first half.
        let half = payload.len() / 2;
        let mut partial = dest.as_os_str().to_owned();
        partial.push(".partial");
        let partial_path = std::path::PathBuf::from(partial);
        fs::write(&partial_path, &payload[..half]).unwrap();

        let got = http_download_verified(
            &format!("{base}/big.bin"),
            &dest,
            &sha(&payload),
            Some(payload.len() as u64),
            0,
            Some(payload.len() as u64),
            "test pack",
        )
        .expect("resume download");
        assert_eq!(got, payload.len() as u64);
        assert_eq!(fs::read(&dest).unwrap(), payload);
        assert!(!partial_path.is_file(), "partial should be promoted away");
        let recorded = hits.lock().unwrap().clone();
        assert!(
            recorded.iter().any(|h| h.contains("bytes=")),
            "expected Range request in hits={recorded:?}"
        );
    }

    #[test]
    fn fetch_rejects_old_graph_format_without_promoting() {
        let car = vec![0u8; 64];
        let foot = vec![1u8; 64];
        let poi = vec![2u8; 64];
        let wet = vec![3u8; 48];
        let bake = "europe_monaco-latest";
        let navi = serde_json::json!({
            "schema": 1,
            "stem": bake,
            "pbf_filename": format!("{bake}.osm.pbf"),
            "pbf_size_bytes": 100,
            "pbf_modified_unix_secs": 1,
            "graph_files": {
                "car": format!("{bake}.navi-graph-car.rkyv"),
                "foot": format!("{bake}.navi-graph-foot.rkyv")
            },
            "graph_tiles": {},
            "graph_format_version": 6,
            "poi_barrier_file": format!("{bake}.navi-poi-barrier.rkyv"),
            "poi_barrier_format_version": POI_BARRIER_FORMAT_VERSION,
            "wetland_file": format!("{bake}.navi-wetland.rkyv"),
            "wetland_tiles": [],
            "wetland_format_version": WETLAND_FORMAT_VERSION,
            "has_delta_h": false
        });
        let navi_bytes = serde_json::to_vec_pretty(&navi).unwrap();
        let car_name = format!("{bake}.navi-graph-car.rkyv");
        let foot_name = format!("{bake}.navi-graph-foot.rkyv");
        let poi_name = format!("{bake}.navi-poi-barrier.rkyv");
        let wet_name = format!("{bake}.navi-wetland.rkyv");
        let man_name = format!("{bake}.navi-manifest.json");

        let mut blob = BTreeMap::new();
        blob.insert(car_name.clone(), car.clone());
        blob.insert(foot_name.clone(), foot.clone());
        blob.insert(poi_name.clone(), poi.clone());
        blob.insert(wet_name.clone(), wet.clone());
        blob.insert(man_name.clone(), navi_bytes.clone());

        let client = serde_json::json!({
            "schema": 1,
            "generation": "g-old",
            "region_id": "europe/monaco",
            "bake_id": "europe_monaco",
            "stem": bake,
            "navi_manifest": man_name,
            "files": {
                car_name.clone(): {"sha256": sha(&car), "bytes": 64},
                foot_name.clone(): {"sha256": sha(&foot), "bytes": 64},
                poi_name.clone(): {"sha256": sha(&poi), "bytes": 64},
                wet_name.clone(): {"sha256": sha(&wet), "bytes": 48},
                man_name.clone(): {"sha256": sha(&navi_bytes), "bytes": navi_bytes.len()},
            }
        });
        let client_bytes = serde_json::to_vec_pretty(&client).unwrap();
        blob.insert(
            "packs/europe/monaco/g-old/manifest.json".into(),
            client_bytes.clone(),
        );
        blob.insert("manifest.json".into(), client_bytes);

        let hits = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let base = serve_files_recording(blob, Some(hits.clone()));
        let dir = tempfile::tempdir().unwrap();
        // Plant a sentinel that must not be overwritten by a rejected fetch.
        let sentinel = dir.path().join("monaco-latest.navi-manifest.json");
        fs::write(&sentinel, b"keep-me").unwrap();
        let ready = ReadyRegion {
            region_id: "europe/monaco".into(),
            generation: Some("g-old".into()),
            bytes: Some(1000),
            manifest_url: Some("/packs/europe/monaco/g-old/manifest.json".into()),
        };
        let err = try_fetch_region_packs(&ready, &base, Some(dir.path())).unwrap_err();
        assert!(
            err.contains("graph_format_version=6"),
            "expected format rejection, got {err}"
        );
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "keep-me");
        assert!(!dir
            .path()
            .join("monaco-latest.navi-graph-car.rkyv")
            .is_file());
        let requested = hits.lock().unwrap().clone();
        assert!(
            requested.iter().any(|p| p.contains("manifest.json")),
            "expected catalog manifest fetch, got {requested:?}"
        );
        assert!(
            requested.iter().any(|p| p.contains("navi-manifest.json")),
            "expected early navi-manifest fetch, got {requested:?}"
        );
        assert!(
            !requested.iter().any(|p| p.ends_with(".rkyv")),
            "format reject must not download pack binaries, got {requested:?}"
        );
    }
}
