//! Opt-in OSM extract updates via Geofabrik replication (`.osc.gz`) or full re-download.
//!
//! Network is never required for routing. Checks and applies only run when the user
//! (or an explicit weekly reminder they acknowledged) asks. Silent background swaps
//! of map data are intentionally not supported.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};

use super::region::download_file;

/// If the local extract is older than this many days, skip diff-chaining and
/// offer a full `*-latest.osm.pbf` re-download instead.
pub const STALENESS_FULL_REDOWNLOAD_DAYS: u64 = 28;

/// Suggested cadence for surfacing a "check for updates" reminder (not auto-apply).
pub const WEEKLY_CHECK_REMINDER_DAYS: u64 = 7;

const META_FILENAME: &str = "region_meta.json";

/// Persisted beside the local extract so update checks are deterministic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionExtractMeta {
    /// Geofabrik region path, e.g. `europe/norway/ostlandet`.
    pub geofabrik_region: String,
    pub pbf_filename: String,
    /// Osmosis/Geofabrik sequence number of the local extract, when known.
    pub local_sequence: Option<u64>,
    /// ISO-8601 or Geofabrik `timestamp=` value for the local extract.
    pub local_timestamp: Option<String>,
    /// Unix seconds when the extract was last successfully updated or provisioned.
    pub local_updated_unix: u64,
    /// Unix seconds of the last user-visible update *check* (not apply).
    pub last_check_unix: u64,
    /// When true, the UI may surface a weekly reminder; never auto-downloads.
    #[serde(default)]
    pub weekly_reminder_opt_in: bool,
}

impl RegionExtractMeta {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join(META_FILENAME)
    }

    pub fn load(data_dir: &Path) -> Result<Option<Self>> {
        let p = Self::path(data_dir);
        if !p.is_file() {
            return Ok(None);
        }
        let text = fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
        Ok(Some(serde_json::from_str(&text)?))
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        fs::create_dir_all(data_dir)?;
        let p = Self::path(data_dir);
        let text = serde_json::to_string_pretty(self)?;
        fs::write(&p, text).with_context(|| format!("write {}", p.display()))?;
        Ok(())
    }
}

/// Parsed Geofabrik `state.txt` from the region's `-updates/` directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeofabrikState {
    pub sequence_number: u64,
    pub timestamp: String,
}

/// What the host should show the user before any download starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdatePlan {
    UpToDate {
        local_sequence: Option<u64>,
        remote_sequence: u64,
        remote_timestamp: String,
    },
    /// Diff-chain is practical; user must confirm apply.
    DiffUpdate {
        from_sequence: u64,
        to_sequence: u64,
        remote_timestamp: String,
        osc_urls: Vec<String>,
        days_behind: u64,
    },
    /// Local data too old (or sequence unknown / gaps) — full latest PBF.
    FullRedownload {
        reason: String,
        latest_pbf_url: String,
        remote_timestamp: String,
        remote_sequence: u64,
        days_behind: Option<u64>,
    },
    /// No Geofabrik binding for this extract (e.g. custom corridor cut).
    Unsupported { reason: String },
}

const PENDING_PLAN_FILENAME: &str = "pending_osm_update.json";

pub fn save_pending_plan(data_dir: &Path, plan: &UpdatePlan) -> Result<()> {
    let p = data_dir.join(PENDING_PLAN_FILENAME);
    fs::write(&p, serde_json::to_string_pretty(plan)?)?;
    Ok(())
}

pub fn load_pending_plan(data_dir: &Path) -> Result<Option<UpdatePlan>> {
    let p = data_dir.join(PENDING_PLAN_FILENAME);
    if !p.is_file() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(&p)?)?))
}

