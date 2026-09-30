//! Default HostApi camping caps return honest unknown / empty.

use navi_plugin_host::{
    HostApi, LayerStatus, PoiWrite, Position, TravelModeView,
};

struct EmptyHost;

impl HostApi for EmptyHost {
    fn position(&self) -> Option<Position> {
        None
    }

    fn poi_query(&self, _: f64, _: f64, _: f64) -> Vec<PoiWrite> {
        Vec::new()
    }

    fn poi_write(&mut self, _: PoiWrite) -> Result<(), String> {
        Ok(())
    }

    fn log(&mut self, _: &str) {}
}

#[test]
fn protected_area_and_landcover_default_unknown() {
    let h = EmptyHost;
    let pa = h.protected_area_query(61.0, 10.0);
    assert_eq!(pa.status, LayerStatus::Unknown);
    assert!(pa.areas.is_empty());
    let lc = h.landcover_query(61.0, 10.0);
    assert_eq!(lc.status, LayerStatus::Unknown);
    assert!(lc.class.is_none());
}

#[test]
fn land_tenure_defaults_unknown_manager() {
    let h = EmptyHost;
    let t = h.land_tenure_query(40.0, -110.0);
    assert_eq!(t.manager_type, "unknown");
}

#[test]
fn route_and_destination_default_empty() {
    let h = EmptyHost;
    assert!(h.route_read().waypoints.is_empty());
    assert!(h.route_destination_read().lat.is_none());
}

#[test]
fn vehicle_and_traveller_defaults_are_conservative() {
    let h = EmptyHost;
    let v = h.vehicle_profile_read();
    assert_eq!(v.class, "unknown");
    assert!(!v.is_professional_driver_under_rest_rules);
    assert!(h.traveller_profile_read().residency_country.is_none());
    assert_eq!(h.travel_mode_read(), TravelModeView::Unknown);
}

#[test]
fn admin_region_default_unknown_until_embedder_wires_core() {
    let h = EmptyHost;
    let r = h.admin_region_read(78.22, 15.63);
    assert!(r.country_iso.is_none());
    assert!(r.subdivision_iso.is_none());
}
