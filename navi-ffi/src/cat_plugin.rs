//! CAT / CATS: UniFFI session over `navi-cat` + optional wasm guest install.
//!
//! Radio safety (PTT/DCD, gating, read-back, never TX) stays in `navi-cat`.
//! Product UI talks UniFFI; the WASM guest may only decide *what* to program
//! via HostApi when ticked.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use navi_cat::gating::{GateDecision, GateStatus};
use navi_cat::types::{RigBackend, RigError, VfoState};
use navi_cat::{CatService, RepeaterDb, TcpRigBackend};
use navi_plugin_host::{
    cranelift_abi_supported, plugin_set_enabled, Capability, FilePluginKv, HostApi, PluginHost,
    PluginKvStatus, PluginLimits, PoiWrite, Position, DEFAULT_MEMORY_BYTES,
};

const CAT_NAME: &str = "cat";
const ENABLE_FILE: &str = "plugin_enable_cat.json";
const KV_REL: &str = "plugin_kv/cat.json";
const PLUGINS_REL: &str = "plugins";

#[derive(Debug, Default)]
struct OfflineRig;

impl RigBackend for OfflineRig {
    fn is_connected(&self) -> bool {
        false
    }
    fn model_name(&self) -> String {
        "offline".into()
    }
    fn model_number(&self) -> i32 {
        0
    }
    fn gate(&self) -> GateDecision {
        GateDecision {
            allowed: false,
            status: GateStatus::Unknown,
            can_set_rptr_shift: false,
            can_set_rptr_offs: false,
            can_set_ctcss: false,
            reason: "no radio connected".into(),
            beta_override_warning: None,
        }
    }
    fn get_ptt(&mut self) -> Result<bool, RigError> {
        Ok(false)
    }
    fn get_dcd(&mut self) -> Result<bool, RigError> {
        Ok(false)
    }
    fn set_vfo_state(&mut self, _want: &VfoState) -> Result<(), RigError> {
        Err(RigError::NotConnected)
    }
    fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
        Err(RigError::NotConnected)
    }
}

enum LiveBackend {
    Offline(OfflineRig),
    Tcp(TcpRigBackend),
}

impl RigBackend for LiveBackend {
    fn is_connected(&self) -> bool {
        match self {
            Self::Offline(b) => b.is_connected(),
            Self::Tcp(b) => b.is_connected(),
        }
    }
    fn model_name(&self) -> String {
        match self {
            Self::Offline(b) => b.model_name(),
            Self::Tcp(b) => b.model_name(),
        }
    }
    fn model_number(&self) -> i32 {
        match self {
            Self::Offline(b) => b.model_number(),
            Self::Tcp(b) => b.model_number(),
        }
    }
    fn gate(&self) -> GateDecision {
        match self {
            Self::Offline(b) => b.gate(),
            Self::Tcp(b) => b.gate(),
        }
    }
    fn get_ptt(&mut self) -> Result<bool, RigError> {
        match self {
            Self::Offline(b) => b.get_ptt(),
            Self::Tcp(b) => b.get_ptt(),
        }
    }
    fn get_dcd(&mut self) -> Result<bool, RigError> {
        match self {
            Self::Offline(b) => b.get_dcd(),
            Self::Tcp(b) => b.get_dcd(),
        }
    }
    fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError> {
        match self {
            Self::Offline(b) => b.set_vfo_state(want),
            Self::Tcp(b) => b.set_vfo_state(want),
        }
    }
    fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
        match self {
            Self::Offline(b) => b.read_vfo_state(),
            Self::Tcp(b) => b.read_vfo_state(),
        }
    }
    fn freq_step_hz(&self) -> u64 {
        match self {
            Self::Offline(b) => b.freq_step_hz(),
            Self::Tcp(b) => b.freq_step_hz(),
        }
    }
}

struct Session {
    files_dir: PathBuf,
    #[allow(dead_code)]
    data_dir: PathBuf,
    #[allow(dead_code)]
    timezone: String,
    position: Option<(f64, f64)>,
    cat: CatService<LiveBackend>,
    guest_installed: bool,
}

impl Session {
    fn enable_path(&self) -> PathBuf {
        self.files_dir.join(ENABLE_FILE)
    }
    fn kv_path(&self) -> PathBuf {
        self.files_dir.join(KV_REL)
    }
    fn plugins_root(&self) -> PathBuf {
        self.files_dir.join(PLUGINS_REL)
    }
    fn cat_dir(&self) -> PathBuf {
        self.plugins_root().join(CAT_NAME)
    }
}

