//! Phase 7 host end-to-end: migration against a local fake CoMaps mirror + navi-server.
//!
//! Covers plan -> start -> run -> atomic switch -> deep verify, plus pause / resume with a Range
//! request, a dropped network, and a CoMaps release published while a migration runs. The
//! server is a small threaded HTTP/1.1 responder (Range supported); signatures are real Ed25519.

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use driver_break_core::mwm::checksum::{sha1_base64_of_bytes, sha256_hex_of_bytes};
use driver_break_core::mwm::migrate::{
    build_plan, fetch_plan_sources, read_state, record_old_maps_decision, run_migration,
    start_migration, update_offer, verify_migration, DetectedRkyvRegion, MigrationOutcome,
    MigrationStatus, OldMapsDecision, PlanInput,
};
use driver_break_core::mwm::{
    installed_leaf_ids, read_active_version, with_mwm_client_override, DownloadProgress,
};
use ed25519_dalek::{Signer, SigningKey};
use tempfile::tempdir;

const SERIES: &str = "2026.10.27";

#[derive(Default)]
struct Hits {
    paths: Mutex<Vec<(String, bool)>>, // (path, had_range)
}

struct FakeServer {
    port: u16,
    stop: Arc<AtomicBool>,
    /// Drop every connection without answering.
    refuse: Arc<AtomicBool>,
    /// Bytes of body served so far (all paths), for pause triggers.
    served_bytes: Arc<AtomicU64>,
    /// Sleep per 8 KiB chunk of an `.mwm` body, in ms.
    slow_ms: Arc<AtomicU64>,
    /// Version served at `/mwm/current.json`.
    current: Arc<Mutex<String>>,
    hits: Arc<Hits>,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl FakeServer {
    fn count(&self, needle: &str) -> usize {
        self.hits
            .paths
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| p.contains(needle))
            .count()
    }
    fn ranged(&self, needle: &str) -> usize {
        self.hits
            .paths
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, r)| p.contains(needle) && *r)
            .count()
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn leaf_bytes(id: &str, len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(len);
    let mut x: u32 = id
        .bytes()
        .fold(2166136261, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    for _ in 0..len {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        v.push((x >> 24) as u8);
    }
    v
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn handle(mut s: TcpStream, srv: &FakeServer) {
    let mut buf = [0u8; 4096];
    let mut req = Vec::new();
    loop {
        let n = match s.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        req.extend_from_slice(&buf[..n]);
        if req.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    if srv.refuse.load(Ordering::SeqCst) {
        return; // close without a response
    }
    let text = String::from_utf8_lossy(&req).into_owned();
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
    let path = percent_decode(&path);
    let mut range_from: Option<u64> = None;
    for l in lines {
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("range: bytes=") {
            range_from = v.trim_end_matches('-').parse().ok();
        }
    }
    srv.hits
        .paths
        .lock()
        .unwrap()
        .push((path.clone(), range_from.is_some()));

    let body: Option<Vec<u8>> = if path == "/mwm/current.json" {
        let ver = srv.current.lock().unwrap().clone();
        srv.files
            .lock()
            .unwrap()
            .get(&format!("/mwm/current-{ver}.json"))
            .cloned()
    } else {
        srv.files.lock().unwrap().get(&path).cloned()
    };
    let Some(body) = body else {
        let _ = s
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    let (status, start) = match range_from {
        Some(f) if (f as usize) < body.len() => ("206 Partial Content", f as usize),
        Some(_) => {
            let _ = s.write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            return;
        }
        None => ("200 OK", 0),
    };
    let part = &body[start..];
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nAccept-Ranges: bytes\r\n",
        part.len()
    );
    if start > 0 {
        head.push_str(&format!(
            "Content-Range: bytes {start}-{}/{}\r\n",
            body.len() - 1,
            body.len()
        ));
    }
    head.push_str("\r\n");
    if s.write_all(head.as_bytes()).is_err() {
        return;
    }
    let slow = path.ends_with(".mwm") && srv.slow_ms.load(Ordering::SeqCst) > 0;
    for chunk in part.chunks(8192) {
        if srv.stop.load(Ordering::SeqCst) {
            return;
        }
        if srv.refuse.load(Ordering::SeqCst) {
            return; // drop mid-body: "network lost"
        }
        if s.write_all(chunk).is_err() {
            return;
        }
        srv.served_bytes
            .fetch_add(chunk.len() as u64, Ordering::SeqCst);
        if slow {
            thread::sleep(Duration::from_millis(srv.slow_ms.load(Ordering::SeqCst)));
        }
    }
}

struct Fixture {
    server: Arc<FakeServer>,
    origin: String,
    leaf_ids: Vec<String>,
}

fn build_fixture(version: &str) -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let origin = format!("http://127.0.0.1:{port}");

    let sizes: &[(&str, usize)] = &[
        ("World", 40_000),
        ("WorldCoasts", 20_000),
        ("Norway_Hedmark", 600_000),
        ("Norway_Oppland", 500_000),
        ("Norway_Southern", 800_000),
        ("Denmark_North Denmark Region", 300_000),
        ("Norway_Rogaland", 400_000),
    ];
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut countries_nodes = Vec::new();
    let mut leaves = Vec::new();
    for (id, len) in sizes {
        let mwm = leaf_bytes(id, *len);
        let sha1 = sha1_base64_of_bytes(&mwm);
        let sha256 = sha256_hex_of_bytes(&mwm);
        countries_nodes.push(serde_json::json!({"id": id, "s": mwm.len(), "sha1_base64": sha1}));
        let enc = id.replace(' ', "%20");
        files.insert(format!("/maps/{SERIES}/{version}/{id}.mwm"), mwm.clone());
        let mut leaf = serde_json::json!({
            "id": id,
            "comaps_file": format!("{enc}.mwm"),
            "comaps_bytes": mwm.len(),
            "comaps_sha1_base64": sha1,
            "mwm_bytes": mwm.len(),
            "mwm_sha256": sha256,
            "mwm_fingerprint": {"size": mwm.len(), "sha256": sha256},
            "no_sidecar": id.starts_with("World"),
        });
        if !id.starts_with("World") {
            let side = leaf_bytes(&format!("side-{id}"), 5_000);
            leaf["sidecar_url"] = format!("{version}/{id}.navi-sidecar.v1.zst").into();
            leaf["sidecar_bytes"] = side.len().into();
            leaf["sidecar_sha256"] = sha256_hex_of_bytes(&side).into();
            files.insert(format!("/mwm/{version}/{id}.navi-sidecar.v1.zst"), side);
        }
        leaves.push(leaf);
    }
    let countries = serde_json::to_vec(&serde_json::json!({
        "id": "Countries", "v": version.parse::<u64>().unwrap(), "map_series": SERIES,
        "g": countries_nodes
    }))
    .unwrap();
    let signing = SigningKey::from_bytes(&[9u8; 32]);
    let sig = signing.sign(&countries);
    files.insert(
        format!("/maps/{SERIES}/{version}/countries.txt"),
        countries.clone(),
    );
    files.insert(
        format!("/maps/{SERIES}/{version}/countries.txt.sig"),
        sig.to_bytes().to_vec(),
    );
    files.insert(
        "/meta/maps.json".into(),
        serde_json::to_vec(&serde_json::json!({
            "map-series": {SERIES: {"latest": version.parse::<u64>().unwrap()}}
        }))
        .unwrap(),
    );
    files.insert(
        "/servers".into(),
        serde_json::to_vec(&vec![origin.clone()]).unwrap(),
    );
    files.insert(
        format!("/mwm/{version}/attribution.json"),
        b"{\"sources\":[]}".to_vec(),
    );
    let catalog = serde_json::json!({
        "schema": "navi-mwm-catalog/v2", "mode": "comaps-release", "version": version,
        "comaps": {
            "map_series": SERIES, "release_version": version.parse::<u64>().unwrap(),
            "countries_txt_sha256": sha256_hex_of_bytes(&countries),
            "countries_txt_bytes": countries.len(),
            "countries_txt_pubkey_hex": hex::encode(signing.verifying_key().as_bytes()),
            "metaserver_url": origin,
        },
        "leaves": leaves,
        "region_to_leaves": {},
        "non_routable_leaves": ["World", "WorldCoasts"],
        "region_migration_url": format!("{version}/region-migration.json"),
    });
    files.insert(
        format!("/mwm/current-{version}.json"),
        serde_json::to_vec(&catalog).unwrap(),
    );
    let mig = serde_json::json!({
        "schema": "navi-mwm-region-migration/v1", "comaps_release": version,
        "regions": {
            "europe_norway_ostlandet": {
                "region_id": "europe/norway/ostlandet", "display_name": "Ostlandet",
                "uncovered_pct": 0.04,
                "comaps_leaves": [
                    {"leaf": "Norway_Hedmark", "coverage_of_region_pct": 27.4},
                    {"leaf": "Norway_Southern", "coverage_of_region_pct": 25.5},
                    {"leaf": "Norway_Oppland", "coverage_of_region_pct": 20.0}
                ]
            },
            "europe_norway_sorlandet": {
                "region_id": "europe/norway/sorlandet", "display_name": "Sorlandet",
                "uncovered_pct": 1.6,
                "comaps_leaves": [
                    {"leaf": "Norway_Southern", "coverage_of_region_pct": 79.3},
                    {"leaf": "Denmark_North Denmark Region", "coverage_of_region_pct": 18.5,
                     "overspill_pct": 87.0},
                    {"leaf": "Norway_Rogaland", "coverage_of_region_pct": 5.0}
                ]
            }
        },
        "shared_leaves": {"Norway_Southern": ["europe_norway_ostlandet", "europe_norway_sorlandet"]}
    });
    files.insert(
        format!("/mwm/{version}/region-migration.json"),
        serde_json::to_vec(&mig).unwrap(),
    );

    let server = Arc::new(FakeServer {
        port,
        stop: Arc::new(AtomicBool::new(false)),
        refuse: Arc::new(AtomicBool::new(false)),
        served_bytes: Arc::new(AtomicU64::new(0)),
        slow_ms: Arc::new(AtomicU64::new(0)),
        current: Arc::new(Mutex::new(version.to_string())),
        hits: Arc::new(Hits::default()),
        files: Arc::new(Mutex::new(files)),
    });
    let srv = server.clone();
    thread::spawn(move || {
        for conn in listener.incoming() {
            if srv.stop.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(c) = conn {
                let s2 = srv.clone();
                thread::spawn(move || handle(c, &s2));
            }
        }
    });
    Fixture {
        server,
        origin,
        leaf_ids: sizes.iter().map(|(i, _)| i.to_string()).collect(),
    }
}

fn detected() -> Vec<DetectedRkyvRegion> {
    vec![
        DetectedRkyvRegion {
            region_id: "europe/norway/ostlandet".into(),
            old_bytes: 2_549_326_770,
        },
        DetectedRkyvRegion {
            region_id: "europe/norway/sorlandet".into(),
            old_bytes: 331_098_569,
        },
    ]
}

fn plan_and_start(
    data: &std::path::Path,
    fx: &Fixture,
) -> driver_break_core::mwm::migrate::MigrationPlan {
    let src = fetch_plan_sources(&fx.origin).expect("plan sources");
    let have: BTreeSet<String> = installed_leaf_ids(
        data,
        &read_active_version(data)
            .unwrap()
            .map(|a| a.version)
            .unwrap_or_default(),
    )
    .into_iter()
    .collect();
    let det = detected();
    let plan = build_plan(&PlanInput {
        catalog: &src.catalog,
        migration: &src.migration,
        detected: &det,
        selected_region_ids: None,
        excluded_leaves: &[],
        installed_leaves: &have,
        world_installed: false,
        free_bytes: u64::MAX,
    });
    start_migration(data, &plan, &src.catalog_text, false).expect("start");
    plan
}

#[test]
fn migrates_ostlandet_and_sorlandet_atomically_with_shared_leaf_once() {
    with_mwm_client_override(Some(true), || {
        let fx = build_fixture("261105");
        let dir = tempdir().unwrap();
        let data = dir.path();
        let plan = plan_and_start(data, &fx);
        assert_eq!(plan.leaves.len(), 5, "Southern shared, counted once");
        assert!(plan.world_needed);

        let mut p = DownloadProgress::default();
        let out = run_migration(data, &fx.origin, &mut p, None);
        assert!(matches!(out, MigrationOutcome::Verified { .. }), "{out:?}");
        // The shared leaf was fetched once.
        assert_eq!(fx.server.count("Norway_Southern.mwm"), 1);
        assert_eq!(fx.server.count("World.mwm"), 1);
        // Switched: active release, World at the root, every chosen leaf present.
        assert_eq!(
            read_active_version(data).unwrap().unwrap().version,
            "261105"
        );
        let have = installed_leaf_ids(data, "261105");
        assert_eq!(have.len(), 5);
        assert!(data.join("mwm/World.mwm").is_file());
        assert!(data.join("mwm/WorldCoasts.mwm").is_file());
        // Nothing left in staging, state is Verified.
        assert!(!data.join("mwm/staging/261105").exists());
        let st = read_state(data).unwrap();
        assert_eq!(st.status, MigrationStatus::Verified);
        verify_migration(data).expect("deep verify");

        // A corrupted leaf makes the verification fail (old maps must then not be offered).
        let leaf = data.join("mwm/releases/261105/leaves/Norway_Oppland.mwm");
        let mut bytes = std::fs::read(&leaf).unwrap();
        bytes[100] ^= 0xFF;
        std::fs::write(&leaf, bytes).unwrap();
        assert!(verify_migration(data).is_err());

        record_old_maps_decision(data, OldMapsDecision::Kept, 0).unwrap();
        assert_eq!(read_state(data).unwrap().status, MigrationStatus::Done);
    });
}

#[test]
fn pause_resumes_with_range_and_does_not_switch_early() {
    with_mwm_client_override(Some(true), || {
        let fx = build_fixture("261105");
        fx.server.slow_ms.store(8, Ordering::SeqCst);
        let dir = tempdir().unwrap();
        let data = dir.path().to_path_buf();
        plan_and_start(&data, &fx);

        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let served = fx.server.served_bytes.clone();
        // Pause once about 300 KB of the first big leaf has arrived.
        let watcher = thread::spawn(move || {
            while served.load(Ordering::SeqCst) < 300_000 {
                thread::sleep(Duration::from_millis(5));
            }
            c2.store(true, Ordering::SeqCst);
        });
        let mut p = DownloadProgress::default();
        let out = run_migration(&data, &fx.origin, &mut p, Some(&cancel));
        watcher.join().unwrap();
        assert_eq!(out, MigrationOutcome::Paused);
        assert_eq!(read_state(&data).unwrap().status, MigrationStatus::Paused);
        // Nothing switched: no active release, no World at the root, leaves only in staging.
        assert!(read_active_version(&data).unwrap().is_none());
        assert!(!data.join("mwm/World.mwm").exists());
        assert!(data.join("mwm/staging/261105").is_dir());
        let partials = walk(&data.join("mwm/staging/261105"))
            .into_iter()
            .filter(|p| p.ends_with(".partial"))
            .count();
        assert!(partials >= 1, "a .partial file must remain for resume");

        // Resume.
        fx.server.slow_ms.store(0, Ordering::SeqCst);
        let cancel2 = AtomicBool::new(false);
        let out = run_migration(&data, &fx.origin, &mut p, Some(&cancel2));
        assert!(matches!(out, MigrationOutcome::Verified { .. }), "{out:?}");
        assert!(
            fx.server.ranged(".mwm") >= 1,
            "resume must use a Range request"
        );
        verify_migration(&data).unwrap();
    });
}

fn walk(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p.to_string_lossy().into_owned());
            }
        }
    }
    out
}

