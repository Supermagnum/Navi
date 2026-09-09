//! DATEX host-chain + network-economy tests (mock HTTP).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use driver_break_core::datex::{
    refresh_for_route, reset_session_for_tests, with_session, DatexConfig,
    DATEX_SERVER_SITUATION_POLL_SECS, DATEX_WIFI_ONLY_DEFAULT,
};
use driver_break_core::pack_server::PackDataSource;

const MINI_SITUATION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<d2LogicalModel xmlns="http://datex2.eu/schema/3/d2Payload"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  xmlns:sit="http://datex2.eu/schema/3/situation"
  xmlns:com="http://datex2.eu/schema/3/common"
  xmlns:loc="http://datex2.eu/schema/3/locationReferencing"
  modelBaseVersion="3">
  <payloadPublication xsi:type="sit:SituationPublication" lang="no">
    <sit:situation id="mock-1">
      <sit:situationRecord xsi:type="sit:MaintenanceWorks" id="mock-rec-1" version="1">
        <com:situationRecordCreationTime>2026-09-01T00:00:00+02:00</com:situationRecordCreationTime>
        <com:situationRecordVersionTime>2026-09-01T00:00:00+02:00</com:situationRecordVersionTime>
        <sit:validity>
          <com:validityStatus>definedByValidityTimeSpec</com:validityStatus>
          <com:validityTimeSpecification>
            <com:overallStartTime>2026-09-01T00:00:00+02:00</com:overallStartTime>
            <com:overallEndTime>2027-09-01T00:00:00+02:00</com:overallEndTime>
          </com:validityTimeSpecification>
        </sit:validity>
        <sit:groupOfLocations>
          <loc:coordinatesForDisplay>
            <loc:latitude>60.56</loc:latitude>
            <loc:longitude>11.25</loc:longitude>
          </loc:coordinatesForDisplay>
        </sit:groupOfLocations>
      </sit:situationRecord>
    </sit:situation>
  </payloadPublication>
</d2LogicalModel>
"#;

const SOURCE_JSON: &str = r#"{
  "schema": 1,
  "source": "NPRA test",
  "attribution": "NPRA",
  "license": "NLOD 2.0",
  "endpoints": ["GetSituation"]
}"#;

const CURRENT_JSON: &str =
    r#"{"generation":"g1","regions":[{"region_id":"europe/norway/ostlandet"}]}"#;

fn route() -> Vec<(f64, f64)> {
    vec![(60.56, 11.25), (60.57, 11.26)]
}

