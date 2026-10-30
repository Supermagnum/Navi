//! Navi CAT host service: radio control via `RigBackend` + onboard repeater DB.
//!
//! Safety invariants enforced here (not in WASM):
//! - Never send `T` / `rig_set_ptt`
//! - Refuse program while PTT on or DCD active (follow)
//! - Fail-closed backend gating
//! - Full read-back verification with one retry

pub mod benshi;
pub mod consensus;
pub mod coverage;
pub mod ffi;
pub mod gating;
pub mod importers;
pub mod mirror;
pub mod program;
pub mod repeater;
pub mod repeaterbook;
pub mod resolve;
pub mod tcp;
pub mod types;

pub use benshi::{BenshiBackend, FakeBenshiPeer, TuningMethod};
pub use ffi::FfiRigBackend;
pub use gating::{gate_from_dump_caps, GateDecision, GateStatus};
pub use program::{program_vfo1_verified, ProgramError, ProgramRequest, ReportedVfo};
pub use repeater::{AccessTone, RepeaterDb, RepeaterSite, RepeaterSource};
pub use tcp::TcpRigBackend;
pub use types::{RigBackend, RigError, ShiftDir, VfoState};

/// Shared entry used by HostApi embedders.
pub struct CatService<B: RigBackend> {
    pub backend: B,
    pub db: RepeaterDb,
    pub follow_network_id: Option<String>,
    pub follow_pinned: Option<String>,
    pub follow_stopped_reason: Option<String>,
}

impl<B: RigBackend> CatService<B> {
    pub fn new(backend: B, db: RepeaterDb) -> Self {
        Self {
            backend,
            db,
            follow_network_id: None,
            follow_pinned: None,
            follow_stopped_reason: None,
        }
    }

    pub fn status_json(&mut self) -> String {
        let gate = self.backend.gate();
        let ptt = self.backend.get_ptt().unwrap_or(false);
        serde_json::json!({
            "connected": self.backend.is_connected(),
            "model": self.backend.model_name(),
            "gating": gate,
            "ptt": ptt,
            "follow_network_id": self.follow_network_id,
            "follow_pinned": self.follow_pinned,
            "follow_stopped_reason": self.follow_stopped_reason,
            "now_unix_s": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        })
        .to_string()
    }

    pub fn repeater_query_json(
        &self,
        lat: f64,
        lon: f64,
        radius_km: f64,
        network_id: Option<&str>,
    ) -> String {
        if radius_km > 150.0 {
            return serde_json::json!({
                "ok": false,
                "error": format!("radius_km {radius_km} exceeds maximum 150")
            })
            .to_string();
        }
        let radius = radius_km.max(0.0);
        let filter = crate::repeater::follow_query_filter(network_id);
        let network_id = match &filter {
            crate::repeater::FollowQuery::None => return "[]".into(),
            crate::repeater::FollowQuery::All => None,
            crate::repeater::FollowQuery::Named(name) => Some(name.as_str()),
        };
        let sites = self.db.query_near(lat, lon, radius, network_id);
        // Compact follow payload: guest only needs id/lat/lon/network/freqs.
        // Full RepeaterSite JSON for Ostlandet near-set is ~70–80 KiB and used to
        // truncate the WASM query buffer (live=0 → no VFO1 programs).
        follow_sites_compact_json(&sites)
    }

    /// Nearby list for CAT UI: includes excluded / position_null / DMR (info-only).
    pub fn repeater_list_ui_json(&self, lat: f64, lon: f64, radius_km: f64) -> String {
        if radius_km > 150.0 {
            return serde_json::json!({
                "ok": false,
                "error": format!("radius_km {radius_km} exceeds maximum 150")
            })
            .to_string();
        }
        let sites = self.db.list_near_ui(lat, lon, radius_km.max(0.0));
        serde_json::to_string(&sites).unwrap_or_else(|_| "[]".into())
    }

    pub fn vfo_set_json(&mut self, request_json: &str) -> String {
        let req: ProgramRequest = match serde_json::from_str(request_json) {
            Ok(r) => r,
            Err(e) => {
                return serde_json::json!({"ok":false,"error":format!("bad request: {e}")})
                    .to_string();
            }
        };
        match program_vfo1_verified(&mut self.backend, &req) {
            Ok(reported) => {
                // Selecting a non-networked site leaves follow via normal auto-tune.
                if req.leaves_follow {
                    self.follow_network_id = None;
                    self.follow_pinned = None;
                }
                serde_json::json!({"ok":true,"reported":reported}).to_string()
            }
            Err(e) => {
                if matches!(e, ProgramError::ReadbackMismatch { .. }) {
                    self.follow_stopped_reason = Some(e.to_string());
                    self.follow_network_id = None;
                }
                serde_json::json!({"ok":false,"error":e.to_string(),"field":e.field()}).to_string()
            }
        }
    }

    pub fn network_follow_json(&mut self, request_json: &str) -> String {
        #[derive(serde::Deserialize)]
        struct FollowReq {
            network_id: Option<String>,
            enabled: bool,
            pinned_site: Option<String>,
        }
        let req: FollowReq = match serde_json::from_str(request_json) {
            Ok(r) => r,
            Err(e) => {
                return serde_json::json!({"ok":false,"error":format!("bad request: {e}")})
                    .to_string();
            }
        };
        let raw = req.network_id.unwrap_or_default();
        let trimmed = raw.trim();
        if !req.enabled || trimmed.eq_ignore_ascii_case("off") {
            self.follow_network_id = None;
            self.follow_pinned = None;
            self.follow_stopped_reason = None;
            return serde_json::json!({"ok":true,"enabled":false}).to_string();
        }
        // "auto" is the nearest-network choice. It is not a network id to query.
        let stored = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
            "auto".to_string()
        } else {
            trimmed.to_string()
        };
        self.follow_network_id = Some(stored);
        self.follow_pinned = req.pinned_site;
        self.follow_stopped_reason = None;
        serde_json::json!({
            "ok": true,
            "enabled": true,
            "network_id": self.follow_network_id,
            "pinned": self.follow_pinned,
        })
        .to_string()
    }
}

/// Guest follow tick fields only (keeps Ostlandet near-set well under the WASM buffer).
fn follow_sites_compact_json(sites: &[RepeaterSite]) -> String {
    let rows: Vec<serde_json::Value> = sites
        .iter()
        .filter_map(|s| {
            let lat = s.lat?;
            let lon = s.lon?;
            Some(serde_json::json!({
                "id": s.id,
                "callsign": s.callsign,
                "lat": lat,
                "lon": lon,
                "freq_out_mhz": s.freq_out_mhz,
                "shift_mhz": s.shift_mhz,
                "ctcss_hz": s.ctcss_hz,
                "network_id": s.network_id,
                "distance_km": s.distance_km,
            }))
        })
        .collect();
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
}
