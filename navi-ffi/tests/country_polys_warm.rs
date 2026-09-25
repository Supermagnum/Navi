//! Country polygon index must be warmable off the UI thread and pollable.
use navi::{country_polys_ready, warm_country_polys};

#[test]
fn warm_sets_country_polys_ready() {
    let bytes = warm_country_polys();
    assert!(
        bytes > 100_000,
        "decoded geometry unexpectedly tiny: {bytes}"
    );
    assert!(country_polys_ready());
}
