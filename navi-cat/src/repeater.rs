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
    /// CSV-only without position: never offered for distance auto-tune.
    pub position_accurate: bool,
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
              position_accurate INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS networks (
              id TEXT PRIMARY KEY,
              name TEXT,
              default_ctcss_hz REAL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_site(&self, site: &RepeaterSite) -> anyhow::Result<()> {
        self.conn.execute(
            r#"INSERT INTO sites (id, callsign, lat, lon, freq_out_mhz, shift_mhz, ctcss_hz,
                 dcs_code, color_code, modulation, network_id, source, conflict, conflict_note,
                 position_accurate)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
               ON CONFLICT(id) DO UPDATE SET
                 callsign=excluded.callsign, lat=excluded.lat, lon=excluded.lon,
                 freq_out_mhz=excluded.freq_out_mhz, shift_mhz=excluded.shift_mhz,
                 ctcss_hz=excluded.ctcss_hz, dcs_code=excluded.dcs_code,
                 color_code=excluded.color_code, modulation=excluded.modulation,
                 network_id=excluded.network_id, source=excluded.source,
                 conflict=excluded.conflict, conflict_note=excluded.conflict_note,
                 position_accurate=excluded.position_accurate"#,
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
                format!("{:?}", site.source).to_ascii_lowercase(),
                site.conflict as i32,
                site.conflict_note,
                site.position_accurate as i32,
            ],
        )?;
        Ok(())
    }

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
                          position_accurate FROM sites"#,
            )
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| {
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
                })
            })
            .expect("query");
        let mut out = Vec::new();
        for s in rows.flatten() {
            if is_aprs(&s) || is_simplex(&s) {
                continue;
            }
            if !s.position_accurate {
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
}

fn parse_source(s: &str) -> RepeaterSource {
    match s.to_ascii_lowercase().as_str() {
        "osm" => RepeaterSource::Osm,
        "anytonecsv" | "anytone_csv" => RepeaterSource::AnytoneCsv,
        "openrepeater" => RepeaterSource::OpenRepeater,
        "radioid" | "radio_id" => RepeaterSource::RadioId,
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

/// RepeaterBook sync is permanently disabled until written API permission.
pub fn repeaterbook_sync_enabled() -> bool {
    false
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
        })
        .unwrap();
        db.upsert_site(&RepeaterSite {
            id: "aprs".into(),
            callsign: "LD2APR".into(),
            lat: Some(61.01),
            lon: Some(10.51),
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
        })
        .unwrap();
        let q = db.query_near(61.0, 10.5, 50.0, None);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].callsign, "LA5TRR");
    }

    #[test]
    fn dmr_dedupe_stable() {
        assert_eq!(
            dmr_dedupe_key(434.600, 7.6, 1),
            dmr_dedupe_key(434.600, 7.6, 1)
        );
    }

    #[test]
    fn repeaterbook_off() {
        assert!(!repeaterbook_sync_enabled());
    }
}
