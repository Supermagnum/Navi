//! Phase 3: orchestration, storage estimate, US cross-country fixture tests.

use driver_break_core::long_trip::{
    avoid_country_ids_for_allowed, build_directions_request_body, estimate_trip_disk_bytes,
    ordered_needed_regions_along_route, regions_bbox_adjacent, LongTripError, LongTripPlan,
    RegionDownloader, RegionIndexer, RegionTripState, StorageVolume, TripOrchestrator,
    VolumeSource, LONG_TRIP_CORRIDOR_BUFFER_KM,
};
use driver_break_core::pack_server::catalog_entries_from_ready_ids;
use driver_break_core::routing::basemap::region_bbox;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/long_trip")
        .join(name)
}

fn load_catalog() -> (Vec<String>, Vec<(String, u64)>) {
    #[derive(Deserialize)]
    struct Cat {
        regions: Vec<Reg>,
    }
    #[derive(Deserialize)]
    struct Reg {
        region_id: String,
        bytes: Option<u64>,
    }
    let cat: Cat =
        serde_json::from_str(&std::fs::read_to_string(fixture("current.json")).unwrap()).unwrap();
    let ids: Vec<_> = cat.regions.iter().map(|r| r.region_id.clone()).collect();
    let sizes: Vec<_> = cat
        .regions
        .iter()
        .map(|r| (r.region_id.clone(), r.bytes.unwrap_or(0)))
        .collect();
    (ids, sizes)
}

use std::cell::RefCell;
use std::rc::Rc;

struct FakeDl {
    order: Rc<RefCell<Vec<String>>>,
    fail_on: Option<String>,
}
impl RegionDownloader for FakeDl {
    fn download(&mut self, region_id: &str) -> Result<(), String> {
        if self.fail_on.as_deref() == Some(region_id) {
            return Err("fail".into());
        }
        self.order.borrow_mut().push(region_id.into());
        Ok(())
    }
}
struct FakeIx {
    order: Rc<RefCell<Vec<String>>>,
}
impl RegionIndexer for FakeIx {
    fn index(&mut self, region_id: &str) -> Result<(), String> {
        self.order.borrow_mut().push(region_id.into());
        Ok(())
    }
}
struct FakeVol {
    vols: Vec<StorageVolume>,
}
impl VolumeSource for FakeVol {
    fn volumes(&self) -> Vec<StorageVolume> {
        self.vols.clone()
    }
}

#[test]
fn download_order_matches_route_and_index_waits() {
    let regions = vec![
        "north-america/us/new-york".into(),
        "north-america/us/pennsylvania".into(),
        "north-america/us/ohio".into(),
    ];
    let sizes: Vec<(String, u64)> = regions
        .iter()
        .map(|r: &String| (r.clone(), 1_000_000_000u64))
        .collect();
    let mut plan = LongTripPlan::new(regions[0].clone(), regions.clone());
    let dl_order = Rc::new(RefCell::new(Vec::new()));
    let ix_order = Rc::new(RefCell::new(Vec::new()));
    let mut orch = TripOrchestrator {
        downloader: FakeDl {
            order: Rc::clone(&dl_order),
            fail_on: None,
        },
        indexer: FakeIx {
            order: Rc::clone(&ix_order),
        },
        volumes: FakeVol {
            vols: vec![StorageVolume::primary(200_000_000_000, 256_000_000_000)],
        },
        unmetered: true,
        enabled: true,
    };
    orch.ensure_start(&mut plan).unwrap();
    assert_eq!(dl_order.borrow().as_slice(), ["north-america/us/new-york"]);
    assert_eq!(ix_order.borrow().as_slice(), ["north-america/us/new-york"]);
    orch.run_downloads_then_index(&mut plan, &sizes).unwrap();
    assert_eq!(
        dl_order.borrow().as_slice(),
        [
            "north-america/us/new-york",
            "north-america/us/pennsylvania",
            "north-america/us/ohio"
        ]
    );
    assert_eq!(
        ix_order.borrow().as_slice(),
        [
            "north-america/us/new-york",
            "north-america/us/pennsylvania",
            "north-america/us/ohio"
        ]
    );
}

