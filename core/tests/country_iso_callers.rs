//! Task C: compare HOS / speed-camera / road-sign jurisdiction at evidence points
//! under the pre-polygon coarse rings vs Natural Earth `country_iso_at`.
//!
//! Does not modify those modules — report only.

use driver_break_core::config::JurisdictionDrivingHoursPack;
use driver_break_core::routing::elevation::country_iso_at;
use driver_break_core::routing::resolve_driving_hours_pack_at;
use driver_break_core::routing::road_sign::{
    resolve_road_sign_jurisdiction_at, RoadSignJurisdiction,
};
use driver_break_core::routing::speed_camera::{
    resolve_speed_camera_jurisdiction_at, SpeedCameraJurisdiction,
};
use geo::{point, Contains, Coord, LineString, Polygon};
use serde::Deserialize;
use std::path::PathBuf;

type LonLat = (f64, f64);

fn poly(ring: &[LonLat]) -> Polygon {
    let mut coords: Vec<Coord> = ring
        .iter()
        .map(|(lon, lat)| Coord { x: *lon, y: *lat })
        .collect();
    if let (Some(first), Some(last)) = (coords.first().copied(), coords.last().copied()) {
        if first != last {
            coords.push(first);
        }
    }
    Polygon::new(LineString::new(coords), vec![])
}

/// Coarse rings from `country_polys.rs` before Natural Earth (parent of 69135575).
fn old_box_iso_at(lat: f64, lon: f64) -> Option<&'static str> {
    const RINGS: &[(&str, &[LonLat])] = &[
        (
            "li",
            &[(9.47, 47.05), (9.63, 47.05), (9.63, 47.27), (9.47, 47.27)],
        ),
        (
            "lu",
            &[(5.73, 49.44), (6.53, 49.44), (6.53, 50.19), (5.73, 50.19)],
        ),
        (
            "dk",
            &[(8.05, 54.55), (12.70, 54.55), (12.70, 57.80), (8.05, 57.80)],
        ),
        (
            "se",
            &[
                (11.00, 55.20),
                (24.20, 55.20),
                (24.20, 69.10),
                (11.00, 69.10),
            ],
        ),
        (
            "fi",
            &[
                (20.50, 59.70),
                (31.60, 59.70),
                (31.60, 70.10),
                (20.50, 70.10),
            ],
        ),
        (
            "no",
            &[(4.30, 57.80), (31.20, 57.80), (31.20, 71.40), (4.30, 71.40)],
        ),
        (
            "de",
            &[(5.80, 47.20), (15.10, 47.20), (15.10, 55.20), (5.80, 55.20)],
        ),
        (
            "gb",
            &[(-8.20, 49.80), (2.10, 49.80), (2.10, 59.00), (-8.20, 59.00)],
        ),
        (
            "ie",
            &[
                (-10.50, 51.40),
                (-5.90, 51.40),
                (-5.90, 55.50),
                (-10.50, 55.50),
            ],
        ),
        (
            "us",
            &[(-125.0, 24.0), (-66.0, 24.0), (-66.0, 49.5), (-125.0, 49.5)],
        ),
        (
            "ca",
            &[(-141.0, 41.5), (-52.0, 41.5), (-52.0, 70.0), (-141.0, 70.0)],
        ),
        (
            "mx",
            &[(-118.5, 14.5), (-86.5, 14.5), (-86.5, 32.8), (-118.5, 32.8)],
        ),
        (
            "ch",
            &[(5.95, 45.82), (10.50, 45.82), (10.50, 47.81), (5.95, 47.81)],
        ),
        (
            "fr",
            &[(-5.20, 42.20), (8.30, 42.20), (8.30, 51.20), (-5.20, 51.20)],
        ),
    ];
    let p = point!(x: lon, y: lat);
    for (code, ring) in RINGS {
        if poly(ring).contains(&p) {
            return Some(*code);
        }
    }
    None
}

const EC561_FAMILY: &[&str] = &[
    "at", "be", "bg", "cy", "cz", "de", "dk", "ee", "es", "fi", "fr", "gr", "hr", "hu", "ie", "it",
    "lt", "lu", "lv", "mt", "nl", "pl", "pt", "ro", "se", "si", "sk", "no", "is", "li", "ch",
];

fn old_hos(lat: f64, lon: f64) -> JurisdictionDrivingHoursPack {
    match old_box_iso_at(lat, lon) {
        Some("us") => JurisdictionDrivingHoursPack::Fmcsa,
        Some(code) if EC561_FAMILY.contains(&code) => JurisdictionDrivingHoursPack::Ec561,
        Some(_) | None => JurisdictionDrivingHoursPack::Unknown,
    }
}

