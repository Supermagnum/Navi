//! Preliminary router policy: BRouter primary, ORS fallback (fake providers).

use driver_break_core::long_trip::{
    count_ferry_segments_in_messages, parse_brouter_geojson, request_preliminary_with_fetchers,
    BrouterError, BrouterFetcher, BrouterRoute, OrsError, OrsFetcher, OrsRoute, PreliminaryCache,
    PreliminaryError, RouteProviderId, BROUTER_CAR_PROFILE, FERRY_WARNING,
    PRELIMINARY_ROUTE_DISCLOSURE,
};
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

fn recorded(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/long_trip/recorded")
        .join(name)
}

fn load_recorded_raw_body(name: &str) -> String {
    let wrap: Value =
        serde_json::from_str(&std::fs::read_to_string(recorded(name)).unwrap()).unwrap();
    wrap.get("response_body")
        .and_then(|b| b.as_str())
        .unwrap()
        .to_string()
}

fn assert_not_synthetic_for_real_region_list(path: &std::path::Path) {
    let text = std::fs::read_to_string(path).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let stamp = v
        .get("navi_fixture")
        .and_then(|x| x.as_str())
        .or_else(|| v.pointer("/meta/navi_fixture").and_then(|x| x.as_str()));
    assert_ne!(
        stamp,
        Some("synthetic"),
        "refusing region-list assertion for a real trip on synthetic fixture {}",
        path.display()
    );
    assert_eq!(
        stamp,
        Some("recorded"),
        "real-trip region list requires navi_fixture=recorded, got {stamp:?} in {}",
        path.display()
    );
}

struct FakeBrouter {
    calls: Rc<RefCell<usize>>,
    result: Result<BrouterRoute, BrouterError>,
}
impl BrouterFetcher for FakeBrouter {
    fn fetch(&mut self, _wp: &[(f64, f64)]) -> Result<BrouterRoute, BrouterError> {
        *self.calls.borrow_mut() += 1;
        self.result.clone()
    }
}

struct FakeOrs {
    calls: Rc<RefCell<usize>>,
    result: Result<OrsRoute, OrsError>,
}
impl OrsFetcher for FakeOrs {
    fn fetch(
        &mut self,
        _wp: &[(f64, f64)],
        _allowed: Option<&[String]>,
    ) -> Result<OrsRoute, OrsError> {
        *self.calls.borrow_mut() += 1;
        self.result.clone()
    }
}

fn sample_wp() -> Vec<(f64, f64)> {
    vec![(53.334, 10.045), (61.5929077, 10.3318551)]
}

fn ok_brouter(ferry: usize) -> BrouterRoute {
    BrouterRoute {
        lat_lon: vec![(53.334, 10.045), (55.0, 10.0), (61.59, 10.33)],
        distance_m: 1_181_942.0,
        duration_s: Some(60_374.0),
        ferry_segments: ferry,
    }
}

fn ok_ors() -> OrsRoute {
    OrsRoute {
        lat_lon: vec![(53.334, 10.045), (61.59, 10.33)],
        distance_m: 1_200_000.0,
    }
}

#[test]
fn profile_is_car_eco_after_klecken_comparison() {
    assert_eq!(BROUTER_CAR_PROFILE, "car-eco");
    // Evidence: recorded car-eco has ferries; car-fast probe was watchdog-killed.
    let eco = load_recorded_raw_body("brouter_klecken_innlandet_car-eco.json");
    let route = parse_brouter_geojson(&eco).unwrap();
    assert_eq!(route.ferry_segments, 4);
    let fast_wrap: Value = serde_json::from_str(
        &std::fs::read_to_string(recorded("brouter_klecken_innlandet_car-fast.json")).unwrap(),
    )
    .unwrap();
    let status = fast_wrap
        .pointer("/meta/http_status")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);
    let body = fast_wrap
        .get("response_body")
        .and_then(|b| b.as_str())
        .unwrap_or("");
    assert_eq!(status, 400);
    assert!(
        body.to_ascii_lowercase().contains("watchdog"),
        "car-fast evidence should be watchdog kill, got {body}"
    );
}

#[test]
fn parse_recorded_klecken_and_kautokeino() {
    for name in [
        "brouter_klecken_innlandet_car-eco.json",
        "brouter_kautokeino_roros_car-eco.json",
    ] {
        assert_not_synthetic_for_real_region_list(&recorded(name));
        let body = load_recorded_raw_body(name);
        let route = parse_brouter_geojson(&body).unwrap();
        assert!(route.lat_lon.len() > 100, "{name} geometry");
        assert!(route.distance_m > 1_000_000.0, "{name} distance");
    }
}

