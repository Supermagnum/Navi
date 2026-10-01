//! Navi WASM plugin host.
//!
//! Loads capability-gated `.wasm` modules, wires a narrow HostApi, and enforces
//! per-call fuel, wall-clock epoch interruption, and a linear-memory ceiling so
//! a misbehaving plugin cannot starve routing/sensor/UI threads.

mod abi;
mod file_kv;
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
pub use file_kv::{FileKvHostApi, FilePluginKv};
pub use host::{
    cranelift_abi_supported, CallOutcome, GuestCallStats, PluginError, PluginHost, PluginLimits,
    DEFAULT_MEMORY_BYTES,
};
pub use manifest::PluginManifest;
pub use plugin_enable::{plugin_list, plugin_set_enabled, PluginEnableStore, PluginListEntry};