pub fn clear_pending_plan(data_dir: &Path) -> Result<()> {
    let p = data_dir.join(PENDING_PLAN_FILENAME);
    if p.is_file() {
        fs::remove_file(&p)?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct UpdateApplyResult {
    pub method: String,
    pub pbf_path: PathBuf,
    pub new_sequence: Option<u64>,
    pub new_timestamp: Option<String>,
    pub bytes_downloaded: u64,
    pub report: String,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn days_between(earlier_unix: u64, later_unix: u64) -> u64 {
    later_unix.saturating_sub(earlier_unix) / 86_400
}

/// Geofabrik base URLs for a region path like `europe/norway/ostlandet`.
pub fn geofabrik_latest_pbf_url(region: &str) -> String {
    format!("https://download.geofabrik.de/{region}-latest.osm.pbf")
}

pub fn geofabrik_updates_base(region: &str) -> String {
    format!("https://download.geofabrik.de/{region}-updates")
}

pub fn geofabrik_state_url(region: &str) -> String {
    format!("{}/state.txt", geofabrik_updates_base(region))
}

/// Sequence `123456` -> `000/123/456.osc.gz` under the updates base.
pub fn geofabrik_osc_url(region: &str, sequence: u64) -> String {
    let a = sequence / 1_000_000;
    let b = (sequence / 1_000) % 1_000;
    let c = sequence % 1_000;
    format!(
        "{}/{:03}/{:03}/{:03}.osc.gz",
        geofabrik_updates_base(region),
        a,
        b,
        c
    )
}

/// Fetch and parse Geofabrik `state.txt`.
pub fn fetch_geofabrik_state(region: &str) -> Result<GeofabrikState> {
    let url = geofabrik_state_url(region);
    let text = http_get_text(&url)?;
    parse_geofabrik_state(&text).with_context(|| format!("parse state.txt from {url}"))
}

pub fn parse_geofabrik_state(text: &str) -> Result<GeofabrikState> {
    let mut sequence_number = None;
    let mut timestamp = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("sequenceNumber=") {
            sequence_number = Some(rest.trim().parse::<u64>()?);
        } else if let Some(rest) = line.strip_prefix("timestamp=") {
            // Geofabrik uses `2024-01-15T01\:02\:03Z` with backslash-escaped colons.
            timestamp = Some(rest.trim().replace('\\', ""));
        }
    }
    Ok(GeofabrikState {
        sequence_number: sequence_number.context("missing sequenceNumber")?,
        timestamp: timestamp.context("missing timestamp")?,
    })
}

fn http_get_text(url: &str) -> Result<String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(900))
            .build()?;
        let resp = client.get(url).send().await.context("HTTP GET")?;
        if !resp.status().is_success() {
            bail!("HTTP {} for {url}", resp.status());
        }
        resp.text().await.context("read body")
    })
}

/// Record / refresh metadata after a successful provision of a Geofabrik extract.
pub fn bind_geofabrik_extract(
    data_dir: &Path,
    geofabrik_region: &str,
    pbf_filename: &str,
    sequence: Option<u64>,
    timestamp: Option<String>,
) -> Result<RegionExtractMeta> {
    let now = now_unix();
    let mut meta = RegionExtractMeta::load(data_dir)?.unwrap_or(RegionExtractMeta {
        geofabrik_region: geofabrik_region.to_string(),
        pbf_filename: pbf_filename.to_string(),
        local_sequence: None,
        local_timestamp: None,
        local_updated_unix: now,
        last_check_unix: 0,
        weekly_reminder_opt_in: false,
    });
    meta.geofabrik_region = geofabrik_region.to_string();
    meta.pbf_filename = pbf_filename.to_string();
    if sequence.is_some() {
        meta.local_sequence = sequence;
    }
    if timestamp.is_some() {
        meta.local_timestamp = timestamp;
    }
    meta.local_updated_unix = now;
    meta.save(data_dir)?;
    Ok(meta)
}

/// Whether a weekly reminder should be shown (opt-in only; never downloads).
pub fn weekly_reminder_due(meta: &RegionExtractMeta) -> bool {
    if !meta.weekly_reminder_opt_in {
        return false;
    }
    days_between(meta.last_check_unix, now_unix()) >= WEEKLY_CHECK_REMINDER_DAYS
}

