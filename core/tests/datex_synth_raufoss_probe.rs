use chrono::{TimeZone, Utc};
use driver_break_core::datex::{
    corridor_view, load_plan_datex_situations, parse_situation_publication,
    planner_impacts_from_data_dir,
};
use std::path::PathBuf;
#[test]
fn synth_raufoss_impacts() {
    let dir = PathBuf::from("/tmp/datexprobe_bzaq2dm0");
    let xml = std::fs::read_to_string(dir.join("datex_cache/datex-GetSituation.xml")).unwrap();
    let all = parse_situation_publication(&xml).expect("parse");
    eprintln!("parsed={}", all.len());
    for s in &all {
        eprintln!(
            "id={} impact={:?} geom={:?} valid={:?}..{:?}",
            s.id,
            s.impact,
            s.geometry.first(),
            s.valid_from,
            s.valid_to
        );
    }
    let now = Utc.with_ymd_and_hms(2026, 11, 19, 9, 0, 0).unwrap();
    let route: Vec<(f64, f64)> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("route.json")).unwrap()).unwrap();
    let view = corridor_view(&all, &route, 1500.0, now);
    eprintln!(
        "active={} inactive={}",
        view.active.len(),
        view.inactive.len()
    );
    let loaded = load_plan_datex_situations(&dir, now);
    eprintln!("loaded={:?}", loaded.as_ref().map(|v| v.len()));
    let impacts = planner_impacts_from_data_dir(&dir, &route, now);
    eprintln!("impacts={}", impacts.len());
    for c in &impacts {
        eprintln!("  {} {:?}", c.situation_id, c.impact);
    }
    assert!(!all.is_empty());
}
