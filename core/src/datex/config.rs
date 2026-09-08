//! DATEX client settings and network-economy defaults.

use std::path::PathBuf;

use crate::config::OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M;

/// Product requirement: DATEX overlay ships **disabled** until the user opts in.
pub const DATEX_PLUGIN_DEFAULT_ENABLED: bool = false;

/// Documented settings default for a typical LAN navi-server (legacy single-host
/// override). Discovery prefers [`crate::pack_server`] LAN → duckdns chain.
pub const DATEX_SETTINGS_DEFAULT_HOST: &str = "192.168.1.195";

/// Default HTTP port for navi-server Apache DocumentRoot.
pub const DATEX_SETTINGS_DEFAULT_PORT: u16 = 80;

/// Situation snapshot path under the DocumentRoot (navi-server contract).
pub const DATEX_SITUATION_PATH: &str = "/datex/GetSituation.xml";

/// Attribution metadata path (fetch before XML; 404 means DATEX off / empty).
pub const DATEX_SOURCE_PATH: &str = "/datex/source.json";

/// Server-side Situation poll cadence (seconds). Client min poll is clamped to
/// this so we never outpace the cache TTL.
pub const DATEX_SERVER_SITUATION_POLL_SECS: u64 = 300;

/// Default: prefer Wi-Fi for DATEX pulls (cellular skipped when set).
pub const DATEX_WIFI_ONLY_DEFAULT: bool = true;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatexConfig {
    /// User enable toggle. Default [`DATEX_PLUGIN_DEFAULT_ENABLED`].
    pub enabled: bool,
    /// When true (default), resolve host via pack_server LAN → duckdns chain.
    /// When false, use [`Self::host`] / [`Self::port`] only (tests / override).
    pub use_discovery_chain: bool,
    /// Single-host override when [`Self::use_discovery_chain`] is false.
    pub host: String,
    /// HTTP port for single-host override (default 80).
    pub port: u16,
    /// Skip network when not on Wi-Fi (Android passes [`Self::on_wifi`]).
    pub wifi_only: bool,
    /// Current transport is Wi-Fi (or Ethernet). Set by the host each refresh.
    pub on_wifi: bool,
    /// Minimum seconds between network polls (clamped ≥ server TTL).
    pub min_poll_interval_secs: u64,
    /// Optional directory for persisted last snapshot + freshness marker.
    pub cache_dir: Option<PathBuf>,
    /// Test / advanced: replace LAN→duckdns bases when [`Self::use_discovery_chain`].
    pub discovery_bases_override: Option<Vec<(crate::pack_server::PackDataSource, String)>>,
    /// Corridor band margin (metres).
    pub corridor_margin_m_bits: u64,
}

impl Default for DatexConfig {
    fn default() -> Self {
        Self {
            enabled: DATEX_PLUGIN_DEFAULT_ENABLED,
            use_discovery_chain: true,
            host: DATEX_SETTINGS_DEFAULT_HOST.to_string(),
            port: DATEX_SETTINGS_DEFAULT_PORT,
            wifi_only: DATEX_WIFI_ONLY_DEFAULT,
            on_wifi: true,
            min_poll_interval_secs: DATEX_SERVER_SITUATION_POLL_SECS,
            cache_dir: None,
            discovery_bases_override: None,
            corridor_margin_m_bits: OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M.to_bits(),
        }
    }
}

impl DatexConfig {
    pub fn corridor_margin_m(&self) -> f64 {
        f64::from_bits(self.corridor_margin_m_bits)
    }

    pub fn with_corridor_margin_m(mut self, margin_m: f64) -> Self {
        self.corridor_margin_m_bits = margin_m.max(1.0).to_bits();
        self
    }

    /// Effective poll floor: never below the server Situation cache TTL.
    pub fn effective_poll_interval_secs(&self) -> u64 {
        self.min_poll_interval_secs
            .max(DATEX_SERVER_SITUATION_POLL_SECS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_defaults_off() {
        const { assert!(!DATEX_PLUGIN_DEFAULT_ENABLED) };
        assert!(!DatexConfig::default().enabled);
        assert!(DatexConfig::default().use_discovery_chain);
        assert!(DatexConfig::default().wifi_only);
    }

    #[test]
    fn poll_interval_clamped_to_server_ttl() {
        let c = DatexConfig {
            min_poll_interval_secs: 60,
            ..Default::default()
        };
        assert_eq!(
            c.effective_poll_interval_secs(),
            DATEX_SERVER_SITUATION_POLL_SECS
        );
        let c = DatexConfig {
            min_poll_interval_secs: 600,
            ..Default::default()
        };
        assert_eq!(c.effective_poll_interval_secs(), 600);
    }
}
