//! Offline name/address search via SQLite FTS5.

use std::path::Path;

use osmpbf::{Element, ElementReader};
use rusqlite::{params, Connection, Result as SqlResult};

use crate::storage::Storage;

#[derive(Debug, Clone)]
pub struct NameHit {
    pub osm_id: i64,
    pub name: String,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
}

/// Local FTS5 name index for settlements, POIs, huts, peaks, and named ways.
pub struct NameIndex {
    conn: Connection,
}

impl NameIndex {
    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn open(path: impl AsRef<Path>) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    fn migrate(conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS name_entries (
                osm_id INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                lat REAL NOT NULL,
                lon REAL NOT NULL
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS name_fts USING fts5(
                name,
                kind,
                content='name_entries',
                content_rowid='osm_id'
            );
            ",
        )
    }

    pub fn load_from_pbf(&mut self, path: impl AsRef<Path>) -> anyhow::Result<usize> {
        let path = path.as_ref();
        let file = std::fs::File::open(path)?;
        let reader = ElementReader::new(file);
        let mut batch: Vec<(i64, String, String, f64, f64)> = Vec::new();
        reader.for_each(|element| match element {
            Element::Node(node) => {
                if let Some(hit) = classify_named(node.id(), node.lat(), node.lon(), node.tags()) {
                    batch.push(hit);
                }
            }
            Element::DenseNode(node) => {
                if let Some(hit) = classify_named(node.id, node.lat(), node.lon(), node.tags()) {
                    batch.push(hit);
                }
            }
            _ => {}
        })?;

        // Official hiking/cycling route relations (name/ref/operator) for To/Via search.
        // Uses negative osm_id space offset is unnecessary — relation ids are distinct
        // from node ids in OSM, but we store relation id as-is (FTS rowid = osm_id).
        match crate::routing::graph::load_named_route_entries(path) {
            Ok(routes) => {
                for r in routes {
                    batch.push((r.osm_id, r.name, r.kind, r.lat, r.lon));
                }
            }
            Err(e) => {
                log::warn!("named route relation index skipped: {e:#}");
            }
        }

        let tx = self.conn.unchecked_transaction()?;
        for (osm_id, name, kind, lat, lon) in &batch {
            tx.execute(
                "INSERT OR REPLACE INTO name_entries(osm_id, name, kind, lat, lon) VALUES (?1,?2,?3,?4,?5)",
                params![osm_id, name, kind, lat, lon],
            )?;
            tx.execute(
                "INSERT INTO name_fts(rowid, name, kind) VALUES (?1,?2,?3)",
                params![osm_id, name, kind],
            )?;
        }
        tx.commit()?;
        Ok(batch.len())
    }