/// Compare local meta to Geofabrik remote state and return a user-visible plan.
///
/// Updates `last_check_unix` so weekly reminders reset after an explicit check.
pub fn check_for_updates(data_dir: &Path) -> Result<UpdatePlan> {
    let Some(mut meta) = RegionExtractMeta::load(data_dir)? else {
        return Ok(UpdatePlan::Unsupported {
            reason: "No region_meta.json — bind a Geofabrik region first, or this extract is a custom cut without replication metadata.".into(),
        });
    };
    if meta.geofabrik_region.trim().is_empty() {
        return Ok(UpdatePlan::Unsupported {
            reason: "region_meta.json has empty geofabrik_region".into(),
        });
    }

    let remote = fetch_geofabrik_state(&meta.geofabrik_region)?;
    meta.last_check_unix = now_unix();
    meta.save(data_dir)?;

    let days_behind = days_between(meta.local_updated_unix, now_unix());

    let plan = decide_update_plan(
        &meta.geofabrik_region,
        meta.local_sequence,
        days_behind,
        &remote,
        osmium_available(),
    );
    save_pending_plan(data_dir, &plan)?;
    Ok(plan)
}

/// Pure planner used by [`check_for_updates`] (and unit tests).
pub fn decide_update_plan(
    geofabrik_region: &str,
    local_sequence: Option<u64>,
    days_behind: u64,
    remote: &GeofabrikState,
    osmium: bool,
) -> UpdatePlan {
    if let Some(local_seq) = local_sequence {
        if remote.sequence_number <= local_seq {
            return UpdatePlan::UpToDate {
                local_sequence: Some(local_seq),
                remote_sequence: remote.sequence_number,
                remote_timestamp: remote.timestamp.clone(),
            };
        }
        if days_behind >= STALENESS_FULL_REDOWNLOAD_DAYS {
            return UpdatePlan::FullRedownload {
                reason: format!(
                    "Local extract is {days_behind} days old (threshold {STALENESS_FULL_REDOWNLOAD_DAYS} days); diff-chaining skipped"
                ),
                latest_pbf_url: geofabrik_latest_pbf_url(geofabrik_region),
                remote_timestamp: remote.timestamp.clone(),
                remote_sequence: remote.sequence_number,
                days_behind: Some(days_behind),
            };
        }
        let mut osc_urls = Vec::new();
        for seq in (local_seq + 1)..=remote.sequence_number {
            osc_urls.push(geofabrik_osc_url(geofabrik_region, seq));
        }
        if osc_urls.len() > 400 {
            return UpdatePlan::FullRedownload {
                reason: format!(
                    "{} diffs would be required; falling back to full latest download",
                    osc_urls.len()
                ),
                latest_pbf_url: geofabrik_latest_pbf_url(geofabrik_region),
                remote_timestamp: remote.timestamp.clone(),
                remote_sequence: remote.sequence_number,
                days_behind: Some(days_behind),
            };
        }
        if !osmium {
            return UpdatePlan::FullRedownload {
                reason: "osmium not available; skipping .osc.gz chain, full latest download".into(),
                latest_pbf_url: geofabrik_latest_pbf_url(geofabrik_region),
                remote_timestamp: remote.timestamp.clone(),
                remote_sequence: remote.sequence_number,
                days_behind: Some(days_behind),
            };
        }
        return UpdatePlan::DiffUpdate {
            from_sequence: local_seq,
            to_sequence: remote.sequence_number,
            remote_timestamp: remote.timestamp.clone(),
            osc_urls,
            days_behind,
        };
    }

    UpdatePlan::FullRedownload {
        reason: "Local Geofabrik sequence unknown; cannot safely chain .osc.gz diffs".into(),
        latest_pbf_url: geofabrik_latest_pbf_url(geofabrik_region),
        remote_timestamp: remote.timestamp.clone(),
        remote_sequence: remote.sequence_number,
        days_behind: Some(days_behind),
    }
}