/// Tiny multi-path mock DocumentRoot. Records request paths for assertions.
fn serve_navi_root(hits: Arc<Mutex<Vec<String>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    thread::spawn(move || {
        for _ in 0..32 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            hits.lock().unwrap().push(path.clone());
            let (status, body, ctype) = if path.starts_with("/current.json") {
                ("HTTP/1.1 200 OK", CURRENT_JSON, "application/json")
            } else if path.starts_with("/datex/source.json") {
                ("HTTP/1.1 200 OK", SOURCE_JSON, "application/json")
            } else if path.starts_with("/datex/GetSituation.xml") {
                ("HTTP/1.1 200 OK", MINI_SITUATION, "application/xml")
            } else {
                ("HTTP/1.1 404 Not Found", "missing", "text/plain")
            };
            let resp = format!(
                "{status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    // Give the accept loop a moment.
    thread::sleep(Duration::from_millis(20));
    format!("http://{addr}")
}

#[test]
fn wifi_only_skips_network() {
    reset_session_for_tests();
    let cfg = DatexConfig {
        enabled: true,
        wifi_only: true,
        on_wifi: false,
        ..Default::default()
    };
    let r = refresh_for_route(&cfg, &route(), Utc::now());
    assert!(!r.overlay_enabled);
    assert_eq!(r.warning.as_deref(), Some("wifi_only"));
    assert_eq!(r.data_source, "none");
    const { assert!(DATEX_WIFI_ONLY_DEFAULT) };
}

#[test]
fn poll_interval_clamped_regression() {
    let cfg = DatexConfig {
        min_poll_interval_secs: 10,
        ..Default::default()
    };
    assert_eq!(
        cfg.effective_poll_interval_secs(),
        DATEX_SERVER_SITUATION_POLL_SECS
    );
}

#[test]
fn both_hosts_unreachable_no_stale_actives() {
    reset_session_for_tests();
    // Seed a stale cache that must NOT become active when hosts are down.
    with_session(|s| {
        s.cached_xml = Some(MINI_SITUATION.to_string());
        s.last_fetch_unix = Some(0);
        s.source_fingerprint = Some("stale".into());
    });
    let cfg = DatexConfig {
        enabled: true,
        use_discovery_chain: false,
        host: "192.0.2.1".into(),
        port: 9,
        wifi_only: false,
        on_wifi: true,
        min_poll_interval_secs: DATEX_SERVER_SITUATION_POLL_SECS,
        ..Default::default()
    };
    // Force outside poll window so we attempt network.
    with_session(|s| s.last_fetch_unix = Some(0));

    let r = refresh_for_route(&cfg, &route(), Utc::now());
    assert!(!r.overlay_enabled);
    assert!(r.active.is_empty());
    assert_eq!(r.data_source, "none");
    assert!(
        r.warning
            .as_deref()
            .is_some_and(|w| w.contains("hosts_unreachable") || w.contains("fetch_failed")),
        "warn={:?}",
        r.warning
    );
    with_session(|s| assert!(s.cached_xml.is_none(), "stale cache must be cleared"));
}

#[test]
fn chain_falls_to_second_host_tagged_duckdns() {
    reset_session_for_tests();
    let hits = Arc::new(Mutex::new(Vec::new()));
    let duck = serve_navi_root(hits.clone());

    let cfg = DatexConfig {
        enabled: true,
        use_discovery_chain: true,
        wifi_only: false,
        on_wifi: true,
        discovery_bases_override: Some(vec![
            (
                PackDataSource::ServerDuckdns,
                "http://192.0.2.1:9".to_string(),
            ),
            (PackDataSource::ServerDuckdns, duck.clone()),
        ]),
        ..Default::default()
    };

    let r = refresh_for_route(&cfg, &route(), Utc::now());
    assert!(r.overlay_enabled, "warn={:?}", r.warning);
    assert_eq!(r.data_source, "server-duckdns");
    assert!(!r.active.is_empty() || !r.situations_on_route.is_empty());

    // Second poll: sticky duckdns — must not re-probe dead LAN first.
    with_session(|s| {
        s.last_fetch_unix = None;
        s.source_fingerprint = None;
        s.chain_probe_count = 0;
        s.sticky_reuse_count = 0;
    });
    hits.lock().unwrap().clear();
    let r2 = refresh_for_route(&cfg, &route(), Utc::now());
    assert!(r2.overlay_enabled, "second warn={:?}", r2.warning);
    assert_eq!(r2.data_source, "server-duckdns");
    let (reuse, chain) = with_session(|s| (s.sticky_reuse_count, s.chain_probe_count));
    assert!(reuse >= 1, "sticky reuse expected");
    assert_eq!(chain, 0, "must not re-run full chain while sticky works");
    let paths = hits.lock().unwrap().clone();
    assert!(
        paths.iter().all(|p| !p.contains("192.0.2.1")),
        "sticky path must not contact dead LAN; paths={paths:?}"
    );
}

#[test]
fn sticky_skips_lan_reprobe_on_second_cycle() {
    reset_session_for_tests();
    let hits = Arc::new(Mutex::new(Vec::new()));
    let base = serve_navi_root(hits.clone());

    let cfg = DatexConfig {
        enabled: true,
        use_discovery_chain: true,
        wifi_only: false,
        on_wifi: true,
        discovery_bases_override: Some(vec![(PackDataSource::ServerDuckdns, base)]),
        ..Default::default()
    };

    let r1 = refresh_for_route(&cfg, &route(), Utc::now());
    assert!(r1.overlay_enabled, "first: {:?}", r1.warning);
    let chain1 = with_session(|s| s.chain_probe_count);
    assert!(chain1 >= 1);

    with_session(|s| {
        s.last_fetch_unix = None;
        s.source_fingerprint = None;
        s.sticky_reuse_count = 0;
    });
    let r2 = refresh_for_route(&cfg, &route(), Utc::now());
    assert!(r2.overlay_enabled, "second: {:?}", r2.warning);
    let (reuse, chain2) = with_session(|s| (s.sticky_reuse_count, s.chain_probe_count));
    assert!(reuse >= 1, "sticky reuse expected");
    assert_eq!(
        chain2, chain1,
        "chain_probe_count must not increase on sticky reuse"
    );
}
