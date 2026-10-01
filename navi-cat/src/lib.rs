//! Navi CAT host service: radio control via `RigBackend` + onboard repeater DB.
//!
//! Safety invariants enforced here (not in WASM):
//! - Never send `T` / `rig_set_ptt`
//! - Refuse program while PTT on or DCD active (follow)
//! - Fail-closed backend gating
//! - Full read-back verification with one retry

pub mod ffi;
pub mod gating;
pub mod importers;
pub mod program;
pub mod repeater;
pub mod tcp;
pub mod types;

pub use ffi::FfiRigBackend;
pub use gating::{gate_from_dump_caps, GateDecision, GateStatus};
pub use program::{program_vfo1_verified, ProgramError, ProgramRequest, ReportedVfo};
pub use repeater::{RepeaterDb, RepeaterSite, RepeaterSource};
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
        let sites = self.db.query_near(lat, lon, radius, network_id);
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
                serde_json::json!({"ok":false,"error":e.to_string(),"field":e.field()})
                    .to_string()
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
        if !req.enabled {
            self.follow_network_id = None;
            self.follow_pinned = None;
            self.follow_stopped_reason = None;
            return serde_json::json!({"ok":true,"enabled":false}).to_string();
        }
        let Some(nid) = req.network_id else {
            return serde_json::json!({"ok":false,"error":"network_id required"}).to_string();
        };
        self.follow_network_id = Some(nid);
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
