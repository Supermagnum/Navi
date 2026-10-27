//! Onboard repeater DB: import, filter, cross-ref, query ≤ 150 km.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeaterSource {
    Osm,
    AnytoneCsv,
    OpenRepeater,
    RadioId,
    /// From navi-server mirror (never fetched client-side from repeaterbook.com).
    RepeaterBook,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeaterSite {
    pub id: String,
    pub callsign: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub freq_out_mhz: f64,
    pub shift_mhz: f64,
    pub ctcss_hz: Option<f64>,
    pub dcs_code: Option<u32>,
    pub color_code: Option<u8>,
    pub modulation: String,
    pub network_id: Option<String>,
    pub source: RepeaterSource,
    pub conflict: bool,
    pub conflict_note: Option<String>,
    pub distance_km: Option<f64>,
    /// False when `position_null` — never used for distance auto-tune.
    pub position_accurate: bool,
    /// Excluded from auto-tune / follow; still listed in UI as excluded.
    #[serde(default)]
    pub exclude_recommended: bool,
    /// Source attribution text (e.g. from navi-server current.json).
    #[serde(default)]
    pub attribution: Option<String>,
}

pub struct RepeaterDb {
    conn: Connection,
}

impl RepeaterDb {
    pub fn open_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_path(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sites (
              id TEXT PRIMARY KEY,
              callsign TEXT NOT NULL,
              lat REAL,
              lon REAL,
              freq_out_mhz REAL NOT NULL,
              shift_mhz REAL NOT NULL,
              ctcss_hz REAL,
              dcs_code INTEGER,
              color_code INTEGER,
              modulation TEXT NOT NULL,
              network_id TEXT,
              source TEXT NOT NULL,
              conflict INTEGER NOT NULL DEFAULT 0,
              conflict_note TEXT,
              position_accurate INTEGER NOT NULL DEFAULT 1,
              exclude_recommended INTEGER NOT NULL DEFAULT 0,
              attribution TEXT
            );
            CREATE TABLE IF NOT EXISTS networks (
              id TEXT PRIMARY KEY,
              name TEXT,
              default_ctcss_hz REAL
            );
            "#,
        )?;
        // Additive columns for DBs created before exclude/attribution.
        let _ = self.conn.execute_batch(
            "ALTER TABLE sites ADD COLUMN exclude_recommended INTEGER NOT NULL DEFAULT 0;",
        );
        let _ = self
            .conn
            .execute_batch("ALTER TABLE sites ADD COLUMN attribution TEXT;");
        Ok(())
    }

    pub fn upsert_site(&self, site: &RepeaterSite) -> anyhow::Result<()> {
        self.conn.execute(
            r#"INSERT INTO sites (id, callsign, lat, lon, freq_out_mhz, shift_mhz, ctcss_hz,
                 dcs_code, color_code, modulation, network_id, source, conflict, conflict_note,
                 position_accurate, exclude_recommended, attribution)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
               ON CONFLICT(id) DO UPDATE SET
                 callsign=excluded.callsign, lat=excluded.lat, lon=excluded.lon,
                 freq_out_mhz=excluded.freq_out_mhz, shift_mhz=excluded.shift_mhz,
                 ctcss_hz=excluded.ctcss_hz, dcs_code=excluded.dcs_code,
                 color_code=excluded.color_code, modulation=excluded.modulation,
                 network_id=excluded.network_id, source=excluded.source,
                 conflict=excluded.conflict, conflict_note=excluded.conflict_note,
                 position_accurate=excluded.position_accurate,
                 exclude_recommended=excluded.exclude_recommended,
                 attribution=excluded.attribution"#,
            params![
                site.id,
                site.callsign,
                site.lat,
                site.lon,
                site.freq_out_mhz,
                site.shift_mhz,
                site.ctcss_hz,
                site.dcs_code,
                site.color_code,
                site.modulation,
                site.network_id,
                source_key(site.source),
                site.conflict as i32,
                site.conflict_note,
                site.position_accurate as i32,
                site.exclude_recommended as i32,
                site.attribution,
            ],
        )?;
        Ok(())
    }

    pub fn delete_sources(&self, sources: &[RepeaterSource]) -> anyhow::Result<()> {
        for s in sources {
            self.conn.execute(
                "DELETE FROM sites WHERE source = ?1",
                params![source_key(*s)],
            )?;
        }
        Ok(())
    }

    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RepeaterSite> {
        Ok(RepeaterSite {
            id: row.get(0)?,
            callsign: row.get(1)?,
            lat: row.get(2)?,
            lon: row.get(3)?,
            freq_out_mhz: row.get(4)?,
            shift_mhz: row.get(5)?,
            ctcss_hz: row.get(6)?,
            dcs_code: row.get(7)?,
            color_code: row.get(8)?,
            modulation: row.get(9)?,
            network_id: row.get(10)?,
            source: parse_source(&row.get::<_, String>(11)?),
            conflict: row.get::<_, i32>(12)? != 0,
            conflict_note: row.get(13)?,
            distance_km: None,
            position_accurate: row.get::<_, i32>(14)? != 0,
            exclude_recommended: row.get::<_, i32>(15).unwrap_or(0) != 0,
            attribution: row.get(16)?,
        })
    }

    /// Distance auto-tune / follow: skips APRS, simplex, position_null, exclude_recommended.
    pub fn query_near(
        &self,
        lat: f64,
        lon: f64,
        radius_km: f64,
        network_id: Option<&str>,
    ) -> Vec<RepeaterSite> {
        let mut stmt = self
            .conn
            .prepare(
                r#"SELECT id, callsign, lat, lon, freq_out_mhz, shift_mhz, ctcss_hz, dcs_code,
                          color_code, modulation, network_id, source, conflict, conflict_note,
                          position_accurate, exclude_recommended, attribution FROM sites"#,
            )
            .expect("prepare");
        let rows = stmt.query_map([], Self::map_row).expect("query");
        let mut out = Vec::new();
        for s in rows.flatten() {
            if is_aprs(&s) || is_simplex(&s) {
                continue;
            }
            if !s.position_accurate || s.exclude_recommended {
                continue;
            }
            let (Some(slat), Some(slon)) = (s.lat, s.lon) else {
                continue;
            };
            if let Some(nid) = network_id {
                if s.network_id.as_deref() != Some(nid) {
                    continue;
                }
            }
            let d = haversine_km(lat, lon, slat, slon);
            if d <= radius_km {
                let mut site = s;
                site.distance_km = Some(d);
                out.push(site);
            }
        }
        out.sort_by(|a, b| {
            a.distance_km
                .partial_cmp(&b.distance_km)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }

    /// Full list for UI (includes excluded / position_null, still skips APRS).
    pub fn list_sites(&self) -> Vec<RepeaterSite> {
        let mut stmt = self
            .conn
            .prepare(
                r#"SELECT id, callsign, lat, lon, freq_out_mhz, shift_mhz, ctcss_hz, dcs_code,
                          color_code, modulation, network_id, source, conflict, conflict_note,
                          position_accurate, exclude_recommended, attribution FROM sites"#,
            )
            .expect("prepare");
        let rows = stmt.query_map([], Self::map_row).expect("query");
        rows.flatten().filter(|s| !is_aprs(s)).collect()
    }

    /// UI list within radius: includes `exclude_recommended` and `position_null`
    /// (manual-only). Auto-tune / follow must keep using [`Self::query_near`].
    pub fn list_near_ui(&self, lat: f64, lon: f64, radius_km: f64) -> Vec<RepeaterSite> {
        let mut out = Vec::new();
        for s in self.list_sites() {
            if is_simplex(&s) {
                continue;
            }
            match (s.lat, s.lon, s.position_accurate) {
                (Some(slat), Some(slon), true) => {
                    let d = haversine_km(lat, lon, slat, slon);
                    if d <= radius_km {
                        let mut site = s;
                        site.distance_km = Some(d);
                        out.push(site);
                    }
                }
                (Some(slat), Some(slon), false) => {
                    // position_null flagged but coords present — show as manual-only.
                    let d = haversine_km(lat, lon, slat, slon);
                    if d <= radius_km {
                        let mut site = s;
                        site.distance_km = Some(d);
                        out.push(site);
                    }
                }
                _ => {
                    // No usable coords: still list when browsing UI (manual selection only).
                    if !s.position_accurate {
                        let mut site = s;
                        site.distance_km = None;
                        out.push(site);
                    }
                }
            }
        }
        out.sort_by(|a, b| match (a.distance_km, b.distance_km) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.callsign.cmp(&b.callsign),
        });
        out
    }

    /// Programmable FM networks near a position. Sorted by distance to the nearest member.
    pub fn list_networks(&self, lat: f64, lon: f64, radius_km: f64) -> Vec<NetworkListing> {
        let mut groups: std::collections::BTreeMap<String, NetworkListing> =
            std::collections::BTreeMap::new();
        for s in self.list_sites() {
            if !is_fm_programmable(&s) {
                continue;
            }
            let Some(name_raw) = s.network_id.as_deref() else {
                continue;
            };
            let name = normalize_network_name(name_raw);
            if name.is_empty() {
                continue;
            }
            let (Some(slat), Some(slon)) = (s.lat, s.lon) else {
                continue;
            };
            let d = haversine_km(lat, lon, slat, slon);
            if d > radius_km {
                continue;
            }
            let entry = groups
                .entry(name.clone())
                .or_insert_with(|| NetworkListing {
                    name: name.clone(),
                    members: 0,
                    nearest_km: f64::MAX,
                    sources: Vec::new(),
                });
            entry.members += 1;
            if d < entry.nearest_km {
                entry.nearest_km = d;
            }
            let src = source_key(s.source);
            if !entry.sources.iter().any(|x| x == &src) {
                entry.sources.push(src);
            }
        }
        let mut out: Vec<_> = groups.into_values().collect();
        out.sort_by(|a, b| {
            a.nearest_km
                .partial_cmp(&b.nearest_km)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.name.cmp(&b.name))
        });
        out
    }
}

