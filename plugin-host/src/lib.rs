//! Navi WASM plugin host.
//!
//! Loads capability-gated `.wasm` modules, wires a narrow HostApi, and enforces
//! per-call fuel plus wall-clock epoch interruption so a misbehaving plugin
//! cannot starve routing/sensor/UI threads.

mod abi;
mod host;
mod manifest;
pub mod smoke;

pub use abi::{Capability, HostApi, PoiWrite, Position};
pub use host::{CallOutcome, PluginError, PluginHost, PluginLimits};
pub use manifest::PluginManifest;
