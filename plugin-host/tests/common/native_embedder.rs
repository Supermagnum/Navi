//! Native camping embedder used by Phase 2 behaviour / real-pack tests.
//! Supplies real SafetyConfig (ConfigStore), OS local clock, in-memory KV,
//! core `admin_region_at`, TravelProfile→mode, and OvernightProximityIndex.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Datelike;
use chrono::Local;
use driver_break_core::admin_region_at;
use driver_break_core::config::{Profile, SafetyConfig};
use driver_break_core::routing::graph::{RouteGraph, RouteOptions, RoutingProfile};
use driver_break_core::routing::indexed::{
    try_load_graph_for_plan_bbox, try_load_poi_barrier_for_plan_bbox,
};
use driver_break_core::routing::safety::OvernightProximityIndex;
use driver_break_core::storage::{ConfigStore, Storage};
use driver_break_core::subdivision_ring_count;
use navi_plugin_host::{
    AdminRegionView, ClockView, HostApi, LayerStatus, PluginKvStatus, PoiWrite, Position,
    RoadTrackJunction, RouteDestinationView, RouteView, SafetyConfigView, TravelModeView,
    TravellerProfileView, VehicleProfileView,
};
use navi_right_to_roam_camping::{
    find_road_track_junctions, CampingHost, LocalDate, TravelMode, CORRIDOR_SEED_RADIUS_M,
};

pub const LILLEHAMMER: (f64, f64) = (61.11515, 10.46628);
pub const SJUSJOEN: (f64, f64) = (61.1475, 10.6980);
/// Swedish inland point (Värmland) — clearly SE for Natural Earth admin.
pub const SWEDEN_INLAND: (f64, f64) = (59.3793, 13.5036); // Karlstad
/// Near Charlottenberg (SE side of the E18 border crossing).
pub const SWEDEN_NEAR_BORDER: (f64, f64) = (59.8840, 12.3040);

pub fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/espa-dombas-e2e")
}

pub fn packs_present(dir: &Path) -> bool {
    dir.join("ostlandet-latest.osm.pbf").is_file()
        && dir.join("ostlandet-latest.navi-poi-barrier.rkyv").is_file()
}

/// Warm hook (baked fylke asset loads on first subdivision query).
pub fn warm_ostlandet_subdivisions(_dir: &Path) -> usize {
    subdivision_ring_count()
}

/// Map core travel [`Profile`] to camping travel mode.
pub fn travel_mode_from_profile(profile: Profile) -> TravelMode {
    match profile {
        Profile::Hiking | Profile::Cycling | Profile::CyclingElectric => TravelMode::NonMotorised,
        Profile::Car
        | Profile::CarElectric
        | Profile::Truck
        | Profile::TruckElectric
        | Profile::MobileHome
        | Profile::Motorcycle
        | Profile::MotorcycleElectric => TravelMode::Motorised,
    }
}

pub fn travel_mode_view_from_profile(profile: Profile) -> TravelModeView {
    match travel_mode_from_profile(profile) {
        TravelMode::NonMotorised => TravelModeView::NonMotorised,
        TravelMode::Motorised => TravelModeView::Motorised,
        TravelMode::Unknown => TravelModeView::Unknown,
    }
}

pub fn local_date_now() -> LocalDate {
    let n = Local::now().date_naive();
    LocalDate {
        year: n.year(),
        month: n.month(),
        day: n.day(),
    }
}

pub struct NativeCampingEmbedder {
    pub safety: SafetyConfig,
    pub clock: Option<LocalDate>,
    pub kv: HashMap<String, String>,
    pub kv_available: bool,
    pub travel_profile: Profile,
    /// User-set; never inferred from weight/size. Default false.
    pub is_professional_driver_under_rest_rules: bool,
    pub buildings: Vec<(f64, f64)>,
    pub glacier_rings: Vec<Vec<[f64; 2]>>,
    pub route_waypoints: Vec<[f64; 2]>,
    pub destination: Option<(f64, f64)>,
    pub protected_ready: bool,
    pub landcover_ready: bool,
}

impl NativeCampingEmbedder {
    /// Copy host backends for a second evaluate pass (same safety/buildings/clock).
    pub fn clone_for_replay(&self) -> Self {
        Self {
            safety: self.safety.clone(),
            clock: self.clock,
            kv: HashMap::new(),
            kv_available: self.kv_available,
            travel_profile: self.travel_profile,
            is_professional_driver_under_rest_rules: self.is_professional_driver_under_rest_rules,
            buildings: self.buildings.clone(),
            glacier_rings: self.glacier_rings.clone(),
            route_waypoints: self.route_waypoints.clone(),
            destination: self.destination,
            protected_ready: self.protected_ready,
            landcover_ready: self.landcover_ready,
        }
    }

