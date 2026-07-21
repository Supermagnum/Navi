//! Capability surface exposed to sandboxed plugins.

use serde::{Deserialize, Serialize};

/// Declared plugin capabilities (must match the manifest before load).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Read the host's last known position.
    PositionRead,
    /// Query host POI store near a point.
    PoiQuery,
    /// Write / upsert a POI into the host store.
    PoiWrite,
    /// Append a log line visible to the host.
    Log,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PositionRead => "position_read",
            Self::PoiQuery => "poi_query",
            Self::PoiWrite => "poi_write",
            Self::Log => "log",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "position_read" | "position" => Some(Self::PositionRead),
            "poi_query" => Some(Self::PoiQuery),
            "poi_write" => Some(Self::PoiWrite),
            "log" => Some(Self::Log),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PoiWrite {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub kind: String,
}

/// Host-side callbacks the engine may invoke on behalf of a plugin.
pub trait HostApi: Send + Sync {
    fn position(&self) -> Option<Position>;
    fn poi_query(&self, lat: f64, lon: f64, radius_m: f64) -> Vec<PoiWrite>;
    fn poi_write(&mut self, poi: PoiWrite) -> Result<(), String>;
    fn log(&mut self, message: &str);
}