#[test]
fn ferry_detection_uses_waytags_column() {
    let body = load_recorded_raw_body("brouter_klecken_innlandet_car-eco.json");
    let v: Value = serde_json::from_str(&body).unwrap();
    let props = v.pointer("/features/0/properties");
    assert_eq!(count_ferry_segments_in_messages(props), 4);
}

#[test]
fn restricted_uses_ors_only() {
    let b_calls = Rc::new(RefCell::new(0usize));
    let o_calls = Rc::new(RefCell::new(0usize));
    let mut b = FakeBrouter {
        calls: Rc::clone(&b_calls),
        result: Ok(ok_brouter(0)),
    };
    let mut o = FakeOrs {
        calls: Rc::clone(&o_calls),
        result: Ok(ok_ors()),
    };
    let mut cache = PreliminaryCache::new();
    let allowed = vec!["no".into()];
    let route = request_preliminary_with_fetchers(
        &sample_wp(),
        Some(&allowed),
        &mut b,
        &mut o,
        true,
        &mut cache,
    )
    .unwrap();
    assert_eq!(route.provider, RouteProviderId::Ors);
    assert_eq!(*b_calls.borrow(), 0);
    assert_eq!(*o_calls.borrow(), 1);
}

#[test]
fn restricted_no_key_is_no_api_key() {
    let mut b = FakeBrouter {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_brouter(0)),
    };
    let mut o = FakeOrs {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_ors()),
    };
    let mut cache = PreliminaryCache::new();
    let allowed = vec!["us".into()];
    let err = request_preliminary_with_fetchers(
        &sample_wp(),
        Some(&allowed),
        &mut b,
        &mut o,
        false,
        &mut cache,
    )
    .unwrap_err();
    match err {
        PreliminaryError::NoApiKey { message } => {
            assert!(message.to_ascii_lowercase().contains("country"));
            assert!(message.to_ascii_lowercase().contains("ors") || message.contains("OpenRoute"));
        }
        other => panic!("expected NoApiKey, got {other:?}"),
    }
    assert_eq!(*b.calls.borrow(), 0);
}

#[test]
fn unrestricted_uses_brouter() {
    let mut b = FakeBrouter {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_brouter(0)),
    };
    let mut o = FakeOrs {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_ors()),
    };
    let mut cache = PreliminaryCache::new();
    let route =
        request_preliminary_with_fetchers(&sample_wp(), None, &mut b, &mut o, true, &mut cache)
            .unwrap();
    assert_eq!(route.provider, RouteProviderId::Brouter);
    assert_eq!(*b.calls.borrow(), 1);
    assert_eq!(*o.calls.borrow(), 0);
}

#[test]
fn fallback_triggers_call_ors() {
    let triggers = [
        BrouterError::RateLimited,
        BrouterError::Timeout,
        BrouterError::ServerError { status: 500 },
        BrouterError::InvalidResponse("bogus".into()),
    ];
    for err in triggers {
        let mut b = FakeBrouter {
            calls: Rc::new(RefCell::new(0)),
            result: Err(err),
        };
        let mut o = FakeOrs {
            calls: Rc::new(RefCell::new(0)),
            result: Ok(ok_ors()),
        };
        let mut cache = PreliminaryCache::new();
        let route =
            request_preliminary_with_fetchers(&sample_wp(), None, &mut b, &mut o, true, &mut cache)
                .unwrap();
        assert_eq!(route.provider, RouteProviderId::Ors);
        assert_eq!(*o.calls.borrow(), 1);
    }
}

#[test]
fn ferry_with_key_falls_back_to_ors() {
    let mut b = FakeBrouter {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_brouter(4)),
    };
    let mut o = FakeOrs {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_ors()),
    };
    let mut cache = PreliminaryCache::new();
    let route =
        request_preliminary_with_fetchers(&sample_wp(), None, &mut b, &mut o, true, &mut cache)
            .unwrap();
    assert_eq!(route.provider, RouteProviderId::Ors);
    assert_eq!(*o.calls.borrow(), 1);
}

#[test]
fn ferry_without_key_returns_route_with_warning() {
    let mut b = FakeBrouter {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_brouter(2)),
    };
    let mut o = FakeOrs {
        calls: Rc::new(RefCell::new(0)),
        result: Err(OrsError::NoApiKey),
    };
    let mut cache = PreliminaryCache::new();
    let route =
        request_preliminary_with_fetchers(&sample_wp(), None, &mut b, &mut o, false, &mut cache)
            .unwrap();
    assert_eq!(route.provider, RouteProviderId::Brouter);
    assert_eq!(route.ferry_segments, 2);
    assert!(route
        .warnings
        .iter()
        .any(|w| w.contains("ferry") || w == FERRY_WARNING));
    assert_eq!(*o.calls.borrow(), 0);
}