#[test]
fn network_loss_waits_then_resumes() {
    with_mwm_client_override(Some(true), || {
        let fx = build_fixture("261105");
        fx.server.slow_ms.store(8, Ordering::SeqCst);
        let dir = tempdir().unwrap();
        let data = dir.path().to_path_buf();
        plan_and_start(&data, &fx);

        let refuse = fx.server.refuse.clone();
        let served = fx.server.served_bytes.clone();
        let watcher = thread::spawn(move || {
            while served.load(Ordering::SeqCst) < 300_000 {
                thread::sleep(Duration::from_millis(5));
            }
            refuse.store(true, Ordering::SeqCst); // cable out
        });
        let mut p = DownloadProgress::default();
        let out = run_migration(&data, &fx.origin, &mut p, None);
        watcher.join().unwrap();
        assert!(
            matches!(out, MigrationOutcome::WaitingNetwork(_)),
            "expected WaitingNetwork, got {out:?}"
        );
        assert_eq!(
            read_state(&data).unwrap().status,
            MigrationStatus::WaitingNetwork
        );
        assert!(read_active_version(&data).unwrap().is_none());

        // Network back.
        fx.server.refuse.store(false, Ordering::SeqCst);
        fx.server.slow_ms.store(0, Ordering::SeqCst);
        let out = run_migration(&data, &fx.origin, &mut p, None);
        assert!(matches!(out, MigrationOutcome::Verified { .. }), "{out:?}");
        verify_migration(&data).unwrap();
    });
}

