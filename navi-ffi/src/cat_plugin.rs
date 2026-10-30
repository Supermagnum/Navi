//! CAT / CATS: UniFFI session over `navi-cat` + optional wasm guest install.
//!
//! Radio safety (PTT/DCD, gating, read-back, never TX) stays in `navi-cat`.
//! Product UI talks UniFFI; the WASM guest may only decide *what* to program
//! via HostApi when ticked.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use navi_cat::benshi::backend::BenshiTransport;
use navi_cat::benshi::{
    gate_native_identity, BenshiBackend, FakeBenshiPeer, InProcessTransport,
    ALLOWED_PRODUCT_VR_N76, ALLOWED_PRODUCT_VR_N76_FW,
};
use navi_cat::gating::{GateDecision, GateStatus};
use navi_cat::program::{program_vfo1_verified, ProgramRequest};
use navi_cat::types::{RigBackend, RigError, VfoState};
use navi_cat::{CatService, FfiRigBackend, RepeaterDb, TcpRigBackend};
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

/// Failure reported by a Kotlin [`BenshiBytePipe`].
#[derive(Debug, uniffi::Error)]
pub enum BenshiPipeError {
    Io { reason: String },
}

impl std::fmt::Display for BenshiPipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { reason } => f.write_str(reason),
        }
    }
}

impl std::error::Error for BenshiPipeError {}

impl From<uniffi::UnexpectedUniFFICallbackError> for BenshiPipeError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Io { reason: e.reason }
    }
}

/// Raw RFCOMM byte pipe to a bonded Benshi radio, implemented in Kotlin.
///
/// Must be bound to the SPP control UUID only, never the audio RFCOMM UUID.
/// GAIA framing and the never-transmit allowlist stay in `navi-cat`.
/// Implementations must not call back into this library.
#[uniffi::export(with_foreign)]
pub trait BenshiBytePipe: Send + Sync {
    fn write_bytes(&self, bytes: Vec<u8>) -> Result<(), BenshiPipeError>;
    /// Bytes received so far; empty after `timeout_ms` with nothing pending.
    fn read_bytes(&self, timeout_ms: u32) -> Result<Vec<u8>, BenshiPipeError>;
    fn shutdown(&self);
}

struct ForeignBenshiPipe {
    pipe: Arc<dyn BenshiBytePipe>,
}

impl Drop for ForeignBenshiPipe {
    fn drop(&mut self) {
        self.pipe.shutdown();
    }
}

/// Benshi byte path: in-process demo peer or a real radio over RFCOMM.
enum BenshiLink {
    Lab(InProcessTransport),
    Rfcomm(ForeignBenshiPipe),
}

impl BenshiTransport for BenshiLink {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), RigError> {
        match self {
            Self::Lab(t) => t.write_all(bytes),
            Self::Rfcomm(p) => p
                .pipe
                .write_bytes(bytes.to_vec())
                .map_err(|e| RigError::Protocol(format!("RFCOMM write: {e}"))),
        }
    }

    fn read_some(&mut self, timeout: Duration) -> Result<Vec<u8>, RigError> {
        match self {
            Self::Lab(t) => t.read_some(timeout),
            Self::Rfcomm(p) => {
                let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
                p.pipe
                    .read_bytes(ms)
                    .map_err(|e| RigError::Protocol(format!("RFCOMM read: {e}")))
            }
        }
    }
}

enum LiveBackend {
    Offline(OfflineRig),
    Tcp(TcpRigBackend),
    Ffi(FfiRigBackend),
    /// Native Benshi / VR-N76 family.
    Benshi(BenshiBackend<BenshiLink>),
}

impl LiveBackend {
    /// Status `link` field so the UI can tell a real radio from the demo peer.
    fn link(&self) -> &'static str {
        match self {
            Self::Offline(_) => "offline",
            Self::Tcp(_) => "tcp",
            Self::Ffi(_) => "ffi",
            Self::Benshi(b) => match b.transport() {
                BenshiLink::Lab(_) => "benshi_lab",
                BenshiLink::Rfcomm(_) => "benshi_rfcomm",
            },
        }
    }
}