    /// Real backends: ConfigStore safety, OS local clock, empty KV store, optional
    /// OvernightProximityIndex from the downloaded poi-barrier pack.
    pub fn with_real_backends(
        storage: &Storage,
        prox: Option<&OvernightProximityIndex>,
        travel_profile: Profile,
    ) -> Self {
        let store = ConfigStore::new(storage);
        let safety = store.load_safety_config().unwrap_or_default();
        Self {
            safety,
            clock: Some(local_date_now()),
            kv: HashMap::new(),
            kv_available: true,
            travel_profile,
            is_professional_driver_under_rest_rules: false,
            buildings: prox.map(|p| p.buildings.clone()).unwrap_or_default(),
            glacier_rings: prox.map(|p| p.glacier_rings.clone()).unwrap_or_default(),
            route_waypoints: Vec::new(),
            destination: None,
            protected_ready: false,
            landcover_ready: false,
        }
    }

    pub fn set_route(&mut self, waypoints: Vec<[f64; 2]>, destination: Option<(f64, f64)>) {
        self.route_waypoints = waypoints;
        self.destination = destination;
    }
}

impl CampingHost for NativeCampingEmbedder {
    fn safety_config(&self) -> Option<navi_right_to_roam_camping::OvernightSafety> {
        Some((&self.safety).into())
    }

    fn clock_local(&self) -> Option<LocalDate> {
        self.clock
    }

    fn plugin_kv_available(&self) -> bool {
        self.kv_available
    }

    fn kv_get(&self, key: &str) -> Option<String> {
        self.kv.get(key).cloned().filter(|s| !s.is_empty())
    }

    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if !self.kv_available {
            return Err("plugin_kv unavailable".into());
        }
        self.kv.insert(key.into(), value.into());
        Ok(())
    }

    fn admin_country_iso(&self, lat: f64, lon: f64) -> Option<String> {
        admin_region_at(lat, lon).country_iso
    }

    fn admin_subdivision_iso(&self, lat: f64, lon: f64) -> Option<String> {
        admin_region_at(lat, lon).subdivision_iso
    }

    fn travel_mode(&self) -> TravelMode {
        travel_mode_from_profile(self.travel_profile)
    }

    fn overnight_buildings(&self) -> &[(f64, f64)] {
        &self.buildings
    }

    fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>] {
        &self.glacier_rings
    }

    fn protected_area_layer_ready(&self) -> bool {
        self.protected_ready
    }

    fn landcover_layer_ready(&self) -> bool {
        self.landcover_ready
    }

    fn vehicle_overnight_profile(&self) -> navi_right_to_roam_camping::VehicleProfile {
        use navi_right_to_roam_camping::{VehicleClass, VehicleProfile};
        let class = match self.travel_profile {
            Profile::Car
            | Profile::CarElectric
            | Profile::Motorcycle
            | Profile::MotorcycleElectric => VehicleClass::Car,
            Profile::MobileHome => VehicleClass::CampervanMotorhome,
            Profile::Truck | Profile::TruckElectric => VehicleClass::Hgv,
            Profile::Hiking | Profile::Cycling | Profile::CyclingElectric => VehicleClass::Unknown,
        };
        VehicleProfile {
            class,
            is_professional_driver_under_rest_rules: self.is_professional_driver_under_rest_rules,
        }
    }

    fn route_destination(&self) -> Option<(f64, f64)> {
        self.destination
    }
}

impl HostApi for NativeCampingEmbedder {
    fn position(&self) -> Option<Position> {
        self.route_waypoints.first().map(|w| Position {
            lat: w[0],
            lon: w[1],
        })
    }

    fn poi_query(&self, _: f64, _: f64, _: f64) -> Vec<PoiWrite> {
        Vec::new()
    }

    fn poi_write(&mut self, _: PoiWrite) -> Result<(), String> {
        Ok(())
    }

    fn log(&mut self, msg: &str) {
        eprintln!("[camping-embedder] {msg}");
    }

    fn route_read(&self) -> RouteView {
        let junctions: Vec<RoadTrackJunction> = Vec::new();
        RouteView {
            waypoints: self.route_waypoints.clone(),
            junctions,
        }
    }

    fn route_destination_read(&self) -> RouteDestinationView {
        match self.destination {
            Some((lat, lon)) => RouteDestinationView {
                lat: Some(lat),
                lon: Some(lon),
            },
            None => RouteDestinationView::default(),
        }
    }

    fn safety_config_read(&self) -> Option<SafetyConfigView> {
        Some(SafetyConfigView {
            min_building_distance_m: self.safety.min_building_distance_m,
            min_glacier_distance_m: Some(self.safety.min_glacier_distance_m),
        })
    }

    fn admin_region_read(&self, lat: f64, lon: f64) -> AdminRegionView {
        let r = admin_region_at(lat, lon);
        AdminRegionView {
            country_iso: r.country_iso,
            subdivision_iso: r.subdivision_iso,
        }
    }

    fn clock_read(&self) -> Option<ClockView> {
        let d = self.clock?;
        Some(ClockView {
            unix_secs: Local::now().timestamp(),
            year: d.year,
            month: d.month,
            day: d.day,
            timezone: "local".into(),
        })
    }

    fn plugin_kv_status(&self) -> PluginKvStatus {
        if self.kv_available {
            PluginKvStatus::Available
        } else {
            PluginKvStatus::Unavailable
        }
    }