#[test]
fn new_release_mid_migration_is_finished_first_then_offered() {
    with_mwm_client_override(Some(true), || {
        let fx = build_fixture("261105");
        let dir = tempdir().unwrap();
        let data = dir.path().to_path_buf();
        plan_and_start(&data, &fx);

        // navi-server publishes 261106 while the migration is queued.
        let newer = build_fixture("261106");
        {
            let mut files = fx.server.files.lock().unwrap();
            let new_files = newer.server.files.lock().unwrap();
            files.insert(
                "/mwm/current-261106.json".into(),
                new_files["/mwm/current-261106.json"].clone(),
            );
        }
        *fx.server.current.lock().unwrap() = "261106".into();

        let mut p = DownloadProgress::default();
        let out = run_migration(&data, &fx.origin, &mut p, None);
        match out {
            MigrationOutcome::Verified { ref version, .. } => assert_eq!(version, "261105"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            read_active_version(&data).unwrap().unwrap().version,
            "261105"
        );
        verify_migration(&data).unwrap();

        // Now the newer release is offered (not installed).
        let (cat, _) = driver_break_core::mwm::fetch_current_catalog_text(&fx.origin).unwrap();
        assert_eq!(cat.version, "261106");
        let offer = update_offer(&data, &cat, Some("261105"));
        assert!(offer.update_available);
        assert!(offer.update_bytes > 0);
        assert_eq!(
            read_active_version(&data).unwrap().unwrap().version,
            "261105"
        );
    });
}

#[test]
fn second_migration_start_is_refused_until_done() {
    with_mwm_client_override(Some(true), || {
        let fx = build_fixture("261105");
        let dir = tempdir().unwrap();
        let data = dir.path();
        let plan = plan_and_start(data, &fx);
        let src = fetch_plan_sources(&fx.origin).unwrap();
        assert!(start_migration(data, &plan, &src.catalog_text, true).is_err());
        let _ = &fx.leaf_ids;
    });
}
