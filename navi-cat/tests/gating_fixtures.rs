//! Gating parser tests against saved `\dump_caps` fixtures.

use navi_cat::gating::gate_from_dump_caps;
use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/dump_caps");
    p.push(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn stable_fixture_allowed() {
    let d = gate_from_dump_caps(&fixture("stable.txt"), false, false);
    assert!(d.allowed, "{}", d.reason);
}

#[test]
fn beta_fixture_refused_without_override() {
    let d = gate_from_dump_caps(&fixture("beta.txt"), false, false);
    assert!(!d.allowed);
}

#[test]
fn beta_fixture_override() {
    let d = gate_from_dump_caps(&fixture("beta.txt"), true, false);
    assert!(d.allowed);
    assert!(d.beta_override_warning.is_some());
}

#[test]
fn alpha_fixture_refused() {
    let d = gate_from_dump_caps(&fixture("alpha.txt"), true, false);
    assert!(!d.allowed);
}

#[test]
fn untested_fixture_refused() {
    let d = gate_from_dump_caps(&fixture("untested.txt"), true, false);
    assert!(!d.allowed);
}

#[test]
fn missing_lines_fail_closed() {
    let d = gate_from_dump_caps(&fixture("missing_lines.txt"), false, false);
    assert!(!d.allowed);
}

#[test]
fn missing_ctcss_fail_closed() {
    let d = gate_from_dump_caps(&fixture("missing_ctcss.txt"), false, false);
    assert!(!d.allowed);
}
