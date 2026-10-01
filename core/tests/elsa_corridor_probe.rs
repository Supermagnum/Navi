//! Bugøynes→Sjuvasslia corridor must pull northern Sweden + Finland (fair
//! ~1944 km path), not stay on Norway landsdel-only adjacency.

use driver_break_core::long_trip::{ordered_needed_regions_for_trip, region_containing};

#[test]
fn trip_bugoynes_sjuvasslia_includes_northern_sweden() {
    // Elsa's caravan & galleri → Sjuvasslia Camping.
    let start = (69.9741435, 29.6337571);
    let end = (59.803175, 9.397871);
    let needed = ordered_needed_regions_for_trip(&[start, end], &[], None).expect("corridor");
    eprintln!("Bugøynes→Sjuvasslia adjacency: {needed:?}");
    assert!(
        needed.iter().any(|r| r.contains("nord-norge")),
        "expected Nord-Norge start: {needed:?}"
    );
    assert!(
        needed.iter().any(|r| r.contains("ostlandet")),
        "expected Østlandet end: {needed:?}"
    );
    assert!(
        needed
            .iter()
            .any(|r| r.contains("norrbotten") || r.contains("vasterbotten")),
        "fair path must request northern SE transit, not Norway-only: {needed:?}"
    );
    assert!(
        needed.iter().any(|r| r.contains("finland")),
        "fair path crosses Finnish Lapland; Finland pack required: {needed:?}"
    );
    assert!(
        !needed.iter().any(|r| r.contains("trondelag")),
        "Norway-coastal Trøndelag should not be on the SE transit corridor: {needed:?}"
    );
    // Finland is inserted immediately after Nord-Norge.
    let nn = needed.iter().position(|r| r.contains("nord-norge"));
    let fi = needed.iter().position(|r| r.contains("finland"));
    assert!(
        matches!((nn, fi), (Some(a), Some(b)) if b == a + 1),
        "Finland should follow Nord-Norge in download order: {needed:?}"
    );
}

#[test]
fn trip_bugoynes_sjuvasslia_norway_only_filter_stays_no() {
    let start = (69.9741435, 29.6337571);
    let end = (59.803175, 9.397871);
    let needed =
        ordered_needed_regions_for_trip(&[start, end], &[], Some("no")).expect("NO corridor");
    eprintln!("Bugøynes→Sjuvasslia NO-only: {needed:?}");
    for r in &needed {
        assert!(
            r.starts_with("europe/norway"),
            "Norway-only leaked {r} in {needed:?}"
        );
    }
}

#[test]
fn fair_path_exit_stays_in_nord_norge_then_holes_fi() {
    // First densify joint must remain PIP-coverable by Nord-Norge; the next
    // fair-path samples sit in the Lapland hole until Finland is downloaded.
    assert_eq!(
        region_containing(69.79367, 29.35743, None),
        Some("europe/norway/nord-norge")
    );
    assert!(
        region_containing(69.58521, 28.74130, None).is_none(),
        "Lapland hole must stay None until adjacency rings include Finland"
    );
}
