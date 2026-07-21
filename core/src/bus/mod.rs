//! Inter-thread snapshot and channel types (minimal stubs for this pass).

use std::sync::Arc;

use crate::config::Profile;
use crate::ecu::LiveEnergySnapshot;
use crate::sensors::PositionSample;

/// Immutable world snapshot consumed by routing (T3) and UI (T2).
#[derive(Debug, Clone, Default)]
pub struct WorldSnapshot {
    pub position: PositionSample,
    pub profile: Profile,
    pub live_energy: Option<LiveEnergySnapshot>,
    pub active_route_id: Option<String>,
}

pub type SharedSnapshot = Arc<WorldSnapshot>;