/// FM sites the tuning backend may program. Off-air rows are excluded via
/// `exclude_recommended`. Digital modes are kept in the DB but not programmed.
pub fn is_fm_programmable(s: &RepeaterSite) -> bool {
    if !s.position_accurate || s.exclude_recommended || s.lat.is_none() || s.lon.is_none() {
        return false;
    }
    let m = s.modulation.to_ascii_uppercase();
    if m.contains("DMR")
        || m.contains("D-STAR")
        || m.contains("DSTAR")
        || m.contains("C4FM")
        || m.contains("P25")
    {
        return false;
    }
    m.is_empty() || m.contains("FM") || m.contains("11K2") || m.contains("F3E")
}

pub fn normalize_network_name(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NetworkListing {
    pub name: String,
    pub members: usize,
    pub nearest_km: f64,
    pub sources: Vec<String>,
}

fn source_key(s: RepeaterSource) -> String {
    match s {
        RepeaterSource::Osm => "osm".into(),
        RepeaterSource::AnytoneCsv => "anytonecsv".into(),
        RepeaterSource::OpenRepeater => "openrepeater".into(),
        RepeaterSource::RadioId => "radioid".into(),
        RepeaterSource::RepeaterBook => "repeaterbook".into(),
        RepeaterSource::User => "user".into(),
    }
}

fn parse_source(s: &str) -> RepeaterSource {
    match s.to_ascii_lowercase().as_str() {
        "osm" => RepeaterSource::Osm,
        "anytonecsv" | "anytone_csv" => RepeaterSource::AnytoneCsv,
        "openrepeater" => RepeaterSource::OpenRepeater,
        "radioid" | "radio_id" => RepeaterSource::RadioId,
        "repeaterbook" | "repeater_book" => RepeaterSource::RepeaterBook,
        _ => RepeaterSource::User,
    }
}

pub fn is_aprs(s: &RepeaterSite) -> bool {
    let name = s.callsign.to_ascii_uppercase();
    let modu = s.modulation.to_ascii_uppercase();
    name.contains("APRS")
        || modu.contains("APRS")
        || modu.contains("AX.25")
        || (s.freq_out_mhz - 144.800).abs() < 0.001
}

pub fn is_simplex(s: &RepeaterSite) -> bool {
    s.shift_mhz.abs() < 1e-9 && s.callsign.to_ascii_uppercase().contains("VFO")
}

/// DMR dedupe key: RX + TX + color code.
pub fn dmr_dedupe_key(freq_out_mhz: f64, shift_mhz: f64, color_code: u8) -> String {
    let tx = freq_out_mhz + shift_mhz;
    format!("{freq_out_mhz:.5}|{tx:.5}|cc{color_code}")
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// Direct client RepeaterBook sync stays off. Files are imported locally.
pub fn repeaterbook_sync_enabled() -> bool {
    false
}

/// How a follow choice maps onto `query_near`. `auto` is not a network name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowQuery {
    All,
    None,
    Named(String),
}

pub fn follow_query_filter(network_id: Option<&str>) -> FollowQuery {
    match network_id.map(str::trim).filter(|s| !s.is_empty()) {
        None => FollowQuery::All,
        Some(s) if s.eq_ignore_ascii_case("auto") => FollowQuery::All,
        Some(s) if s.eq_ignore_ascii_case("off") => FollowQuery::None,
        Some(s) => FollowQuery::Named(s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_aprs_and_orders_by_distance() {
        let db = RepeaterDb::open_memory().unwrap();
        db.upsert_site(&RepeaterSite {
            id: "1".into(),
            callsign: "LA5TRR".into(),
            lat: Some(61.0),
            lon: Some(10.5),
            freq_out_mhz: 145.725,
            shift_mhz: -0.6,
            ctcss_hz: Some(88.5),
            dcs_code: None,
            color_code: None,
            modulation: "NFM".into(),
            network_id: Some("LA5MR".into()),
            source: RepeaterSource::Osm,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: true,
            exclude_recommended: false,
            attribution: None,
        })
        .unwrap();
        db.upsert_site(&RepeaterSite {
            id: "2".into(),
            callsign: "APRS".into(),
            lat: Some(61.0),
            lon: Some(10.5),
            freq_out_mhz: 144.800,
            shift_mhz: 0.0,
            ctcss_hz: None,
            dcs_code: None,
            color_code: None,
            modulation: "APRS".into(),
            network_id: None,
            source: RepeaterSource::Osm,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: true,
            exclude_recommended: false,
            attribution: None,
        })
        .unwrap();
        let q = db.query_near(61.0, 10.5, 50.0, None);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].callsign, "LA5TRR");
    }

    #[test]
    fn exclude_recommended_skipped_in_query_near() {
        let db = RepeaterDb::open_memory().unwrap();
        db.upsert_site(&RepeaterSite {
            id: "x".into(),
            callsign: "OFFAIR".into(),
            lat: Some(61.0),
            lon: Some(10.5),
            freq_out_mhz: 145.0,
            shift_mhz: -0.6,
            ctcss_hz: None,
            dcs_code: None,
            color_code: None,
            modulation: "FM".into(),
            network_id: None,
            source: RepeaterSource::RadioId,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: true,
            exclude_recommended: true,
            attribution: Some("Data courtesy of RadioID.net".into()),
        })
        .unwrap();
        assert!(db.query_near(61.0, 10.5, 50.0, None).is_empty());
        assert_eq!(db.list_sites().len(), 1);
    }

    #[test]
    fn repeaterbook_off() {
        assert!(!repeaterbook_sync_enabled());
    }

    #[test]
    fn auto_and_off_are_not_network_names() {
        assert_eq!(follow_query_filter(Some("auto")), FollowQuery::All);
        assert_eq!(follow_query_filter(Some("off")), FollowQuery::None);
        assert_eq!(
            follow_query_filter(Some("LA5MR")),
            FollowQuery::Named("LA5MR".into())
        );
    }
}