    /// Insert or replace one name row (tests / incremental updates).
    pub fn upsert_entry(
        &mut self,
        osm_id: i64,
        name: String,
        kind: String,
        lat: f64,
        lon: f64,
    ) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO name_entries(osm_id, name, kind, lat, lon) VALUES (?1,?2,?3,?4,?5)",
            params![osm_id, name, kind, lat, lon],
        )?;
        // Rebuild FTS row for this id (delete + insert keeps content sync).
        let _ = self.conn.execute(
            "INSERT INTO name_fts(name_fts, rowid, name, kind) VALUES('delete', ?1, NULL, NULL)",
            params![osm_id],
        );
        self.conn.execute(
            "INSERT INTO name_fts(rowid, name, kind) VALUES (?1,?2,?3)",
            params![osm_id, name, kind],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> SqlResult<Vec<NameHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let prefix = format!("{}*", q.replace('"', ""));
        let mut stmt = self.conn.prepare(
            "
            SELECT e.osm_id, e.name, e.kind, e.lat, e.lon
            FROM name_fts f
            JOIN name_entries e ON e.osm_id = f.rowid
            WHERE name_fts MATCH ?1
            LIMIT ?2
            ",
        )?;
        let rows = stmt.query_map(params![prefix, limit as i64], |row| {
            Ok(NameHit {
                osm_id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                lat: row.get(3)?,
                lon: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Entries within [radius_m] of `(lat, lon)`, nearest first (Haversine).
    ///
    /// Uses a degree bbox prefilter then exact distance — fine for place-index
    /// sizes; not a substitute for a dedicated road-edge spatial index.
    pub fn nearby(
        &self,
        lat: f64,
        lon: f64,
        radius_m: f64,
        limit: usize,
    ) -> SqlResult<Vec<NameHit>> {
        let radius_m = radius_m.max(1.0);
        let limit = limit.max(1);
        let lat_pad = radius_m / 111_320.0;
        let lon_pad = radius_m / (111_320.0 * lat.to_radians().cos().max(0.2));
        let mut stmt = self.conn.prepare(
            "
            SELECT osm_id, name, kind, lat, lon
            FROM name_entries
            WHERE lat BETWEEN ?1 AND ?2 AND lon BETWEEN ?3 AND ?4
            ",
        )?;
        let rows = stmt.query_map(
            params![lat - lat_pad, lat + lat_pad, lon - lon_pad, lon + lon_pad],
            |row| {
                Ok(NameHit {
                    osm_id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    lat: row.get(3)?,
                    lon: row.get(4)?,
                })
            },
        )?;
        let mut scored: Vec<(f64, NameHit)> = Vec::new();
        for r in rows {
            let hit = r?;
            let d_m = crate::tracks::haversine_km(lat, lon, hit.lat, hit.lon) * 1000.0;
            if d_m <= radius_m {
                scored.push((d_m, hit));
            }
        }
        scored.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(scored.into_iter().take(limit).map(|(_, h)| h).collect())
    }
}

fn classify_named<'a>(
    osm_id: i64,
    lat: f64,
    lon: f64,
    tags: impl Iterator<Item = (&'a str, &'a str)>,
) -> Option<(i64, String, String, f64, f64)> {
    let mut name = None;
    let mut addr_street = None;
    let mut addr_housenumber = None;
    let mut kind = "named".to_string();
    for (k, v) in tags {
        match k {
            "name" => name = Some(v.to_string()),
            "addr:street" => addr_street = Some(v.to_string()),
            "addr:housenumber" => addr_housenumber = Some(v.to_string()),
            "place" => kind = format!("place:{v}"),
            "tourism" => kind = format!("tourism:{v}"),
            "natural" if v == "peak" => kind = "natural:peak".into(),
            "highway" => kind = format!("highway:{v}"),
            "amenity" => kind = format!("amenity:{v}"),
            _ => {}
        }
    }
    if name.is_none() {
        if let (Some(street), Some(num)) = (addr_street.as_ref(), addr_housenumber.as_ref()) {
            name = Some(format!("{street} {num}"));
            if kind == "named" {
                kind = "addr:housenumber".into();
            }
        } else if let Some(street) = addr_street {
            name = Some(street);
            if kind == "named" {
                kind = "addr:street".into();
            }
        }
    }
    name.map(|n| (osm_id, n, kind, lat, lon))
}

/// Saved route persistence (host UI route list).
#[derive(Debug, Clone)]
pub struct SavedRoute {
    pub id: String,
    pub start_lat: f64,
    pub start_lon: f64,
    pub start_name: Option<String>,
    pub end_lat: f64,
    pub end_lon: f64,
    pub end_name: Option<String>,
    pub via_json: String,
    pub profile: String,
    pub vehicle_json: String,
    pub summary_json: String,
    pub created_at: String,
    pub last_break_lat: Option<f64>,
    pub last_break_lon: Option<f64>,
    pub last_overnight_lat: Option<f64>,
    pub last_overnight_lon: Option<f64>,
}

pub struct RouteStore<'a> {
    storage: &'a Storage,
}

impl<'a> RouteStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn insert(&self, route: &SavedRoute) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO routes(
                    id, start_lat, start_lon, start_name, end_lat, end_lon, end_name,
                    via_json, profile, vehicle_json, summary_json, created_at,
                    last_break_lat, last_break_lon, last_overnight_lat, last_overnight_lon
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                params![
                    route.id,
                    route.start_lat,
                    route.start_lon,
                    route.start_name,
                    route.end_lat,
                    route.end_lon,
                    route.end_name,
                    route.via_json,
                    route.profile,
                    route.vehicle_json,
                    route.summary_json,
                    route.created_at,
                    route.last_break_lat,
                    route.last_break_lon,
                    route.last_overnight_lat,
                    route.last_overnight_lon,
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete(&self, id: &str) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute("DELETE FROM routes WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn list(&self) -> SqlResult<Vec<SavedRoute>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, start_lat, start_lon, start_name, end_lat, end_lon, end_name,
                        via_json, profile, vehicle_json, summary_json, created_at,
                        last_break_lat, last_break_lon, last_overnight_lat, last_overnight_lon
                 FROM routes ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(SavedRoute {
                    id: row.get(0)?,
                    start_lat: row.get(1)?,
                    start_lon: row.get(2)?,
                    start_name: row.get(3)?,
                    end_lat: row.get(4)?,
                    end_lon: row.get(5)?,
                    end_name: row.get(6)?,
                    via_json: row.get(7)?,
                    profile: row.get(8)?,
                    vehicle_json: row.get(9)?,
                    summary_json: row.get(10)?,
                    created_at: row.get(11)?,
                    last_break_lat: row.get(12)?,
                    last_break_lon: row.get(13)?,
                    last_overnight_lat: row.get(14)?,
                    last_overnight_lon: row.get(15)?,
                })
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_matches_norwegian_special_chars() {
        let mut idx = NameIndex::open_in_memory().expect("mem index");
        idx.upsert_entry(1, "Mjøsvegen".into(), "highway:tertiary".into(), 60.8, 11.0)
            .unwrap();
        idx.upsert_entry(2, "Trollåsveien".into(), "named".into(), 59.8, 10.8)
            .unwrap();
        idx.upsert_entry(
            3,
            "Ævongsli 2".into(),
            "addr:housenumber".into(),
            61.0,
            11.2,
        )
        .unwrap();
        idx.upsert_entry(
            4,
            "Bjørnhollia".into(),
            "tourism:alpine_hut".into(),
            61.7,
            10.0,
        )
        .unwrap();

        let mjos = idx.search("Mjøs", 8).unwrap();
        assert!(
            mjos.iter().any(|h| h.name.contains('ø')),
            "expected ø hit, got {mjos:?}"
        );
        let aas = idx.search("Trollås", 8).unwrap();
        assert!(aas.iter().any(|h| h.name.contains('å')), "got {aas:?}");
        let ae = idx.search("Ævongsli", 8).unwrap();
        assert!(!ae.is_empty(), "æ/Æ query empty");
        let bj = idx.search("Bjørn", 8).unwrap();
        assert!(bj.iter().any(|h| h.name.contains('ø')), "got {bj:?}");
    }

    /// Document SQLite FTS5 unicode61 folding: å/ü fold to ASCII bases; æ/ø do not.
    #[test]
    fn fts_unicode61_folds_aa_but_not_ae_oe() {
        let mut idx = NameIndex::open_in_memory().expect("mem index");
        idx.upsert_entry(1, "Eldåbu".into(), "tourism:alpine_hut".into(), 61.75, 9.97)
            .unwrap();
        idx.upsert_entry(2, "Bærums Verk".into(), "place:suburb".into(), 59.94, 10.50)
            .unwrap();
        idx.upsert_entry(3, "Løten".into(), "place:village".into(), 60.82, 11.34)
            .unwrap();
        idx.upsert_entry(4, "Müllerstraße".into(), "named".into(), 52.5, 13.4)
            .unwrap();

        // Stored glyphs unchanged.
        assert_eq!(idx.search("Eldåbu", 1).unwrap()[0].name, "Eldåbu");
        assert!(idx.search("Bærum", 1).unwrap()[0].name.contains('æ'));
        assert!(idx.search("Løten", 1).unwrap()[0].name.contains('ø'));

        // å / ü fold → ASCII keyboard can find them.
        assert!(
            idx.search("Eldabu", 8)
                .unwrap()
                .iter()
                .any(|h| h.name == "Eldåbu"),
            "å should fold to a"
        );
        assert!(
            idx.search("Muller", 8)
                .unwrap()
                .iter()
                .any(|h| h.name.contains('ü') || h.name.contains("Muller")),
            "ü should fold to u"
        );

        // æ / ø do NOT fold to ae / o — ASCII approximations miss (product quirk).
        assert!(
            idx.search("Baerum", 8).unwrap().is_empty(),
            "æ must not match ae under unicode61"
        );
        assert!(
            idx.search("Loten", 8)
                .unwrap()
                .iter()
                .all(|h| h.name != "Løten"),
            "ø must not match plain o under unicode61"
        );
    }

    #[test]
    fn nearby_finds_peer_gyntvegen_address() {
        let mut idx = NameIndex::open_in_memory().expect("mem");
        idx.upsert_entry(
            1,
            "Peer Gyntvegen 1377".into(),
            "addr:housenumber".into(),
            61.420522,
            9.927719,
        )
        .unwrap();
        idx.upsert_entry(
            2,
            "Steinbrotvegen 4".into(),
            "addr:housenumber".into(),
            61.420086,
            9.927864,
        )
        .unwrap();
        let hits = idx.nearby(61.419774, 9.927647, 120.0, 8).expect("nearby");
        assert!(!hits.is_empty());
        assert!(
            hits.iter().any(|h| h.name.starts_with("Peer Gyntvegen")),
            "expected Peer Gyntvegen near fix, got {hits:?}"
        );
    }
}
