//! Navi WASM plugin host.
//!
//! Loads capability-gated `.wasm` modules, wires a narrow HostApi, and enforces
//! per-call fuel plus wall-clock epoch interruption so a misbehaving plugin
//! cannot starve routing/sensor/UI threads.

mod abi;
mod host;
mod manifest;
mod plugin_enable;
pub mod smoke;

pub use abi::{
    AdminRegionView, Capability, ClockView, HostApi, LandTenureView, LandcoverQueryView,
    LayerStatus, MemoryPluginKv, PluginKvStatus, PoiWrite, Position, ProtectedAreaHit,
    ProtectedAreaQueryView, RoadTrackJunction, RouteDestinationView, RouteView, SafetyConfigView,
    TravelModeView, TravellerProfileView, VehicleProfileView, WeatherSampleView,
};
pub use host::{CallOutcome, PluginError, PluginHost, PluginLimits};
pub use manifest::PluginManifest;
pub use plugin_enable::{
    plugin_list, plugin_set_enabled, PluginEnableStore, PluginListEntry,
};
