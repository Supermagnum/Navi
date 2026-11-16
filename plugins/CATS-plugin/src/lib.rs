//! CATS WASM guest: repeater selection, auto-tune decisions, network follow,
//! leave-network (decision C), and developer demo-mode sequencing.

pub mod demo;
pub mod follow;
pub mod select;

/// Auto-tune radius (km) — must match HostApi cap and docs/CAT.md.
pub const AUTO_TUNE_RADIUS_KM: f64 = 150.0;

#[no_mangle]
pub extern "C" fn plugin_main() {
    navi_plugin_sdk::host_log("cat: guest loaded");
    if let Some(pos) = navi_plugin_sdk::host_position() {
        // Heap buffer: Ostlandet near-set can exceed 64 KiB; avoid stack overflow.
        let mut buf = vec![0u8; 524_288];
        let n = navi_plugin_sdk::host_repeater_query(
            pos.lat,
            pos.lon,
            AUTO_TUNE_RADIUS_KM,
            None,
            &mut buf,
        );
        if n > 0 {
            let json = std::str::from_utf8(&buf[..n]).unwrap_or("[]");
            if let Some(best) = select::pick_best_nfm(json, pos.lat, pos.lon) {
                navi_plugin_sdk::host_log(&format!("cat: candidate {}", best.callsign));
            }
        }
        run_position_follow(pos.lat, pos.lon);
    }
}

/// Position-only follow. The host supplies parsed sites and a clock in status JSON.
/// This function records the next site id in plugin KV. It does not open files.
fn run_position_follow(lat: f64, lon: f64) {
    let mut status_buf = [0u8; 4096];
    let n = navi_plugin_sdk::host_cat_status(&mut status_buf);
    let status = std::str::from_utf8(&status_buf[..n]).unwrap_or("{}");
    let status_v: serde_json::Value = serde_json::from_str(status).unwrap_or_default();
    let choice = status_v
        .get("follow_network_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if choice.is_empty() || choice.eq_ignore_ascii_case(follow::NETWORK_OFF) {
        return;
    }
    let now = status_v
        .get("now_unix_s")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    // Must fit full host sites_json; 64 KiB truncates and yields live=0 (no VFO programs).
    let mut buf = vec![0u8; 524_288];
    let n = navi_plugin_sdk::host_repeater_query(lat, lon, AUTO_TUNE_RADIUS_KM, None, &mut buf);
    if n == 0 {
        return;
    }
    if n >= buf.len() {
        navi_plugin_sdk::host_log(&format!(
            "cat: repeater_query truncated at {} bytes",
            buf.len()
        ));
    }
    let json = std::str::from_utf8(&buf[..n]).unwrap_or("[]");
    let rows: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    if rows.is_empty() && n > 2 {
        navi_plugin_sdk::host_log(&format!(
            "cat: repeater_query parse empty n={n} head={:?}",
            json.chars().take(80).collect::<String>()
        ));
    }
    let live = follow::live_members_from_rows(&rows);
    let members = follow::select_members(choice, &live);
    if members.is_empty() {
        navi_plugin_sdk::host_log(&format!(
            "cat: follow members empty choice={choice} live={} n={n}",
            live.len()
        ));
        return;
    }
    let mut state_buf = [0u8; 4096];
    let mut state = navi_plugin_sdk::host_plugin_kv_get(follow::FOLLOW_STATE_KEY, &mut state_buf)
        .and_then(|n| std::str::from_utf8(&state_buf[..n]).ok())
        .and_then(|s| serde_json::from_str::<follow::FollowState>(s).ok())
        .unwrap_or_default();
    state.network_id = Some(choice.to_string());
    // Outcome of the host's last program attempt: confirms the switch, or restores the
    // state from before it and starts the cool-down for the refused site.
    let mut result_buf = [0u8; 512];
    if let Some(n) = navi_plugin_sdk::host_plugin_kv_get(follow::FOLLOW_RESULT_KEY, &mut result_buf)
    {
        let text = std::str::from_utf8(&result_buf[..n]).unwrap_or("");
        if let Some(r) = follow::apply_program_result_json(&mut state, text) {
            navi_plugin_sdk::host_log(&format!(
                "cat: follow program result site={} ok={} attempts={}",
                r.site_id, r.ok, r.attempts
            ));
        }
        let _ = navi_plugin_sdk::host_plugin_kv_set(follow::FOLLOW_RESULT_KEY, "");
    }
    if let follow::FollowTick::Switch { site_id } =
        follow::tick_at(&mut state, lat, lon, &members, now)
    {
        navi_plugin_sdk::host_log(&format!(
            "cat: follow switch -> {site_id} members={}",
            members.len()
        ));
        let _ = navi_plugin_sdk::host_plugin_kv_set(follow::FOLLOW_SWITCH_KEY, &site_id);
    } else {
        navi_plugin_sdk::host_log(&format!(
            "cat: follow hold site={:?} members={}",
            state.current_site,
            members.len()
        ));
    }
    if let Ok(encoded) = serde_json::to_string(&state) {
        let _ = navi_plugin_sdk::host_plugin_kv_set(follow::FOLLOW_STATE_KEY, &encoded);
    }
}