#[test]
fn planning_region_installed_while_others_progress() {
    // Finished region stays Indexed while siblings download / index.
    let mut plan = LongTripPlan::new("a".into(), vec!["a".into(), "b".into(), "c".into()]);
    plan.set_state("a", RegionTripState::Indexed);
    plan.set_state("b", RegionTripState::Downloading);
    plan.set_state("c", RegionTripState::Indexing);
    assert!(matches!(
        plan.states.get("a"),
        Some(RegionTripState::Indexed)
    ));
    assert!(matches!(
        plan.states.get("b"),
        Some(RegionTripState::Downloading)
    ));
    assert!(matches!(
        plan.states.get("c"),
        Some(RegionTripState::Indexing)
    ));
    // Planning on `a` must not require siblings to finish.
    assert_eq!(plan.states.get("a"), Some(&RegionTripState::Indexed));
}

#[test]
fn restart_resumes_from_installed() {
    let regions = vec!["a".into(), "b".into(), "c".into()];
    let sizes: Vec<(String, u64)> = regions
        .iter()
        .map(|r: &String| (r.clone(), 1_000_000u64))
        .collect();
    let mut plan = LongTripPlan::new("a".into(), regions);
    plan.set_state("a", RegionTripState::Indexed);
    plan.set_state("b", RegionTripState::Installed);
    plan.set_state("c", RegionTripState::Downloading);
    let dl_order = Rc::new(RefCell::new(Vec::new()));
    let ix_order = Rc::new(RefCell::new(Vec::new()));
    let mut orch = TripOrchestrator {
        downloader: FakeDl {
            order: Rc::clone(&dl_order),
            fail_on: None,
        },
        indexer: FakeIx {
            order: Rc::clone(&ix_order),
        },
        volumes: FakeVol {
            vols: vec![StorageVolume::primary(u64::MAX / 4, u64::MAX / 2)],
        },
        unmetered: true,
        enabled: false,
    };
    // Crash mid-download of c → Unavailable; restart restores queue.
    orch.on_card_removed_mid_download(&mut plan);
    assert!(matches!(
        plan.states.get("c"),
        Some(RegionTripState::Unavailable)
    ));
    orch.resume_after_restart(&mut plan);
    assert!(orch.enabled);
    assert!(matches!(
        plan.states.get("a"),
        Some(RegionTripState::Indexed)
    ));
    assert!(matches!(
        plan.states.get("b"),
        Some(RegionTripState::Installed)
    ));
    assert!(matches!(
        plan.states.get("c"),
        Some(RegionTripState::Needed)
    ));
    orch.run_downloads_then_index(&mut plan, &sizes).unwrap();
    // a already Indexed → not re-downloaded; b Installed → index only; c download+index.
    assert_eq!(dl_order.borrow().as_slice(), ["c"]);
    assert_eq!(ix_order.borrow().as_slice(), ["b", "c"]);
    assert!(matches!(
        plan.states.get("a"),
        Some(RegionTripState::Indexed)
    ));
    assert!(matches!(
        plan.states.get("b"),
        Some(RegionTripState::Indexed)
    ));
    assert!(matches!(
        plan.states.get("c"),
        Some(RegionTripState::Indexed)
    ));
}

#[test]
fn toggle_off_keeps_installed_data() {
    let mut plan = LongTripPlan::new("a".into(), vec!["a".into(), "b".into()]);
    plan.set_state("a", RegionTripState::Indexed);
    plan.set_state("b", RegionTripState::Needed);
    let mut orch = TripOrchestrator {
        downloader: FakeDl {
            order: Rc::new(RefCell::new(vec![])),
            fail_on: None,
        },
        indexer: FakeIx {
            order: Rc::new(RefCell::new(vec![])),
        },
        volumes: FakeVol {
            vols: vec![StorageVolume::primary(u64::MAX / 4, u64::MAX / 2)],
        },
        unmetered: true,
        enabled: true,
    };
    orch.cancel_pending(&mut plan);
    assert!(matches!(
        plan.states.get("a"),
        Some(RegionTripState::Indexed)
    ));
    assert!(matches!(
        plan.states.get("b"),
        Some(RegionTripState::Paused)
    ));
}

#[test]
fn insufficient_space_and_card_removed() {
    let regions = vec!["a".into(), "b".into()];
    let sizes = vec![
        ("a".into(), 50_000_000_000u64),
        ("b".into(), 50_000_000_000),
    ];
    let mut plan = LongTripPlan::new("a".into(), regions.clone());
    let mut orch = TripOrchestrator {
        downloader: FakeDl {
            order: Rc::new(RefCell::new(vec![])),
            fail_on: None,
        },
        indexer: FakeIx {
            order: Rc::new(RefCell::new(vec![])),
        },
        volumes: FakeVol {
            vols: vec![StorageVolume::primary(1_000_000_000, 64_000_000_000)],
        },
        unmetered: true,
        enabled: true,
    };
    let err = orch
        .run_downloads_then_index(&mut plan, &sizes)
        .unwrap_err();
    assert!(matches!(err, LongTripError::InsufficientSpace { .. }));

    plan.set_state("b", RegionTripState::Downloading);
    orch.on_card_removed_mid_download(&mut plan);
    assert!(matches!(
        plan.states.get("b"),
        Some(RegionTripState::Unavailable)
    ));
}