/// Apply a previously computed plan. Caller must have shown the plan to the user.
pub fn apply_update_plan(data_dir: &Path, plan: &UpdatePlan) -> Result<UpdateApplyResult> {
    let meta =
        RegionExtractMeta::load(data_dir)?.context("region_meta.json required to apply updates")?;
    let pbf_path = data_dir.join(&meta.pbf_filename);
    if !pbf_path.is_file() {
        bail!("local PBF missing: {}", pbf_path.display());
    }

    let result = match plan {
        UpdatePlan::UpToDate { .. } => UpdateApplyResult {
            method: "none".into(),
            pbf_path: pbf_path.clone(),
            new_sequence: meta.local_sequence,
            new_timestamp: meta.local_timestamp.clone(),
            bytes_downloaded: 0,
            report: "Already up to date — nothing applied.\n".into(),
        },
        UpdatePlan::Unsupported { reason } => bail!("cannot apply: {reason}"),
        UpdatePlan::FullRedownload {
            latest_pbf_url,
            remote_sequence,
            remote_timestamp,
            reason,
            ..
        } => {
            let bytes = download_tracked(latest_pbf_url, &pbf_path)?;
            let mut meta = meta.clone();
            meta.local_sequence = Some(*remote_sequence);
            meta.local_timestamp = Some(remote_timestamp.clone());
            meta.local_updated_unix = now_unix();
            meta.save(data_dir)?;
            invalidate_derived(data_dir)?;
            UpdateApplyResult {
                method: "full_redownload".into(),
                pbf_path: pbf_path.clone(),
                new_sequence: Some(*remote_sequence),
                new_timestamp: Some(remote_timestamp.clone()),
                bytes_downloaded: bytes,
                report: format!(
                    "PASS\nmethod=full_redownload\nreason={reason}\nbytes={bytes}\nsequence={remote_sequence}\ntimestamp={remote_timestamp}\nUSER_VISIBLE=true\n"
                ),
            }
        }
        UpdatePlan::DiffUpdate {
            from_sequence,
            to_sequence,
            remote_timestamp,
            osc_urls,
            ..
        } => apply_osc_chain(
            data_dir,
            &meta,
            &pbf_path,
            *from_sequence,
            *to_sequence,
            remote_timestamp,
            osc_urls,
        )?,
    };
    clear_pending_plan(data_dir)?;
    Ok(result)
}

/// Apply the pending plan saved by the last [`check_for_updates`] call.
pub fn apply_pending_update(data_dir: &Path) -> Result<UpdateApplyResult> {
    let plan = load_pending_plan(data_dir)?
        .context("no pending_osm_update.json — run check_for_updates first")?;
    apply_update_plan(data_dir, &plan)
}

fn invalidate_derived(data_dir: &Path) -> Result<()> {
    let cache = data_dir.join("graph-cache");
    if cache.is_dir() {
        let _ = fs::remove_dir_all(&cache);
        fs::create_dir_all(&cache)?;
    }
    let place = data_dir.join("place_index.db");
    if place.is_file() {
        let _ = fs::remove_file(&place);
    }
    Ok(())
}