fn old_speed(lat: f64, lon: f64) -> SpeedCameraJurisdiction {
    match old_box_iso_at(lat, lon) {
        Some("no") | Some("gb") => SpeedCameraJurisdiction::AllowedOptIn,
        _ => SpeedCameraJurisdiction::Declined,
    }
}

fn old_road_sign(lat: f64, lon: f64) -> RoadSignJurisdiction {
    match old_box_iso_at(lat, lon) {
        Some("no") => RoadSignJurisdiction::Norway,
        // Same SE Innlandet workaround as production `road_sign.rs`.
        Some("se") if (59.3..=63.5).contains(&lat) && lon < 12.15 => RoadSignJurisdiction::Norway,
        _ => RoadSignJurisdiction::Other,
    }
}

#[derive(Deserialize)]
struct File {
    points: Vec<Point>,
}

#[derive(Deserialize)]
struct Point {
    id: String,
    lat: f64,
    lon: f64,
    expected_iso: String,
}

#[test]
fn report_hos_speed_road_sign_changes_vs_old_boxes() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/long_trip/country_iso_expected.json");
    let file: File = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

    let mut hos_changes = Vec::new();
    let mut speed_changes = Vec::new();
    let mut sign_changes = Vec::new();
    let mut se_workaround_unnecessary = 0usize;
    let mut se_workaround_still_needed = 0usize;

    for p in &file.points {
        let new_iso = country_iso_at(p.lat, p.lon);
        let old_iso = old_box_iso_at(p.lat, p.lon);

        let oh = old_hos(p.lat, p.lon);
        let nh = resolve_driving_hours_pack_at(p.lat, p.lon);
        if oh != nh {
            let correct = match p.expected_iso.as_str() {
                "us" => nh == JurisdictionDrivingHoursPack::Fmcsa,
                code if EC561_FAMILY.contains(&code) => nh == JurisdictionDrivingHoursPack::Ec561,
                _ => nh == JurisdictionDrivingHoursPack::Unknown,
            };
            hos_changes.push(format!(
                "{} expected_iso={} old_iso={:?} new_iso={:?} old={:?} new={:?} new_correct={}",
                p.id, p.expected_iso, old_iso, new_iso, oh, nh, correct
            ));
        }

        let os = old_speed(p.lat, p.lon);
        let ns = resolve_speed_camera_jurisdiction_at(p.lat, p.lon);
        if os != ns {
            let expect_allowed = matches!(p.expected_iso.as_str(), "no" | "gb");
            let correct = expect_allowed == (ns == SpeedCameraJurisdiction::AllowedOptIn);
            speed_changes.push(format!(
                "{} expected_iso={} old_iso={:?} new_iso={:?} old={:?} new={:?} new_correct={}",
                p.id, p.expected_iso, old_iso, new_iso, os, ns, correct
            ));
        }

        let or = old_road_sign(p.lat, p.lon);
        let nr = resolve_road_sign_jurisdiction_at(p.lat, p.lon);
        if or != nr {
            let correct = (p.expected_iso == "no") == (nr == RoadSignJurisdiction::Norway);
            sign_changes.push(format!(
                "{} expected_iso={} old_iso={:?} new_iso={:?} old={:?} new={:?} new_correct={}",
                p.id, p.expected_iso, old_iso, new_iso, or, nr, correct
            ));
        }

        if old_iso == Some("se") && (59.3..=63.5).contains(&p.lat) && p.lon < 12.15 {
            if new_iso == Some("no") && p.expected_iso == "no" {
                se_workaround_unnecessary += 1;
            } else if new_iso == Some("se") && p.expected_iso == "no" {
                se_workaround_still_needed += 1;
            }
        }
    }

    eprintln!("HOS_CHANGES={}", hos_changes.len());
    for l in &hos_changes {
        eprintln!("HOS {l}");
    }
    eprintln!("SPEED_CHANGES={}", speed_changes.len());
    for l in &speed_changes {
        eprintln!("SPEED {l}");
    }
    eprintln!("ROAD_SIGN_CHANGES={}", sign_changes.len());
    for l in &sign_changes {
        eprintln!("SIGN {l}");
    }
    eprintln!(
        "SE_WORKAROUND unnecessary_on_fixture={se_workaround_unnecessary} \
         still_needed_on_fixture={se_workaround_still_needed}"
    );
    eprintln!(
        "NOTE: road_sign.rs still contains the SE Innlandet lon<12.15 workaround; \
         modules were not modified (Task C)."
    );
}
