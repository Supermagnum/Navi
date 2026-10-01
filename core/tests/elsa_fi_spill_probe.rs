//! Finland catalog AABB spills over eastern Finnmark; same-leaf densify hops
//! must not treat that spill as an endpoint-covering extra stem.

use driver_break_core::long_trip::region_containing;
use driver_break_core::routing::basemap::{bbox_covers_point, region_bbox};

#[test]
fn finland_aabb_covers_bugoynes_hop_but_pip_is_nord_norge() {
    let fi = region_bbox("europe/finland").expect("fi");
    let nn = region_bbox("europe/norway/nord-norge").expect("nn");
    let start = (69.9741435, 29.6337571);
    let end = (69.79367, 29.35743);
    assert!(
        bbox_covers_point(fi, start.0, start.1) && bbox_covers_point(fi, end.0, end.1),
        "precondition: Finland AABB must spill over this Nord-Norge hop"
    );
    let fi_area = (fi[2] - fi[0]) * (fi[3] - fi[1]);
    let nn_area = (nn[2] - nn[0]) * (nn[3] - nn[1]);
    assert!(
        fi_area < nn_area,
        "precondition: Finland smaller AABB than Nord-Norge (fi={fi_area} nn={nn_area})"
    );
    assert_eq!(
        region_containing(start.0, start.1, None),
        Some("europe/norway/nord-norge")
    );
    assert_eq!(
        region_containing(end.0, end.1, None),
        Some("europe/norway/nord-norge")
    );
}

#[test]
fn se_spine_second_joint_is_finland_pip_hole() {
    // Fair-path densify joint after Bugøynes exit sits in the Lapland PIP hole;
    // planning must force-retain the Finland country pack (see load.rs).
    assert!(region_containing(69.58521, 28.74130, None).is_none());
    let fi = region_bbox("europe/finland").expect("fi");
    assert!(bbox_covers_point(fi, 69.58521, 28.74130));
}