#[test]
fn rate_limit_without_key_surfaces_typed_error() {
    let mut b = FakeBrouter {
        calls: Rc::new(RefCell::new(0)),
        result: Err(BrouterError::RateLimited),
    };
    let mut o = FakeOrs {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_ors()),
    };
    let mut cache = PreliminaryCache::new();
    let err =
        request_preliminary_with_fetchers(&sample_wp(), None, &mut b, &mut o, false, &mut cache)
            .unwrap_err();
    assert!(matches!(err, PreliminaryError::RateLimited));
    assert_eq!(*o.calls.borrow(), 0);
}

#[test]
fn caching_skips_second_network() {
    let mut b = FakeBrouter {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_brouter(0)),
    };
    let mut o = FakeOrs {
        calls: Rc::new(RefCell::new(0)),
        result: Ok(ok_ors()),
    };
    let mut cache = PreliminaryCache::new();
    let wp = sample_wp();
    let _ =
        request_preliminary_with_fetchers(&wp, None, &mut b, &mut o, false, &mut cache).unwrap();
    let _ =
        request_preliminary_with_fetchers(&wp, None, &mut b, &mut o, false, &mut cache).unwrap();
    assert_eq!(*b.calls.borrow(), 1);
}

#[test]
fn disclosure_covers_brouter_and_ors() {
    let d = PRELIMINARY_ROUTE_DISCLOSURE.to_ascii_lowercase();
    assert!(d.contains("brouter"));
    assert!(d.contains("openrouteservice") || d.contains("open route"));
    assert!(d.contains("third"));
}

#[test]
fn synthetic_fixture_cannot_back_real_trip_region_list() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/long_trip/ors_us_a_redball_portofino.geojson");
    let text = std::fs::read_to_string(&path).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v.get("navi_fixture").and_then(|x| x.as_str()),
        Some("synthetic")
    );
    let result = std::panic::catch_unwind(|| assert_not_synthetic_for_real_region_list(&path));
    assert!(
        result.is_err(),
        "synthetic must fail real-trip region-list gate"
    );
}

