//! Download + verify + install published packs from a navi-server DocumentRoot.
//!
//! Contract: [navi-server `docs/client-fetch.md`](https://github.com/Supermagnum/navi-server/blob/main/docs/client-fetch.md).
//! Bake stems (`europe_monaco-latest`) are remapped to Geofabrik leaf stems
//! (`monaco-latest`) so existing Download / planning / pill paths keep working.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::acquisition::{leaf_stem_for_region_id, normalize_region_id};
use super::{http_get_text, PackServerError, ReadyRegion, CONNECTIVITY_TIMEOUT, USER_AGENT};
use crate::routing::indexed::{manifest_path, server_install_path, NaviManifest, PackStatus};

/// Pack GET timeout (large regions; streaming — not held entirely in RAM).
const PACK_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(3600);

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

/// Stream GET → file with running sha256. Atomic via `.partial` rename.
fn http_download_verified(
    url: &str,
    dest: &Path,
    expect_sha256: &str,
    expect_bytes: Option<u64>,
) -> Result<(), String> {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let partial = dest.with_file_name(format!(
        "{}.partial",
        dest.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("pack.bin")
    ));
    let _ = fs::remove_file(&partial);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let expect = expect_sha256.to_string();
    let url = url.to_string();
    let partial_clone = partial.clone();
    let digest = rt.block_on(async move {
        let client = reqwest::Client::builder()
            .timeout(PACK_DOWNLOAD_TIMEOUT)
            .connect_timeout(CONNECTIVITY_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("GET {url}: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {} for {url}", resp.status().as_u16()));
        }
        let mut file = File::create(&partial_clone).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut written: u64 = 0;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("body: {e}"))?;
            hasher.update(&chunk);
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            written += chunk.len() as u64;
        }
        file.flush().map_err(|e| e.to_string())?;
        if let Some(n) = expect_bytes {
            if written != n {
                return Err(format!(
                    "size mismatch for {url}: got {written}, expect {n}"
                ));
            }
        }
        Ok((hex::encode(hasher.finalize()), written))
    })?;

    verify_hex(&digest.0, &expect)?;
    let _ = digest.1;
    fs::rename(&partial, dest).map_err(|e| format!("rename {}: {e}", dest.display()))?;
    Ok(())
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

/// Fetch, verify, remap to leaf stem, and install into `data_dir`.
pub fn try_fetch_region_packs(
    ready: &ReadyRegion,
    base_url: &str,
    data_dir: Option<&Path>,
) -> Result<(), String> {
    let data_dir = data_dir.ok_or_else(|| "pack fetch needs data_dir".to_string())?;
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;

    let region_id = normalize_region_id(&ready.region_id);
    let generation = ready
        .generation
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "ready region missing generation".to_string())?;

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

    let bake_stem = bake_stem_from_manifest(&client, ready);
    let leaf_stem = leaf_stem_for_region_id(&region_id);
    let pack_base = join_url(base_url, &format!("/packs/{region_id}/{generation}"));

    let staging = data_dir.join(format!(".pack-fetch-{leaf_stem}.partial"));
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let navi_name = client
        .navi_manifest
        .clone()
        .unwrap_or_else(|| format!("{bake_stem}.navi-manifest.json"));

    for (remote_name, meta) in &client.files {
        let url = join_url(&pack_base, remote_name);
        let staged = staging.join(remote_name);
        // Prefer streaming for large binaries; small JSON can use RAM path.
        let is_json = remote_name.ends_with(".json");
        if is_json {
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
        } else {
            http_download_verified(&url, &staged, &meta.sha256, meta.bytes)?;
        }
    }

    // Remap bake → leaf into final names under staging/out.
    let out = staging.join("out");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;

    for remote_name in client.files.keys() {
        let src = staging.join(remote_name);
        let dest_name = if remote_name == &navi_name || remote_name.ends_with(".navi-manifest.json")
        {
            let raw = fs::read(&src).map_err(|e| e.to_string())?;
            let rewritten = rewrite_navi_manifest_bytes(&raw, &bake_stem, &leaf_stem)?;
            let dest = out.join(format!("{leaf_stem}.navi-manifest.json"));
            fs::write(&dest, rewritten).map_err(|e| e.to_string())?;
            continue;
        } else {
            remap_filename(remote_name, &bake_stem, &leaf_stem)
        };
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
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        thread::spawn(move || {
            for _ in 0..64 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/");
                let key = path.trim_start_matches('/');
                let body = files.get(key).cloned().or_else(|| {
                    key.rsplit('/')
                        .next()
                        .and_then(|leaf| files.get(leaf).cloned())
                });
                if let Some(body) = body {
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.write_all(&body);
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
}