static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

fn session_lock() -> &'static Mutex<Option<Session>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

fn cat_policy() -> HashSet<Capability> {
    [
        Capability::Log,
        Capability::PositionRead,
        Capability::CatStatus,
        Capability::RepeaterQuery,
        Capability::CatVfoSet,
        Capability::CatNetworkFollow,
        Capability::PluginKv,
    ]
    .into_iter()
    .collect()
}

fn open_db(files_dir: &Path) -> RepeaterDb {
    let path = files_dir.join("cat_repeaters.sqlite");
    RepeaterDb::open_path(&path).unwrap_or_else(|_| {
        RepeaterDb::open_memory().expect("in-memory repeater db")
    })
}

/// Snapshot HostApi for a guest tick — VFO/follow from the guest are refused so
/// a malicious guest cannot bypass UniFFI `CatService` read-back.
struct CatTickApi {
    position: Option<(f64, f64)>,
    status_json: String,
    sites_json: String,
    kv: FilePluginKv,
}

impl HostApi for CatTickApi {
    fn position(&self) -> Option<Position> {
        self.position.map(|(lat, lon)| Position { lat, lon })
    }
    fn poi_query(&self, _lat: f64, _lon: f64, _radius_m: f64) -> Vec<PoiWrite> {
        Vec::new()
    }
    fn poi_write(&mut self, _poi: PoiWrite) -> Result<(), String> {
        Ok(())
    }
    fn log(&mut self, message: &str) {
        log::info!(target: "NaviCat", "{message}");
    }
    fn plugin_kv_status(&self) -> PluginKvStatus {
        PluginKvStatus::Available
    }
    fn plugin_kv_get(&self, key: &str) -> Option<String> {
        self.kv.get(key)
    }
    fn plugin_kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.kv.set(key, value).map_err(|e| e.to_string())
    }
    fn cat_status(&self) -> String {
        self.status_json.clone()
    }
    fn repeater_query(
        &self,
        _lat: f64,
        _lon: f64,
        _radius_km: f64,
        _network_id: Option<&str>,
    ) -> String {
        self.sites_json.clone()
    }
    fn cat_vfo_set(&mut self, _request_json: &str) -> String {
        r#"{"ok":false,"error":"guest VFO refused — use host UniFFI"}"#.into()
    }
    fn cat_network_follow(&mut self, _request_json: &str) -> String {
        r#"{"ok":false,"error":"guest follow refused — use host UniFFI"}"#.into()
    }
}

#[uniffi::export]
pub fn cat_plugin_configure(files_dir: String, data_dir: String, timezone: String) {
    crate::init_native_logging();
    let files = PathBuf::from(files_dir);
    let data = PathBuf::from(data_dir);
    let db = open_db(&files);
    let cat = CatService::new(LiveBackend::Offline(OfflineRig), db);
    let mut guard = session_lock().lock().expect("cat session");
    *guard = Some(Session {
        files_dir: files,
        data_dir: data,
        timezone: if timezone.trim().is_empty() {
            "local".into()
        } else {
            timezone
        },
        position: None,
        cat,
        guest_installed: false,
    });
}

#[uniffi::export]
pub fn cat_plugin_install_guest(name: String, manifest_json: String, wasm_bytes: Vec<u8>) -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    if name != CAT_NAME && name != "CATS-plugin" {
        // Accept either asset folder name or plugin.json "name".
    }
    let dir = session.cat_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        return format!("FAIL: mkdir: {e}");
    }
    if let Err(e) = fs::write(dir.join("plugin.json"), manifest_json.as_bytes()) {
        return format!("FAIL: write manifest: {e}");
    }
    if let Err(e) = fs::write(dir.join("plugin.wasm"), &wasm_bytes) {
        return format!("FAIL: write wasm: {e}");
    }
    session.guest_installed = true;
    "OK: installed".into()
}

#[uniffi::export]
pub fn cat_plugin_set_enabled(enabled: bool) -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    match plugin_set_enabled(&session.enable_path(), CAT_NAME, enabled) {
        Ok(()) => {
            if !enabled {
                session.cat.follow_network_id = None;
                session.cat.follow_pinned = None;
            }
            if enabled {
                "OK: enabled".into()
            } else {
                "OK: disabled".into()
            }
        }
        Err(e) => format!("FAIL: {e}"),
    }
}