impl RigBackend for LiveBackend {
    fn is_connected(&self) -> bool {
        match self {
            Self::Offline(b) => b.is_connected(),
            Self::Tcp(b) => b.is_connected(),
            Self::Ffi(b) => b.is_connected(),
            Self::Benshi(b) => b.is_connected(),
        }
    }
    fn model_name(&self) -> String {
        match self {
            Self::Offline(b) => b.model_name(),
            Self::Tcp(b) => b.model_name(),
            Self::Ffi(b) => b.model_name(),
            Self::Benshi(b) => b.model_name(),
        }
    }
    fn model_number(&self) -> i32 {
        match self {
            Self::Offline(b) => b.model_number(),
            Self::Tcp(b) => b.model_number(),
            Self::Ffi(b) => b.model_number(),
            Self::Benshi(b) => b.model_number(),
        }
    }
    fn gate(&self) -> GateDecision {
        match self {
            Self::Offline(b) => b.gate(),
            Self::Tcp(b) => b.gate(),
            Self::Ffi(b) => b.gate(),
            Self::Benshi(b) => b.gate(),
        }
    }
    fn get_ptt(&mut self) -> Result<bool, RigError> {
        match self {
            Self::Offline(b) => b.get_ptt(),
            Self::Tcp(b) => b.get_ptt(),
            Self::Ffi(b) => b.get_ptt(),
            Self::Benshi(b) => b.get_ptt(),
        }
    }
    fn get_dcd(&mut self) -> Result<bool, RigError> {
        match self {
            Self::Offline(b) => b.get_dcd(),
            Self::Tcp(b) => b.get_dcd(),
            Self::Ffi(b) => b.get_dcd(),
            Self::Benshi(b) => b.get_dcd(),
        }
    }
    fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError> {
        match self {
            Self::Offline(b) => b.set_vfo_state(want),
            Self::Tcp(b) => b.set_vfo_state(want),
            Self::Ffi(b) => b.set_vfo_state(want),
            Self::Benshi(b) => b.set_vfo_state(want),
        }
    }
    fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
        match self {
            Self::Offline(b) => b.read_vfo_state(),
            Self::Tcp(b) => b.read_vfo_state(),
            Self::Ffi(b) => b.read_vfo_state(),
            Self::Benshi(b) => b.read_vfo_state(),
        }
    }
    fn freq_step_hz(&self) -> u64 {
        match self {
            Self::Offline(b) => b.freq_step_hz(),
            Self::Tcp(b) => b.freq_step_hz(),
            Self::Ffi(b) => b.freq_step_hz(),
            Self::Benshi(b) => b.freq_step_hz(),
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
    /// Cached CATS guest — `PluginHost::load_dir` compiles Cranelift every call and
    /// was reloading on every follow tick (RFCOMM timeouts + MemAvailable spikes).
    guest_host: Option<CachedGuestHost>,
}

/// Identity of the on-disk guest used to invalidate [`Session::guest_host`].
struct CachedGuestHost {
    wasm_len: u64,
    wasm_mtime_secs: i64,
    host: PluginHost,
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
    RepeaterDb::open_path(&path)
        .unwrap_or_else(|_| RepeaterDb::open_memory().expect("in-memory repeater db"))
}

fn ensure_import_dir(files_dir: &Path) {
    let _ = navi_cat::importers::ensure_cat_import_dir(files_dir);
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
    // Keep DT_NEEDED libhamlib.so on Android when built with hamlib-ffi.
    #[cfg(feature = "hamlib-ffi")]
    navi_hamlib_sys::retain_hamlib_link();
    let files = PathBuf::from(files_dir);
    let data = PathBuf::from(data_dir);
    ensure_import_dir(&files);
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
        guest_host: None,
    });
}