fn apply_osc_chain(
    data_dir: &Path,
    meta: &RegionExtractMeta,
    pbf_path: &Path,
    from_sequence: u64,
    to_sequence: u64,
    remote_timestamp: &str,
    osc_urls: &[String],
) -> Result<UpdateApplyResult> {
    // Check osmium *before* any .osc.gz download. Without it, skip straight to full PBF.
    if !osmium_available() {
        let latest = geofabrik_latest_pbf_url(&meta.geofabrik_region);
        let bytes = download_tracked(&latest, pbf_path)?;
        let mut meta = meta.clone();
        meta.local_sequence = Some(to_sequence);
        meta.local_timestamp = Some(remote_timestamp.to_string());
        meta.local_updated_unix = now_unix();
        meta.save(data_dir)?;
        invalidate_derived(data_dir)?;
        return Ok(UpdateApplyResult {
            method: "full_redownload".into(),
            pbf_path: pbf_path.to_path_buf(),
            new_sequence: Some(to_sequence),
            new_timestamp: Some(remote_timestamp.to_string()),
            bytes_downloaded: bytes,
            report: format!(
                "PASS\nmethod=full_redownload\nreason=osmium not available; skipped .osc.gz fetch\nfrom={from_sequence}\nto={to_sequence}\nbytes={bytes}\nUSER_VISIBLE=true\n"
            ),
        });
    }

    let updates_dir = data_dir.join("osm-updates");
    fs::create_dir_all(&updates_dir)?;
    let mut bytes_downloaded = 0u64;
    let mut osc_paths = Vec::new();
    for (i, url) in osc_urls.iter().enumerate() {
        let seq = from_sequence + 1 + i as u64;
        let dest = updates_dir.join(format!("{seq:09}.osc.gz"));
        bytes_downloaded += download_tracked(url, &dest)?;
        gunzip_probe(&dest)?;
        osc_paths.push(dest);
    }

    let out = data_dir.join(format!("{}.new.osm.pbf", meta.pbf_filename));
    let mut args = vec![
        "apply-changes".to_string(),
        "-o".into(),
        out.display().to_string(),
        pbf_path.display().to_string(),
    ];
    for p in &osc_paths {
        args.push(p.display().to_string());
    }
    let status = Command::new("osmium")
        .args(&args)
        .status()
        .context("spawn osmium apply-changes")?;
    if !status.success() {
        bail!("osmium apply-changes failed with {status}");
    }
    fs::rename(&out, pbf_path).context("replace PBF with updated extract")?;
    let mut meta = meta.clone();
    meta.local_sequence = Some(to_sequence);
    meta.local_timestamp = Some(remote_timestamp.to_string());
    meta.local_updated_unix = now_unix();
    meta.save(data_dir)?;
    invalidate_derived(data_dir)?;
    Ok(UpdateApplyResult {
        method: "osc_osmium".into(),
        pbf_path: pbf_path.to_path_buf(),
        new_sequence: Some(to_sequence),
        new_timestamp: Some(remote_timestamp.to_string()),
        bytes_downloaded,
        report: format!(
            "PASS\nmethod=osc_osmium\nfrom={from_sequence}\nto={to_sequence}\ndiffs={}\nbytes={bytes_downloaded}\nUSER_VISIBLE=true\n",
            osc_paths.len()
        ),
    })
}

fn gunzip_probe(path: &Path) -> Result<()> {
    let raw = fs::read(path)?;
    let mut dec = GzDecoder::new(raw.as_slice());
    let mut buf = [0u8; 64];
    let n = dec.read(&mut buf).context("gunzip osc")?;
    if n == 0 {
        bail!("empty osc after gunzip: {}", path.display());
    }
    Ok(())
}