    fn plugin_kv_get(&self, key: &str) -> Option<String> {
        if !self.kv_available {
            return None;
        }
        self.kv.get(key).cloned().filter(|s| !s.is_empty())
    }

    fn plugin_kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if !self.kv_available {
            return Err("plugin_kv unavailable".into());
        }
        self.kv.insert(key.into(), value.into());
        Ok(())
    }

    fn travel_mode_read(&self) -> TravelModeView {
        travel_mode_view_from_profile(self.travel_profile)
    }

    fn vehicle_profile_read(&self) -> VehicleProfileView {
        let p = self.vehicle_overnight_profile();
        let class = match p.class {
            navi_right_to_roam_camping::VehicleClass::Car => "car",
            navi_right_to_roam_camping::VehicleClass::CampervanMotorhome => "campervan_motorhome",
            navi_right_to_roam_camping::VehicleClass::Hgv => "hgv",
            navi_right_to_roam_camping::VehicleClass::Unknown => "unknown",
        };
        VehicleProfileView {
            class: class.into(),
            gross_weight_kg: None,
            is_professional_driver_under_rest_rules: p.is_professional_driver_under_rest_rules,
        }
    }

    fn traveller_profile_read(&self) -> TravellerProfileView {
        TravellerProfileView::default()
    }

    fn protected_area_query(&self, _: f64, _: f64) -> navi_plugin_host::ProtectedAreaQueryView {
        navi_plugin_host::ProtectedAreaQueryView {
            status: if self.protected_ready {
                LayerStatus::Ready
            } else {
                LayerStatus::Unknown
            },
            areas: Vec::new(),
        }
    }

    fn landcover_query(&self, _: f64, _: f64) -> navi_plugin_host::LandcoverQueryView {
        navi_plugin_host::LandcoverQueryView {
            status: if self.landcover_ready {
                LayerStatus::Ready
            } else {
                LayerStatus::Unknown
            },
            class: None,
        }
    }
}

/// Load foot graph covering the OD bbox and plan a corridor of lat/lon samples.
pub fn plan_corridor(
    data: &Path,
    start: (f64, f64),
    end: (f64, f64),
) -> Option<(RouteGraph, Vec<[f64; 2]>)> {
    let pbf = data.join("ostlandet-latest.osm.pbf");
    let pad = 0.15;
    let bbox = [
        start.0.min(end.0) - pad,
        start.1.min(end.1) - pad,
        start.0.max(end.0) + pad,
        start.1.max(end.1) + pad,
    ];
    let graph = try_load_graph_for_plan_bbox(data, &pbf, RoutingProfile::Foot, Some(bbox)).ok()?;
    let opts = RouteOptions::default();
    let (s, _) = graph
        .nearest_routable_with_options_max(start.0, start.1, &opts, false, 2_000.0)
        .ok()?;
    let (e, _) = graph
        .nearest_routable_with_options_max(end.0, end.1, &opts, false, 2_000.0)
        .ok()?;
    let st = graph.shortest_path_with_options_stats(s, e, false, &opts);
    let (path, _edges, _cost) = st.path?;
    let mut waypoints = Vec::new();
    for nid in &path {
        if let Some(n) = graph.nodes.get(nid) {
            waypoints.push([n.coord.y, n.coord.x]);
        }
    }
    // Decimate for seed search.
    if waypoints.len() > 80 {
        let step = waypoints.len() / 80;
        waypoints = waypoints.into_iter().step_by(step.max(1)).collect();
    }
    Some((graph, waypoints))
}

pub fn load_proximity(data: &Path, bbox: [f64; 4]) -> Option<OvernightProximityIndex> {
    let pbf = data.join("ostlandet-latest.osm.pbf");
    let (poi, barriers) = try_load_poi_barrier_for_plan_bbox(data, &pbf, Some(bbox)).ok()?;
    let [min_lat, min_lon, max_lat, max_lon] = bbox;
    let buildings: Vec<(f64, f64)> = poi
        .overnight_buildings()
        .iter()
        .copied()
        .filter(|&(lat, lon)| lat >= min_lat && lat <= max_lat && lon >= min_lon && lon <= max_lon)
        .collect();
    let mut prox = OvernightProximityIndex::from_poi_buildings_and_barriers(buildings, &barriers);
    // Glacier rings also clipped roughly by centroid-in-bbox.
    prox.glacier_rings.retain(|ring| {
        ring.iter().any(|p| {
            let (lon, lat) = (p[0], p[1]);
            lat >= min_lat && lat <= max_lat && lon >= min_lon && lon <= max_lon
        })
    });
    Some(prox)
}

/// Build junction list the HostApi `route_read` may expose (runtime from graph).
pub fn junctions_for_route(graph: &RouteGraph, waypoints: &[[f64; 2]]) -> Vec<RoadTrackJunction> {
    find_road_track_junctions(graph, waypoints, CORRIDOR_SEED_RADIUS_M)
        .into_iter()
        .map(|s| RoadTrackJunction {
            lat: s.lat,
            lon: s.lon,
            road_highway: s.road_highway,
            track_continues: s.track_continues_m > 0.0,
        })
        .collect()
}