#[uniffi::export]
pub fn cat_plugin_install_guest(
    name: String,
    manifest_json: String,
    wasm_bytes: Vec<u8>,
) -> String {
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
    session.guest_host = None;
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

/// Open the onboard Hamlib library (Android `libhamlib.so`) for `model`.
/// Used for the Hamlib dummy (model 1) on the emulator and for direct FFI radios.
#[uniffi::export]
pub fn cat_plugin_connect_ffi(model: i32, allow_beta: bool) -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    match FfiRigBackend::open(model, allow_beta) {
        Ok(ffi) => {
            let follow_net = session.cat.follow_network_id.clone();
            let follow_pin = session.cat.follow_pinned.clone();
            let follow_stop = session.cat.follow_stopped_reason.clone();
            let db = open_db(&session.files_dir);
            let mut cat = CatService::new(LiveBackend::Ffi(ffi), db);
            cat.follow_network_id = follow_net;
            cat.follow_pinned = follow_pin;
            cat.follow_stopped_reason = follow_stop;
            session.cat = cat;
            format!("OK: connected ffi model={model}")
        }
        Err(e) => format!("FAIL: {e}"),
    }
}

/// Handshake (GET_DEV_INFO, notification registration, settings read; no TX)
/// and install the Benshi backend as the live CAT session.
fn connect_benshi_link(session: &mut Session, link: BenshiLink, scratch_channel: u8) -> String {
    let is_lab = matches!(link, BenshiLink::Lab(_));
    let mut backend = BenshiBackend::new(link, scratch_channel);
    if !is_lab {
        // WRITE_SETTINGS (cmd 11) on VR-N76 often needs >800ms; under tablet load
        // 3s still timed out — keep headroom for RFCOMM + concurrent guest work.
        backend.set_reply_timeout(Duration::from_secs(5));
    }
    if let Err(e) = backend.handshake() {
        return format!("FAIL: {e}");
    }
    let follow_net = session.cat.follow_network_id.clone();
    let follow_pin = session.cat.follow_pinned.clone();
    let follow_stop = session.cat.follow_stopped_reason.clone();
    let db = open_db(&session.files_dir);
    let model = backend.model_name();
    let mut cat = CatService::new(LiveBackend::Benshi(backend), db);
    cat.follow_network_id = follow_net;
    cat.follow_pinned = follow_pin;
    cat.follow_stopped_reason = follow_stop;
    session.cat = cat;
    if is_lab {
        format!("OK: connected benshi lab ({model})")
    } else {
        format!("OK: connected benshi radio ({model})")
    }
}

/// Connect a real Benshi radio (VR-N76 family) over a Kotlin RFCOMM pipe.
///
/// `scratch_channel` is the channel-A slot used when frequency mode is unavailable.
#[uniffi::export]
pub fn cat_plugin_connect_benshi(scratch_channel: u8, pipe: Arc<dyn BenshiBytePipe>) -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        pipe.shutdown();
        return "FAIL: not configured".into();
    };
    let link = BenshiLink::Rfcomm(ForeignBenshiPipe { pipe });
    connect_benshi_link(session, link, scratch_channel)
}

/// Lab / developer connect: in-process FakeBenshiPeer configured as VR-N76.
///
/// Keeps VR-N76 gating + GAIA program paths linked into release `libnavi.so`
/// (LTO would otherwise strip unused `navi-cat::benshi`). Not a physical radio;
/// [`cat_plugin_connect_benshi`] is the hardware path.
#[uniffi::export]
pub fn cat_plugin_connect_benshi_lab(scratch_channel: u8) -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    let link = BenshiLink::Lab(InProcessTransport::new(FakeBenshiPeer::default_vr_n76()));
    connect_benshi_link(session, link, scratch_channel)
}

/// VR-N76 productIds accepted by the native Benshi allowlist (`0x0002`, `0x0103`).
#[uniffi::export]
pub fn cat_plugin_benshi_vr_n76_product_ids() -> Vec<u32> {
    vec![
        u32::from(ALLOWED_PRODUCT_VR_N76),
        u32::from(ALLOWED_PRODUCT_VR_N76_FW),
    ]
}

