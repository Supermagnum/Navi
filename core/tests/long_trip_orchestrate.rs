//! Phase 3: orchestration, storage estimate, US cross-country fixture tests.

use driver_break_core::long_trip::{
    avoid_country_ids_for_allowed, build_directions_request_body, classify_catalog_coverage,
    estimate_trip_disk_bytes, ordered_needed_regions_along_route, parse_directions_geojson,
    regions_bbox_adjacent, CatalogCoverage, LongTripError, LongTripPlan, RegionDownloader,
    RegionIndexer, RegionTripState, SpaceCheck, StorageVolume, TripOrchestrator, VolumeSource,
    LONG_TRIP_CORRIDOR_BUFFER_KM,
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

fn needed_from_ors(name: &str, installed: &[String]) -> (Vec<String>, f64) {
    let route = parse_directions_geojson(&std::fs::read_to_string(fixture(name)).unwrap()).unwrap();
    let (ids, _) = load_catalog();
    let entries = catalog_entries_from_ready_ids(&ids);
    let needed = driver_break_core::long_trip::ordered_needed_regions_along_route_filtered(
        &route.lat_lon,
        &entries,
        installed,
        LONG_TRIP_CORRIDOR_BUFFER_KM,
        Some("us"),
    );
    (needed, route.distance_m)
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
    let mut plan = LongTripPlan::new("a".into(), vec!["a".into(), "b".into(), "c".into()]);
    plan.set_state("a", RegionTripState::Indexed);
    plan.set_state("b", RegionTripState::Downloading);
    plan.set_state("c", RegionTripState::Indexing);
    // Installed/Indexed ⇒ routable/searchable independently of siblings.
    assert!(matches!(
        plan.states.get("a"),
        Some(RegionTripState::Indexed)
    ));
    assert!(matches!(
        plan.states.get("b"),
        Some(RegionTripState::Downloading)
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
    let (a, dist_a) =
        needed_from_ors("ors_us_a_redball_portofino.geojson", std::slice::from_ref(&start));
    let (b, dist_b) =
        needed_from_ors("ors_us_b_redball_crescent.geojson", std::slice::from_ref(&start));
    eprintln!("route A distance_m={dist_a:.0}");
    eprintln!("route B distance_m={dist_b:.0}");
    if dist_a > driver_break_core::long_trip::ORS_MAX_DISTANCE_M {
        panic!("fixture A exceeds ORS max — would be RequestTooLarge");
    }

    // Drop empties if catalog PIP missed (report gaps).
    let cov_a = classify_catalog_coverage(&a, &ids, Some("us"));
    let cov_b = classify_catalog_coverage(&b, &ids, Some("us"));
    eprintln!("catalog coverage A={cov_a:?}");
    eprintln!("catalog coverage B={cov_b:?}");
    match &cov_a {
        CatalogCoverage::NotPublished { regions, fallback } => {
            for r in regions {
                eprintln!("not published: {r} fallback={fallback}");
            }
        }
        CatalogCoverage::NoCountryCoverage { country_iso } => {
            panic!("typed NoCountryCoverage for {country_iso}");
        }
        CatalogCoverage::Complete => {}
    }

    assert_us_corridor_properties(&a, "socal");
    assert_us_corridor_properties(&b, "norcal");

    eprintln!("region list A ({}):", a.len());
    for r in &a {
        eprintln!("  {r}");
    }
    eprintln!("region list B ({}):", b.len());
    for r in &b {
        eprintln!("  {r}");
    }
    let set_a: BTreeSet<_> = a.iter().collect();
    let set_b: BTreeSet<_> = b.iter().collect();
    eprintln!(
        "only in A: {:?}",
        set_a.difference(&set_b).collect::<Vec<_>>()
    );
    eprintln!(
        "only in B: {:?}",
        set_b.difference(&set_a).collect::<Vec<_>>()
    );

    // Storage estimate vs fake volumes.
    let check_512 = estimate_trip_disk_bytes(&a, &sizes, 512u64 * 1024 * 1024 * 1024);
    let check_64 = estimate_trip_disk_bytes(&a, &sizes, 64u64 * 1024 * 1024 * 1024);
    match &check_512 {
        SpaceCheck::Ok(r) => eprintln!(
            "512GiB: OK packs={} place_index={} pbf={} needed={}",
            r.pack_bytes, r.place_index_bytes, r.pbf_keep_bytes, r.needed_bytes
        ),
        SpaceCheck::InsufficientSpace { report, .. } => {
            eprintln!("512GiB: Insufficient needed={}", report.needed_bytes)
        }
    }
    match &check_64 {
        SpaceCheck::Ok(r) => eprintln!(
            "64GiB: OK needed={} (packs={})",
            r.needed_bytes, r.pack_bytes
        ),
        SpaceCheck::InsufficientSpace {
            needed,
            free,
            shortfall,
            report,
        } => eprintln!(
            "64GiB: InsufficientSpace needed={needed} free={free} shortfall={shortfall} packs={}",
            report.pack_bytes
        ),
    }
    // Always exercise shortfall: free = needed - 1 GiB.
    let needed = match &check_512 {
        SpaceCheck::Ok(r) | SpaceCheck::InsufficientSpace { report: r, .. } => r.needed_bytes,
    };
    let free_short = needed.saturating_sub(1024 * 1024 * 1024);
    match estimate_trip_disk_bytes(&a, &sizes, free_short) {
        SpaceCheck::InsufficientSpace { shortfall, .. } => {
            assert!(shortfall >= 1024 * 1024 * 1024 - 1);
        }
        SpaceCheck::Ok(_) => panic!("expected shortfall at free=needed-1GiB"),
    }
    // 64 GiB / 512 GiB volume matrix.
    match check_64 {
        SpaceCheck::InsufficientSpace { .. } => eprintln!("64GiB: shortfall as expected"),
        SpaceCheck::Ok(_) => eprintln!("64GiB: OK (estimate fits)"),
    }
    match check_512 {
        SpaceCheck::Ok(_) => {}
        SpaceCheck::InsufficientSpace { .. } => panic!("512GiB should fit this fixture estimate"),
    }

    // Destination entry note (report-only): no place index for uninstalled dest.
    eprintln!(
        "destination entry: app today requires map long-press / coordinates or a prior \
         place-index hit; uninstalled dest regions cannot be searched by name. \
         This test uses coordinates from Nominatim (see us_endpoints.json)."
    );

    // Start region first (step 1) with fake orch.
    let mut plan_regions = a.clone();
    if !plan_regions.contains(&start) {
        plan_regions.insert(0, start.clone());
    }
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
    let _ = b;
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