fn osmium_available() -> bool {
    #[cfg(test)]
    {
        if let Some(forced) = test_hooks::force_osmium_available() {
            return forced;
        }
    }
    Command::new("osmium")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Download wrapper that records URLs under test so we can assert no wasted `.osc.gz` fetches.
fn download_tracked(url: &str, dest: &Path) -> Result<u64> {
    #[cfg(test)]
    {
        test_hooks::record_download(url);
        if let Some(bytes) = test_hooks::try_fake_download(url, dest)? {
            return Ok(bytes);
        }
    }
    download_file(url, dest)
}

#[cfg(test)]
mod test_hooks {
    use std::cell::RefCell;
    use std::io::Write;
    use std::path::Path;

    use anyhow::Result;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    thread_local! {
        static OSMIUM_FORCE: RefCell<Option<bool>> = const { RefCell::new(None) };
        static DOWNLOAD_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static FAKE_DOWNLOADS: RefCell<bool> = const { RefCell::new(false) };
    }

    pub fn force_osmium_available() -> Option<bool> {
        OSMIUM_FORCE.with(|c| *c.borrow())
    }

    pub fn set_force_osmium(v: Option<bool>) {
        OSMIUM_FORCE.with(|c| *c.borrow_mut() = v);
    }

    pub fn record_download(url: &str) {
        DOWNLOAD_LOG.with(|l| l.borrow_mut().push(url.to_string()));
    }

    pub fn take_download_log() -> Vec<String> {
        DOWNLOAD_LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
    }

    pub fn set_fake_downloads(enabled: bool) {
        FAKE_DOWNLOADS.with(|c| *c.borrow_mut() = enabled);
    }

    pub fn try_fake_download(url: &str, dest: &Path) -> Result<Option<u64>> {
        let enabled = FAKE_DOWNLOADS.with(|c| *c.borrow());
        if !enabled {
            return Ok(None);
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body: Vec<u8> = if url.ends_with(".osc.gz") {
            let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
            enc.write_all(b"<osmChange version=\"0.6\"/>")?;
            enc.finish()?
        } else {
            // Minimal non-empty stand-in for a PBF body.
            vec![0u8; 64]
        };
        std::fs::write(dest, &body)?;
        Ok(Some(body.len() as u64))
    }

    pub fn reset() {
        set_force_osmium(None);
        set_fake_downloads(false);
        let _ = take_download_log();
    }
}

/// Human-readable multi-line summary for UI / logs (never silent).
pub fn format_update_plan(plan: &UpdatePlan) -> String {
    match plan {
        UpdatePlan::UpToDate {
            local_sequence,
            remote_sequence,
            remote_timestamp,
        } => format!(
            "OSM extract is up to date.\nlocal_sequence={local_sequence:?}\nremote_sequence={remote_sequence}\nremote_timestamp={remote_timestamp}\n"
        ),
        UpdatePlan::DiffUpdate {
            from_sequence,
            to_sequence,
            remote_timestamp,
            osc_urls,
            days_behind,
        } => format!(
            "Update available via Geofabrik .osc.gz diffs (opt-in).\nfrom_sequence={from_sequence}\nto_sequence={to_sequence}\ndiffs={}\ndays_behind={days_behind}\nremote_timestamp={remote_timestamp}\nstaleness_threshold_days={}\nConfirm Apply to download and merge.\n",
            osc_urls.len(),
            STALENESS_FULL_REDOWNLOAD_DAYS
        ),
        UpdatePlan::FullRedownload {
            reason,
            latest_pbf_url,
            remote_timestamp,
            remote_sequence,
            days_behind,
        } => format!(
            "Full re-download recommended (opt-in).\nreason={reason}\nremote_sequence={remote_sequence}\nremote_timestamp={remote_timestamp}\ndays_behind={days_behind:?}\nurl={latest_pbf_url}\nConfirm Apply to replace the local extract.\n"
        ),
        UpdatePlan::Unsupported { reason } => format!("OSM update check unsupported.\nreason={reason}\n"),
    }
}

/// Set weekly reminder opt-in flag (still never auto-applies updates).
pub fn set_weekly_reminder_opt_in(data_dir: &Path, enabled: bool) -> Result<()> {
    let mut meta = RegionExtractMeta::load(data_dir)?.unwrap_or(RegionExtractMeta {
        geofabrik_region: String::new(),
        pbf_filename: String::new(),
        local_sequence: None,
        local_timestamp: None,
        local_updated_unix: now_unix(),
        last_check_unix: 0,
        weekly_reminder_opt_in: false,
    });
    meta.weekly_reminder_opt_in = enabled;
    meta.save(data_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(seq: u64) -> GeofabrikState {
        GeofabrikState {
            sequence_number: seq,
            timestamp: "2024-06-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn parse_state_txt_strips_escaped_colons() {
        let text = "\
# Sat Jan 15 01:02:03 UTC 2024
sequenceNumber=123456
timestamp=2024-01-15T01\\:02\\:03Z
";
        let s = parse_geofabrik_state(text).unwrap();
        assert_eq!(s.sequence_number, 123456);
        assert_eq!(s.timestamp, "2024-01-15T01:02:03Z");
    }

    #[test]
    fn osc_url_pads_sequence_path() {
        let u = geofabrik_osc_url("europe/norway/ostlandet", 1_234_567);
        assert_eq!(
            u,
            "https://download.geofabrik.de/europe/norway/ostlandet-updates/001/234/567.osc.gz"
        );
    }

    #[test]
    fn plan_fresh_is_up_to_date() {
        let plan = decide_update_plan("europe/norway/ostlandet", Some(100), 3, &remote(100), true);
        assert!(matches!(plan, UpdatePlan::UpToDate { .. }));
    }

    #[test]
    fn plan_diff_when_osmium_and_fresh_enough() {
        let plan = decide_update_plan("europe/norway/ostlandet", Some(100), 5, &remote(103), true);
        match plan {
            UpdatePlan::DiffUpdate {
                from_sequence,
                to_sequence,
                osc_urls,
                ..
            } => {
                assert_eq!(from_sequence, 100);
                assert_eq!(to_sequence, 103);
                assert_eq!(osc_urls.len(), 3);
                assert!(osc_urls.iter().all(|u| u.ends_with(".osc.gz")));
            }
            other => panic!("expected DiffUpdate, got {other:?}"),
        }
    }

    #[test]
    fn plan_full_when_osmium_unavailable_instead_of_diff() {
        let plan = decide_update_plan("europe/norway/ostlandet", Some(100), 5, &remote(103), false);
        match plan {
            UpdatePlan::FullRedownload { reason, .. } => {
                assert!(reason.contains("osmium not available"));
            }
            other => panic!("expected FullRedownload, got {other:?}"),
        }
    }

    #[test]
    fn plan_full_when_stale_ge_28_days() {
        let plan = decide_update_plan(
            "europe/norway/ostlandet",
            Some(100),
            STALENESS_FULL_REDOWNLOAD_DAYS,
            &remote(200),
            true,
        );
        assert!(matches!(plan, UpdatePlan::FullRedownload { .. }));
    }

    #[test]
    fn plan_full_when_sequence_unknown() {
        let plan = decide_update_plan("europe/norway/ostlandet", None, 2, &remote(50), true);
        match plan {
            UpdatePlan::FullRedownload { reason, .. } => {
                assert!(reason.contains("sequence unknown"));
            }
            other => panic!("expected FullRedownload, got {other:?}"),
        }
    }

    #[test]
    fn plan_full_when_over_400_diffs() {
        let plan = decide_update_plan(
            "europe/norway/ostlandet",
            Some(1),
            5,
            &remote(1 + 401),
            true,
        );
        match plan {
            UpdatePlan::FullRedownload { reason, .. } => {
                assert!(reason.contains("401 diffs"));
            }
            other => panic!("expected FullRedownload, got {other:?}"),
        }
    }

    #[test]
    fn no_osmium_apply_skips_osc_gz_downloads() {
        test_hooks::reset();
        test_hooks::set_force_osmium(Some(false));
        test_hooks::set_fake_downloads(true);

        let dir = tempfile::tempdir().unwrap();
        let meta = RegionExtractMeta {
            geofabrik_region: "europe/norway/ostlandet".into(),
            pbf_filename: "ostlandet-latest.osm.pbf".into(),
            local_sequence: Some(10),
            local_timestamp: Some("2024-01-01T00:00:00Z".into()),
            local_updated_unix: now_unix(),
            last_check_unix: now_unix(),
            weekly_reminder_opt_in: false,
        };
        meta.save(dir.path()).unwrap();
        let pbf = dir.path().join(&meta.pbf_filename);
        fs::write(&pbf, vec![0u8; 64]).unwrap();

        let plan = UpdatePlan::DiffUpdate {
            from_sequence: 10,
            to_sequence: 12,
            remote_timestamp: "2024-06-01T00:00:00Z".into(),
            osc_urls: vec![
                geofabrik_osc_url("europe/norway/ostlandet", 11),
                geofabrik_osc_url("europe/norway/ostlandet", 12),
            ],
            days_behind: 3,
        };
        let result = apply_update_plan(dir.path(), &plan).unwrap();
        assert_eq!(result.method, "full_redownload");
        assert!(!result.report.contains("osc_downloaded_full_fallback"));

        let log = test_hooks::take_download_log();
        assert!(
            log.iter().all(|u| !u.contains(".osc.gz")),
            "no .osc.gz must be fetched when osmium is unavailable; got {log:?}"
        );
        assert!(
            log.iter().any(|u| u.ends_with("-latest.osm.pbf")),
            "expected full PBF download; got {log:?}"
        );
        test_hooks::reset();
    }

    #[test]
    fn osmium_diff_path_records_osc_before_apply() {
        test_hooks::reset();
        test_hooks::set_force_osmium(Some(true));
        test_hooks::set_fake_downloads(true);

        let dir = tempfile::tempdir().unwrap();
        let meta = RegionExtractMeta {
            geofabrik_region: "europe/norway/ostlandet".into(),
            pbf_filename: "ostlandet-latest.osm.pbf".into(),
            local_sequence: Some(10),
            local_timestamp: Some("2024-01-01T00:00:00Z".into()),
            local_updated_unix: now_unix(),
            last_check_unix: now_unix(),
            weekly_reminder_opt_in: false,
        };
        meta.save(dir.path()).unwrap();
        fs::write(dir.path().join(&meta.pbf_filename), vec![0u8; 64]).unwrap();

        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let osmium_shim = bin.join("osmium");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(
                &osmium_shim,
                "#!/bin/sh\nout=\"\"; in=\"\"\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -o) out=\"$2\"; shift 2 ;;\n    apply-changes|--version) shift ;;\n    *) if [ -z \"$in\" ] && [ -f \"$1\" ]; then in=\"$1\"; fi; shift ;;\n  esac\ndone\nif [ -n \"$out\" ] && [ -n \"$in\" ]; then cp \"$in\" \"$out\"; fi\nexit 0\n",
            )
            .unwrap();
            fs::set_permissions(&osmium_shim, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let old_path = std::env::var_os("PATH");
        std::env::set_var(
            "PATH",
            format!(
                "{}:{}",
                bin.display(),
                old_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ),
        );
        // Use real PATH shim for osmium_available(); keep fake downloads.
        test_hooks::set_force_osmium(None);

        let plan = UpdatePlan::DiffUpdate {
            from_sequence: 10,
            to_sequence: 11,
            remote_timestamp: "2024-06-01T00:00:00Z".into(),
            osc_urls: vec![geofabrik_osc_url("europe/norway/ostlandet", 11)],
            days_behind: 2,
        };
        let result = apply_update_plan(dir.path(), &plan);
        let log = test_hooks::take_download_log();
        if let Some(old) = old_path {
            std::env::set_var("PATH", old);
        } else {
            std::env::remove_var("PATH");
        }
        test_hooks::reset();

        let result = result.expect("diff apply via shim");
        assert_eq!(result.method, "osc_osmium");
        assert!(
            log.iter().any(|u| u.ends_with(".osc.gz")),
            "osmium path must fetch .osc.gz; got {log:?}"
        );
        assert!(
            log.iter().all(|u| !u.ends_with("-latest.osm.pbf")),
            "osmium path must not fall back to full PBF; got {log:?}"
        );
    }

    #[test]
    fn plan_full_redownload_when_stale() {
        let plan = UpdatePlan::FullRedownload {
            reason: "old".into(),
            latest_pbf_url: geofabrik_latest_pbf_url("europe/norway/ostlandet"),
            remote_timestamp: "t".into(),
            remote_sequence: 9,
            days_behind: Some(40),
        };
        let text = format_update_plan(&plan);
        assert!(text.contains("Full re-download"));
        assert!(text.contains("opt-in"));
    }

    #[test]
    fn meta_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let meta = RegionExtractMeta {
            geofabrik_region: "europe/norway/ostlandet".into(),
            pbf_filename: "ostlandet-latest.osm.pbf".into(),
            local_sequence: Some(100),
            local_timestamp: Some("2024-01-01T00:00:00Z".into()),
            local_updated_unix: 1_700_000_000,
            last_check_unix: 1_700_000_000,
            weekly_reminder_opt_in: true,
        };
        meta.save(dir.path()).unwrap();
        let loaded = RegionExtractMeta::load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.local_sequence, Some(100));
        assert!(loaded.weekly_reminder_opt_in);
    }

    #[test]
    fn check_without_meta_is_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let plan = check_for_updates(dir.path()).unwrap();
        assert!(matches!(plan, UpdatePlan::Unsupported { .. }));
    }
}