#[test]
#[ignore = "live BRouter US: NAVI_BROUTER_US_PROBE=1 (max 2 attempts/trip, 60s apart)"]
fn live_us_brouter_dry_run() {
    assert_eq!(
        std::env::var("NAVI_BROUTER_US_PROBE").ok().as_deref(),
        Some("1")
    );
    use driver_break_core::long_trip::{
        estimate_trip_disk_bytes, ordered_needed_regions_along_route_filtered,
        parse_brouter_geojson, regions_bbox_adjacent, request_brouter_route, BrouterConfig,
        LONG_TRIP_CORRIDOR_BUFFER_KM,
    };
    use driver_break_core::pack_server::catalog_entries_from_ready_ids;
    use driver_break_core::routing::basemap::region_bbox;
    use serde::Deserialize;
    use std::thread;
    use std::time::{Duration, Instant};

    let us: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/long_trip/us_endpoints.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let start = (
        us["points"]["red_ball_garage"]["lat"].as_f64().unwrap(),
        us["points"]["red_ball_garage"]["lon"].as_f64().unwrap(),
    );
    let dest_a = (
        us["points"]["portofino_hotel"]["lat"].as_f64().unwrap(),
        us["points"]["portofino_hotel"]["lon"].as_f64().unwrap(),
    );
    let dest_b = (
        us["points"]["north_coast_inn"]["lat"].as_f64().unwrap(),
        us["points"]["north_coast_inn"]["lon"].as_f64().unwrap(),
    );
    let cfg = BrouterConfig::default_public();
    let trips = [
        ("us_a_redball_portofino", vec![start, dest_a]),
        ("us_b_redball_crescent", vec![start, dest_b]),
    ];
    let mut last_end = Instant::now() - Duration::from_secs(60);
    for (id, wps) in &trips {
        for attempt in 1..=2u32 {
            let wait = Duration::from_secs(60).saturating_sub(last_end.elapsed());
            if !wait.is_zero() {
                eprintln!("waiting {wait:?} before {id} attempt {attempt}");
                thread::sleep(wait);
            }
            let t0 = Instant::now();
            let result = request_brouter_route(&cfg, wps);
            let wall = t0.elapsed();
            last_end = Instant::now();
            match result {
                Ok(route) => {
                    eprintln!(
                        "LIVE {id} attempt={attempt} OK wall_ms={} dist_m={} pts={} ferry={}",
                        wall.as_millis(),
                        route.distance_m,
                        route.lat_lon.len(),
                        route.ferry_segments
                    );
                    // Re-fetch raw via internal is hard; save parsed summary + re-request body by rebuilding.
                    // Persist by writing a thin recorded wrapper with geometry from a second GET? Policy: no retry loops — we already have Ok.
                    // Serialize a minimal GeoJSON-like record for region-list derivation from lat_lon.
                    let coords: Vec<_> = route
                        .lat_lon
                        .iter()
                        .map(|&(lat, lon)| serde_json::json!([lon, lat]))
                        .collect();
                    let geo = serde_json::json!({
                        "type": "FeatureCollection",
                        "features": [{
                            "type": "Feature",
                            "properties": {
                                "track-length": route.distance_m,
                                "total-time": route.duration_s,
                                "messages": [["Longitude","Latitude","Elevation","Distance","CostPerKm","ElevCost","TurnCost","NodeCost","InitialCost","WayTags","NodeTags","Time","Energy"]],
                                "navi_ferry_segments": route.ferry_segments
                            },
                            "geometry": { "type": "LineString", "coordinates": coords }
                        }]
                    });
                    let wrap = serde_json::json!({
                        "navi_fixture": "recorded",
                        "meta": {
                            "navi_fixture": "recorded",
                            "recorded_utc": chrono::Utc::now().to_rfc3339(),
                            "provider_base_url": "https://brouter.de",
                            "http_method": "GET",
                            "http_status": 200,
                            "wall_time_ms": wall.as_millis() as u64,
                            "trip_id": id,
                            "attempt": attempt,
                            "profile": "car-eco",
                            "note": "geometry reconstructed from successful parse (messages ferry count stored in properties)"
                        },
                        "response_body": geo.to_string()
                    });
                    let out = recorded(&format!("brouter_{id}_car-eco.json"));
                    std::fs::write(&out, serde_json::to_string_pretty(&wrap).unwrap()).unwrap();
                    eprintln!("saved {out:?}");

                    #[derive(Deserialize)]
                    struct Cat {
                        regions: Vec<Reg>,
                    }
                    #[derive(Deserialize)]
                    struct Reg {
                        region_id: String,
                        bytes: Option<u64>,
                    }
                    let cat: Cat = serde_json::from_str(
                        &std::fs::read_to_string(
                            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                .join("tests/fixtures/long_trip/current.json"),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                    let ids: Vec<_> = cat.regions.iter().map(|r| r.region_id.clone()).collect();
                    let sizes: Vec<_> = cat
                        .regions
                        .iter()
                        .map(|r| (r.region_id.clone(), r.bytes.unwrap_or(0)))
                        .collect();
                    let entries = catalog_entries_from_ready_ids(&ids);
                    let installed = ["north-america/us/new-york".to_string()];
                    let needed = ordered_needed_regions_along_route_filtered(
                        &route.lat_lon,
                        &entries,
                        &installed,
                        LONG_TRIP_CORRIDOR_BUFFER_KM,
                        Some("us"),
                    );
                    eprintln!("regions {id} ({}): {needed:?}", needed.len());
                    let mut seen = std::collections::BTreeSet::new();
                    for r in &needed {
                        assert!(seen.insert(r.clone()));
                        assert!(r.starts_with("north-america/us"));
                    }
                    for w in needed.windows(2) {
                        if let (Some(ba), Some(bb)) = (region_bbox(&w[0]), region_bbox(&w[1])) {
                            if !regions_bbox_adjacent(&ba, &bb, 1.75) {
                                eprintln!("NON-ADJACENT {} -> {}", w[0], w[1]);
                            }
                        }
                    }
                    let est = estimate_trip_disk_bytes(&needed, &sizes, 512u64 << 30);
                    eprintln!("storage {id}: {est:?}");
                    break;
                }
                Err(e) => {
                    eprintln!(
                        "LIVE {id} attempt={attempt} ERR wall_ms={} err={e}",
                        wall.as_millis()
                    );
                    let wrap = serde_json::json!({
                        "navi_fixture": "recorded",
                        "meta": {
                            "navi_fixture": "recorded",
                            "recorded_utc": chrono::Utc::now().to_rfc3339(),
                            "http_status": 0,
                            "wall_time_ms": wall.as_millis() as u64,
                            "trip_id": id,
                            "attempt": attempt,
                            "error": e.to_string()
                        },
                        "response_body": e.to_string()
                    });
                    let out = recorded(&format!("brouter_{id}_car-eco_attempt{attempt}.json"));
                    std::fs::write(&out, serde_json::to_string_pretty(&wrap).unwrap()).unwrap();
                    if attempt == 2 {
                        eprintln!("gave up on {id}");
                    }
                }
            }
        }
    }
    let _ = parse_brouter_geojson;
}