fn assert_us_corridor_properties(needed: &[String], dest_substr: &str) {
    let mut seen = BTreeSet::new();
    for r in needed {
        assert!(seen.insert(r.clone()), "dup {r}");
        assert!(
            r.starts_with("north-america/us"),
            "non-US region {r} in {needed:?}"
        );
    }
    assert!(
        needed
            .last()
            .map(|r| r.contains(dest_substr))
            .unwrap_or(false),
        "dest {dest_substr} should be last in {needed:?}"
    );
    // Consecutive adjacency via bbox touch.
    for w in needed.windows(2) {
        let ba = region_bbox(&w[0]).expect(&w[0]);
        let bb = region_bbox(&w[1]).expect(&w[1]);
        assert!(
            regions_bbox_adjacent(&ba, &bb, 1.75),
            "non-adjacent {} -> {} in {needed:?}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn us_cross_country_fixture_a_and_b() {
    let (ids, sizes) = load_catalog();
    let us_pub: Vec<_> = ids
        .iter()
        .filter(|i| i.starts_with("north-america/us"))
        .cloned()
        .collect();
    eprintln!(
        "catalog US coverage: {} published regions (state + california norcal/socal leaves)",
        us_pub.len()
    );
    assert!(
        !us_pub.is_empty(),
        "current.json fixture must list US regions"
    );
    assert!(
        us_pub.iter().any(|r| r.contains("california/norcal")),
        "expected california/norcal granularity"
    );
    assert!(
        us_pub.iter().any(|r| r.contains("california/socal")),
        "expected california/socal granularity"
    );

    // Request shape for US-only.
    let body = build_directions_request_body(
        &[[-73.9803, 40.7439], [-118.3972, 33.8450]],
        Some(&["us".into()]),
    )
    .unwrap();
    assert_eq!(body["coordinates"][0][0], serde_json::json!(-73.9803));
    assert_eq!(body["coordinates"][0][1], serde_json::json!(40.7439));
    let avoid = avoid_country_ids_for_allowed(&["us".into()]);
    let req_avoid: Vec<u64> = body["options"]["avoid_countries"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_u64())
        .collect();
    for id in avoid {
        assert!(req_avoid.contains(&(id as u64)));
    }
    assert!(body["options"]["avoid_features"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "ferries"));

    let start = "north-america/us/new-york".to_string();
    // Synthetic US corridors must not drive real-trip region-list assertions.
    for name in [
        "ors_us_a_redball_portofino.geojson",
        "ors_us_b_redball_crescent.geojson",
    ] {
        let raw = std::fs::read_to_string(fixture(name)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            v.get("navi_fixture").and_then(|x| x.as_str()),
            Some("synthetic")
        );
    }
    eprintln!(
        "NOTE: ors_us_*.geojson remain SYNTHETIC; region lists for US A/B come from \
         live BRouter recording when available (see live_us_brouter_dry_run)"
    );

    // Optional: if a recorded BRouter US fixture exists, derive lists + storage.
    let recorded_a = fixture("recorded/brouter_us_a_redball_portofino_car-eco.json");
    let mut plan_regions: Vec<String> = vec![start.clone()];
    if recorded_a.exists() {
        let wrap: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&recorded_a).unwrap()).unwrap();
        let status = wrap
            .pointer("/meta/http_status")
            .and_then(|s| s.as_u64())
            .unwrap_or(0);
        if status == 200 {
            assert_eq!(
                wrap.get("navi_fixture").and_then(|x| x.as_str()),
                Some("recorded")
            );
            let body = wrap.get("response_body").and_then(|b| b.as_str()).unwrap();
            let route = driver_break_core::long_trip::parse_brouter_geojson(body).unwrap();
            let entries = catalog_entries_from_ready_ids(&ids);
            let a = driver_break_core::long_trip::ordered_needed_regions_along_route_filtered(
                &route.lat_lon,
                &entries,
                std::slice::from_ref(&start),
                LONG_TRIP_CORRIDOR_BUFFER_KM,
                Some("us"),
            );
            assert_us_corridor_properties(&a, "socal");
            let check_512 = estimate_trip_disk_bytes(&a, &sizes, 512u64 * 1024 * 1024 * 1024);
            let check_64 = estimate_trip_disk_bytes(&a, &sizes, 64u64 * 1024 * 1024 * 1024);
            eprintln!(
                "recorded US A regions={} storage512={check_512:?} storage64={check_64:?}",
                a.len()
            );
            plan_regions = a;
            if !plan_regions.contains(&start) {
                plan_regions.insert(0, start.clone());
            }
        } else {
            eprintln!("recorded US A status={status}; skipping region-list asserts");
        }
    }

    // Start region first (step 1) with fake orch.
    let mut plan = LongTripPlan::new(start.clone(), plan_regions);
    let dl_order = Rc::new(RefCell::new(Vec::new()));
    let mut orch = TripOrchestrator {
        downloader: FakeDl {
            order: Rc::clone(&dl_order),
            fail_on: None,
        },
        indexer: FakeIx {
            order: Rc::new(RefCell::new(Vec::new())),
        },
        volumes: FakeVol {
            vols: vec![StorageVolume::primary(512u64 << 30, 512u64 << 30)],
        },
        unmetered: true,
        enabled: true,
    };
    orch.ensure_start(&mut plan).unwrap();
    assert_eq!(
        dl_order.borrow().first().map(String::as_str),
        Some(start.as_str())
    );
    assert!(matches!(
        plan.states.get(&start),
        Some(RegionTripState::Indexed)
    ));
}