/// Probe the native VR-N76 identity gate (fail-closed on unknown productId).
#[uniffi::export]
pub fn cat_plugin_benshi_gate_product(vendor_id: u8, product_id: u32) -> String {
    let pid = u16::try_from(product_id).unwrap_or(u16::MAX);
    let g = gate_native_identity(Some(vendor_id), Some(pid));
    serde_json::json!({
        "allowed": g.allowed,
        "reason": g.reason,
        "status": format!("{:?}", g.status),
    })
    .to_string()
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
    let json = session.cat.status_json();
    let link = session.cat.backend.link();
    match serde_json::from_str::<serde_json::Value>(&json) {
        Ok(mut v) => {
            v["link"] = serde_json::json!(link);
            v.to_string()
        }
        Err(_) => json,
    }
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

/// CAT sheet list: includes excluded / position_null / DMR (info-only). Not for auto-tune.
#[uniffi::export]
pub fn cat_plugin_repeater_list_ui_json(lat: f64, lon: f64, radius_km: f64) -> String {
    let guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_ref() else {
        return "[]".into();
    };
    session.cat.repeater_list_ui_json(lat, lon, radius_km)
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
    let follow_id = serde_json::from_str::<serde_json::Value>(&status_json)
        .ok()
        .and_then(|v| {
            v.get("follow_network_id")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    // Named follow: shrink host JSON so the WASM guest query buffer is not truncated
    // (Ostlandet full near-set is ~70–80 KiB; guest used to cap at 64 KiB → live=0).
    let follow_filter = match follow_id.as_str() {
        "" | "auto" | "standalone" | "off" => None,
        name => Some(name),
    };
    let sites_json = session
        .cat
        .repeater_query_json(lat, lon, 150.0, follow_filter);
    let sites_len = sites_json.len();
    let la5mr_in_sites = sites_json.matches("\"LA5MR\"").count();
    log::info!(
        target: "NaviCat",
        "tick_prep follow={follow_id:?} filter={follow_filter:?} sites_json_len={sites_len} la5mr_mentions={la5mr_in_sites} pos={lat},{lon}"
    );
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
    let wasm_meta = match fs::metadata(dir.join("plugin.wasm")) {
        Ok(m) => m,
        Err(e) => return format!("FAIL: wasm meta: {e}"),
    };
    let wasm_len = wasm_meta.len();
    let wasm_mtime_secs = wasm_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cache_hit = session
        .guest_host
        .as_ref()
        .is_some_and(|c| c.wasm_len == wasm_len && c.wasm_mtime_secs == wasm_mtime_secs);
    if !cache_hit {
        session.guest_host = None;
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
        log::info!(
            target: "NaviCat",
            "guest_host loaded wasm_len={wasm_len} mtime={wasm_mtime_secs}"
        );
        session.guest_host = Some(CachedGuestHost {
            wasm_len,
            wasm_mtime_secs,
            host,
        });
    }
    let call = {
        let host = &session
            .guest_host
            .as_ref()
            .expect("guest_host just set")
            .host;
        host.call(Box::new(api))
    };
    // Re-open KV after the guest returns: plugin_kv_set flushes to disk on each
    // write, so cat_follow_switch is visible here when the guest requested a tune.
    if let Ok(mut kv) = FilePluginKv::open(session.kv_path()) {
        if let Some(site_id) = kv.get("cat_follow_switch") {
            let _ = kv.set("cat_follow_switch", "");
            log::info!(target: "NaviCat", "follow_switch site_id={site_id}");
            let site = session
                .cat
                .db
                .list_sites()
                .into_iter()
                .find(|s| s.id == site_id);
            let Some(site) = site else {
                log::warn!(
                    target: "NaviCat",
                    "VFO1 skip: site_id={site_id} not in repeater db",
                );
                return match call {
                    Ok(outcome) => format!("OK: {outcome:?}"),
                    Err(e) => format!("FAIL: {e}"),
                };
            };
            let burst = site.access == navi_cat::AccessTone::ToneBurst1750;
            let shift = site.shift_mhz.unwrap_or(f64::NAN);
            let req = ProgramRequest {
                freq_out_mhz: site.freq_out_mhz,
                shift_mhz: shift,
                ctcss_hz: if burst { None } else { site.ctcss_hz },
                dcs_code: if burst { None } else { site.dcs_code },
                mode: "FM".into(),
                passband_hz: 12_500,
                vfo: "VFOA".into(),
                leaves_follow: false,
            };
            match program_vfo1_verified(&mut session.cat.backend, &req) {
                Ok(reported) => {
                    log::info!(
                        target: "NaviCat",
                        "VFO1 program {} {} freq={:.3} shift={:.3} ctcss={:?} dcs={:?} readback={reported:?}",
                        site.callsign,
                        site.id,
                        site.freq_out_mhz,
                        shift,
                        if burst { None } else { site.ctcss_hz },
                        if burst { None } else { site.dcs_code },
                    );
                }
                Err(e) => {
                    log::warn!(
                        target: "NaviCat",
                        "VFO1 program FAIL {} {} freq={:.3}: {e}",
                        site.callsign,
                        site.id,
                        site.freq_out_mhz,
                    );
                }
            }
        }
    }
    match call {
        Ok(outcome) => format!("OK: {outcome:?}"),
        Err(e) => format!("FAIL: {e}"),
    }
}

/// User-pressed Update only: GET navi-server `/repeaters/…`, verify checksums,
/// atomically swap into the onboard repeater DB. Never contacts RepeaterBook or
/// RadioID directly. Missing sources (e.g. RepeaterBook) are reported and skipped.
#[uniffi::export]
pub fn cat_plugin_update_repeaters_from_server(base_url: String) -> String {
    let mut guard = session_lock().lock().expect("cat session");
    let Some(session) = guard.as_mut() else {
        return r#"{"ok":false,"error":"not configured","kept_last_good":true}"#.into();
    };
    let (drop_dir, base, consensus, elev_dir) = if base_url.trim_start().starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(&base_url).unwrap_or_default();
        let dir = v.get("dir").and_then(|d| d.as_str()).map(|s| s.to_string());
        let consensus = v
            .get("consensus")
            .and_then(|d| d.as_bool())
            .unwrap_or(false);
        let elev = v
            .get("elev")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());
        let base = v
            .get("base")
            .and_then(|d| d.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(navi_cat::mirror::DEFAULT_REPEATERS_BASE)
            .to_string();
        (dir, base, consensus, elev)
    } else if base_url.trim().is_empty() {
        (
            None,
            navi_cat::mirror::DEFAULT_REPEATERS_BASE.to_string(),
            false,
            None,
        )
    } else {
        (None, base_url, false, None)
    };
    let db_path = session.files_dir.join("cat_repeaters.sqlite");
    let local = drop_dir.as_ref().map(|dir| {
        navi_cat::repeaterbook::import_directory(std::path::Path::new(dir), &db_path, None)
    });
    let report = navi_cat::mirror::fetch_and_update_with_files(
        &base,
        &db_path,
        drop_dir.as_deref().map(std::path::Path::new),
    );
    let local_ok = local.as_ref().map(|l| l.ok).unwrap_or(false);
    if report.ok {
        if let Ok(db) = navi_cat::RepeaterDb::open_path(&db_path) {
            if consensus {
                let elev_owned = elev_dir.clone();
                let sample = move |lat: f64, lon: f64| {
                    elev_owned
                        .as_ref()
                        .and_then(|dir| crate::elevation_at(dir.clone(), lat, lon))
                };
                let _ = navi_cat::consensus::apply(&db, true, &sample, &[]);
            } else {
                let _ = navi_cat::consensus::revert_all(&db);
            }
        }
    }
    if report.ok || local_ok {
        // Reload DB handle so subsequent queries see the new rows.
        let db = open_db(&session.files_dir);
        let follow_net = session.cat.follow_network_id.clone();
        let follow_pin = session.cat.follow_pinned.clone();
        let follow_stop = session.cat.follow_stopped_reason.clone();
        let connected = session.cat.backend.is_connected();
        let model = session.cat.backend.model_name();
        let _ = (connected, model);
        // Preserve backend by taking ownership via replace with same offline/tcp —
        // simplest: reopen offline if disconnected; keep TCP by reconstructing CatService.
        let backend = std::mem::replace(&mut session.cat.backend, LiveBackend::Offline(OfflineRig));
        let mut cat = CatService::new(backend, db);
        cat.follow_network_id = follow_net;
        cat.follow_pinned = follow_pin;
        cat.follow_stopped_reason = follow_stop;
        session.cat = cat;
    }
    let mut combined = serde_json::to_value(&report).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(local) = local {
        combined["local_files"] = serde_json::to_value(&local).unwrap_or(serde_json::json!(null));
        if local.ok {
            combined["ok"] = serde_json::json!(true);
        }
    }
    combined.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct PeerPipe {
        peer: Mutex<FakeBenshiPeer>,
        fail_writes: bool,
        closed: Arc<AtomicBool>,
    }

    impl BenshiBytePipe for PeerPipe {
        fn write_bytes(&self, bytes: Vec<u8>) -> Result<(), BenshiPipeError> {
            if self.fail_writes {
                return Err(BenshiPipeError::Io {
                    reason: "socket closed".into(),
                });
            }
            self.peer
                .lock()
                .unwrap()
                .write_from_host(&bytes)
                .map_err(|reason| BenshiPipeError::Io { reason })
        }

        fn read_bytes(&self, _timeout_ms: u32) -> Result<Vec<u8>, BenshiPipeError> {
            let peer = self.peer.lock().unwrap();
            let mut out = Vec::new();
            while let Some(frame) = peer.read_to_host() {
                out.extend_from_slice(&frame);
            }
            Ok(out)
        }

        fn shutdown(&self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    fn pipe(fail_writes: bool) -> (Arc<dyn BenshiBytePipe>, Arc<AtomicBool>) {
        let closed = Arc::new(AtomicBool::new(false));
        let p = PeerPipe {
            peer: Mutex::new(FakeBenshiPeer::default_vr_n76()),
            fail_writes,
            closed: closed.clone(),
        };
        (Arc::new(p), closed)
    }

    fn link_of(status: &str) -> String {
        let v: serde_json::Value = serde_json::from_str(status).unwrap();
        v["link"].as_str().unwrap_or_default().to_string()
    }

    #[test]
    fn benshi_pipe_connect_reports_radio_link_and_closes_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let files = dir.path().to_string_lossy().to_string();
        cat_plugin_configure(files.clone(), files, String::new());

        let (good, closed) = pipe(false);
        let msg = cat_plugin_connect_benshi(5, good);
        assert_eq!(msg, "OK: connected benshi radio (VR-N76)");
        assert_eq!(link_of(&cat_plugin_status_json()), "benshi_rfcomm");
        assert!(!closed.load(Ordering::SeqCst));
        cat_plugin_disconnect();
        assert!(closed.load(Ordering::SeqCst));
        assert_eq!(link_of(&cat_plugin_status_json()), "offline");

        let msg = cat_plugin_connect_benshi_lab(5);
        assert_eq!(msg, "OK: connected benshi lab (VR-N76)");
        assert_eq!(link_of(&cat_plugin_status_json()), "benshi_lab");

        let (bad, closed) = pipe(true);
        let msg = cat_plugin_connect_benshi(5, bad);
        assert!(msg.starts_with("FAIL: "), "{msg}");
        assert!(msg.contains("socket closed"), "{msg}");
        assert!(closed.load(Ordering::SeqCst));
        assert_eq!(link_of(&cat_plugin_status_json()), "benshi_lab");
    }
}
