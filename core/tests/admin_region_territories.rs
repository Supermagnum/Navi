//! Admin-region territory overrides + Natural Earth fall-through.
//!
//! Override-only coverage lives in `admin_region` unit tests (fast). This file
//! pays the Natural Earth warm cost once to prove Tromsø stays `no`.

use driver_break_core::routing::elevation::warm_country_polys;
use driver_break_core::{admin_region_at, territory_override_at, TerritoryOverride};

#[test]
fn sj_override_beats_ne_and_tromso_stays_no() {
    assert_eq!(territory_override_at(69.6492, 18.9553), None);
    let _ = warm_country_polys();

    assert_eq!(
        territory_override_at(78.2232, 15.6267),
        Some(TerritoryOverride::Confident("sj"))
    );
    assert_eq!(
        admin_region_at(78.2232, 15.6267).country_iso.as_deref(),
        Some("sj"),
        "NE would say no; override must win"
    );

    let tromso = admin_region_at(69.6492, 18.9553);
    assert_eq!(tromso.country_iso.as_deref(), Some("no"));
    // Coastal Tromsø can miss the NE Admin-1 polygon; when it hits, expect Troms.
    if let Some(sub) = tromso.subdivision_iso.as_deref() {
        assert!(
            sub == "no-19" || sub == "no-54" || sub == "no-55",
            "unexpected Tromsø subdivision {sub}"
        );
    }
}