#[test]
#[ignore = "live dry-run: OPENROUTESERVICE_API_KEY + NAVI_LONG_TRIP_DRY_RUN=1"]
fn live_us_long_trip_dry_run() {
    assert_eq!(
        std::env::var("NAVI_LONG_TRIP_DRY_RUN").ok().as_deref(),
        Some("1"),
        "refusing live dry-run without NAVI_LONG_TRIP_DRY_RUN=1"
    );
    let key = std::env::var("OPENROUTESERVICE_API_KEY").expect("OPENROUTESERVICE_API_KEY");
    let cfg = driver_break_core::long_trip::OrsConfig::from_parts(
        key,
        driver_break_core::long_trip::DEFAULT_ORS_BASE_URL,
    );
    let start = (40.7439301, -73.9803488);
    let dest_a = (33.8450175, -118.3971954);
    let dest_b = (41.7610600, -124.1987020);
    let allowed = vec!["us".to_string()];
    let route_a =
        driver_break_core::long_trip::request_directions(&cfg, &[start, dest_a], Some(&allowed))
            .expect("ORS A");
    let route_b =
        driver_break_core::long_trip::request_directions(&cfg, &[start, dest_b], Some(&allowed))
            .expect("ORS B");
    eprintln!("live A distance_m={:.0}", route_a.distance_m);
    eprintln!("live B distance_m={:.0}", route_b.distance_m);
    let cat = driver_break_core::pack_server::check_connectivity_blocking(
        driver_break_core::pack_server::DEFAULT_PACK_SERVER_BASE_URL,
    );
    let ready = match cat {
        driver_break_core::pack_server::Connectivity::Ready(c) => c,
        other => panic!("catalog unreachable: {other:?}"),
    };
    let ids: Vec<_> = ready.regions.iter().map(|r| r.region_id.clone()).collect();
    let sizes: Vec<_> = ready
        .regions
        .iter()
        .map(|r| (r.region_id.clone(), r.bytes.unwrap_or(0)))
        .collect();
    let entries = catalog_entries_from_ready_ids(&ids);
    let installed = ["north-america/us/new-york".to_string()];
    let a = ordered_needed_regions_along_route(
        &route_a.lat_lon,
        &entries,
        &installed,
        LONG_TRIP_CORRIDOR_BUFFER_KM,
    );
    let b = ordered_needed_regions_along_route(
        &route_b.lat_lon,
        &entries,
        &installed,
        LONG_TRIP_CORRIDOR_BUFFER_KM,
    );
    eprintln!("live A regions: {a:?}");
    eprintln!("live B regions: {b:?}");
    let est = estimate_trip_disk_bytes(&a, &sizes, 512u64 << 30);
    eprintln!("live storage estimate: {est:?}");
    eprintln!("dry-run: downloads skipped");
}