#[uniffi::export]
pub fn cat_plugin_is_enabled() -> bool {
    let guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_ref() else {
        return false;
    };
    navi_plugin_host::PluginEnableStore::open(session.enable_path())
        .map(|s| s.is_enabled(CAT_NAME))
        .unwrap_or(false)
}

#[uniffi::export]
pub fn cat_plugin_set_position(lat: f64, lon: f64) {
    let mut guard = session_lock().lock().expect("cat session");
    if let Some(session) = guard.as_mut() {
        session.position = Some((lat, lon));
    }
}

#[uniffi::export]
pub fn cat_plugin_connect_tcp(host: String, port: u16, allow_beta: bool) -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    match TcpRigBackend::connect(&host, port, allow_beta) {
        Ok(tcp) => {
            let follow_net = session.cat.follow_network_id.clone();
            let follow_pin = session.cat.follow_pinned.clone();
            let follow_stop = session.cat.follow_stopped_reason.clone();
            let db = open_db(&session.files_dir);
            let mut cat = CatService::new(LiveBackend::Tcp(tcp), db);
            cat.follow_network_id = follow_net;
            cat.follow_pinned = follow_pin;
            cat.follow_stopped_reason = follow_stop;
            session.cat = cat;
            "OK: connected".into()
        }
        Err(e) => format!("FAIL: {e}"),
    }
}

#[uniffi::export]
pub fn cat_plugin_disconnect() -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    let follow_net = session.cat.follow_network_id.clone();
    let follow_pin = session.cat.follow_pinned.clone();
    let follow_stop = session.cat.follow_stopped_reason.clone();
    let db = open_db(&session.files_dir);
    let mut cat = CatService::new(LiveBackend::Offline(OfflineRig), db);
    cat.follow_network_id = follow_net;
    cat.follow_pinned = follow_pin;
    cat.follow_stopped_reason = follow_stop;
    session.cat = cat;
    "OK: disconnected".into()
}

#[uniffi::export]
pub fn cat_plugin_status_json() -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return r#"{"connected":false,"reason":"not configured"}"#.into();
    };
    session.cat.status_json()
}

#[uniffi::export]
pub fn cat_plugin_repeater_query_json(
    lat: f64,
    lon: f64,
    radius_km: f64,
    network_id: Option<String>,
) -> String {
    let guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_ref() else {
        return "[]".into();
    };
    session
        .cat
        .repeater_query_json(lat, lon, radius_km, network_id.as_deref())
}

#[uniffi::export]
pub fn cat_plugin_vfo_set_json(request_json: String) -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return r#"{"ok":false,"error":"not configured"}"#.into();
    };
    session.cat.vfo_set_json(&request_json)
}

#[uniffi::export]
pub fn cat_plugin_network_follow_json(request_json: String) -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return r#"{"ok":false,"error":"not configured"}"#.into();
    };
    session.cat.network_follow_json(&request_json)
}

/// Load + run the CATS wasm guest once (selection / follow decision helpers).
#[uniffi::export]
pub fn cat_plugin_tick_guest() -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    if !navi_plugin_host::PluginEnableStore::open(session.enable_path())
        .map(|s| s.is_enabled(CAT_NAME))
        .unwrap_or(false)
    {
        return "FAIL: disabled".into();
    }
    if !cranelift_abi_supported() {
        return format!("FAIL: plugin unavailable on ABI {}", std::env::consts::ARCH);
    }
    let dir = session.cat_dir();
    if !dir.join("plugin.json").is_file() || !dir.join("plugin.wasm").is_file() {
        return format!("FAIL: guest not installed under {}", dir.display());
    }
    let status_json = session.cat.status_json();
    let (lat, lon) = session.position.unwrap_or((0.0, 0.0));
    let sites_json = session.cat.repeater_query_json(lat, lon, 150.0, None);
    let kv = match FilePluginKv::open(session.kv_path()) {
        Ok(k) => k,
        Err(e) => return format!("FAIL: kv: {e}"),
    };
    let api = CatTickApi {
        position: session.position,
        status_json,
        sites_json,
        kv,
    };
    let host = match PluginHost::load_dir(
        &dir,
        &cat_policy(),
        PluginLimits {
            fuel: 50_000_000,
            timeout_ms: 5_000,
            memory_bytes: DEFAULT_MEMORY_BYTES.min(8 * 1024 * 1024),
        },
    ) {
        Ok(h) => h,
        Err(e) => return format!("FAIL: load: {e}"),
    };
    match host.call(Box::new(api)) {
        Ok(outcome) => format!("OK: {outcome:?}"),
        Err(e) => format!("FAIL: {e}"),
    }
}
