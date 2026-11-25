//! Navi WASM plugin host.
//!
//! Loads capability-gated `.wasm` modules, wires a narrow HostApi, and enforces
//! per-call fuel, wall-clock epoch interruption, and a linear-memory ceiling so
//! a misbehaving plugin cannot starve routing/sensor/UI threads.

mod abi;
pub mod cluster_guidance;
pub mod cluster_sink;
mod file_kv;
mod host;
mod manifest;
mod plugin_enable;
mod sensor_hub;
pub mod smoke;

pub use abi::{
    AdminRegionView, Capability, ClockView, HostApi, LandTenureView, LandcoverQueryView,
    LayerStatus, MemoryPluginKv, PluginKvStatus, PoiWrite, Position, ProtectedAreaHit,
    ProtectedAreaQueryView, RoadTrackJunction, RouteDestinationView, RouteView, SafetyConfigView,
    SensorAvailabilityView, SensorKind, SensorReadView, SensorSample, SensorStatus, TravelModeView,
    TravellerProfileView, VehicleProfileView, WeatherSampleView,
};
pub use cluster_guidance::{
    assemble_nav_guidance_v1, idle_nav_guidance_v1, merge_primary_warning, GuidanceAssemblyInput,
};
pub use cluster_sink::{
    cluster_json_to_vss_updates, default_json_udp_endpoint, default_kuksa_endpoint,
    ClusterEndpoint, ClusterSink, ClusterSinkEndpoints, ClusterSinkKind, JsonUdpLiveSink,
    KuksaLiveSink, LogSink, NdjsonBufferSink,
};
pub use file_kv::{FileKvHostApi, FilePluginKv, EPHEMERAL_KEY_PREFIX};
pub use host::{
    cranelift_abi_supported, CallOutcome, GuestCallStats, PluginError, PluginHost, PluginLimits,
    DEFAULT_MEMORY_BYTES,
};
pub use manifest::PluginManifest;
pub use plugin_enable::{plugin_list, plugin_set_enabled, PluginEnableStore, PluginListEntry};
pub use sensor_hub::{SensorBackend, SensorHub, SensorHubConfig};
