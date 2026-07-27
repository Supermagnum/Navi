use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::routing::basemap::bbox_covers_point;
use crate::storage::Storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PmtilesJobStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmtilesJobRecord {
    pub id: Uuid,
    pub region_key: String,
    pub url: String,
    pub local_path: String,
    pub bytes_received: u64,
    pub total_bytes: Option<u64>,
    pub status: PmtilesJobStatus,
    pub paused: bool,
    pub min_lat: Option<f64>,
    pub min_lon: Option<f64>,
    pub max_lat: Option<f64>,
    pub max_lon: Option<f64>,
}

pub struct PmtilesJobStore<'a> {
    storage: &'a Storage,
}

impl<'a> PmtilesJobStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn create_job(
        &self,
        region_key: &str,
        url: &str,
        local_path: &Path,
        bbox: Option<[f64; 4]>,
    ) -> SqlResult<PmtilesJobRecord> {
        let id = Uuid::new_v4();
        let now = now_rfc3339();
        let (min_lat, min_lon, max_lat, max_lon) = match bbox {
            Some(b) => (Some(b[0]), Some(b[1]), Some(b[2]), Some(b[3])),
            None => (None, None, None, None),
        };
        self.storage.with_conn(|conn| {
            conn.execute(
                "INSERT INTO pmtiles_jobs
                 (id, region_key, url, local_path, bytes_received, total_bytes, status,
                  paused, min_lat, min_lon, max_lat, max_lon, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 0, NULL, ?5, 0, ?6, ?7, ?8, ?9, ?10, ?10)",
                params![
                    id.to_string(),
                    region_key,
                    url,
                    local_path.to_string_lossy(),
                    status_label(PmtilesJobStatus::Pending),
                    min_lat,
                    min_lon,
                    max_lat,
                    max_lon,
                    now,
                ],
            )?;
            Ok(())
        })?;
        self.get_job(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_job(&self, id: Uuid) -> SqlResult<Option<PmtilesJobRecord>> {
        self.storage.with_conn(|conn| read_job(conn, id))
    }

    pub fn list_jobs(&self) -> SqlResult<Vec<PmtilesJobRecord>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT id FROM pmtiles_jobs ORDER BY created_at DESC")?;
            let ids: Vec<Uuid> = stmt
                .query_map([], |row| {
                    let s: String = row.get(0)?;
                    Uuid::parse_str(&s).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })
                })?
                .collect::<SqlResult<Vec<_>>>()?;
            let mut out = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(j) = read_job(conn, id)? {
                    out.push(j);
                }
            }
            Ok(out)
        })
    }

    pub fn list_completed_covering(&self, lat: f64, lon: f64) -> SqlResult<Vec<PmtilesJobRecord>> {
        let all = self.list_jobs()?;
        Ok(all
            .into_iter()
            .filter(|j| j.status == PmtilesJobStatus::Completed)
            .filter(|j| match (j.min_lat, j.min_lon, j.max_lat, j.max_lon) {
                (Some(a), Some(b), Some(c), Some(d)) => bbox_covers_point([a, b, c, d], lat, lon),
                _ => false,
            })
            .collect())
    }

    pub fn set_status(&self, id: Uuid, status: PmtilesJobStatus, paused: bool) -> SqlResult<()> {
        let now = now_rfc3339();
        self.storage.with_conn(|conn| {
            conn.execute(
                "UPDATE pmtiles_jobs SET status = ?1, paused = ?2, updated_at = ?3 WHERE id = ?4",
                params![status_label(status), paused as i64, now, id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn set_progress(
        &self,
        id: Uuid,
        bytes_received: u64,
        total_bytes: Option<u64>,
    ) -> SqlResult<()> {
        let now = now_rfc3339();
        self.storage.with_conn(|conn| {
            conn.execute(
                "UPDATE pmtiles_jobs SET bytes_received = ?1, total_bytes = ?2, updated_at = ?3
                 WHERE id = ?4",
                params![
                    bytes_received as i64,
                    total_bytes.map(|v| v as i64),
                    now,
                    id.to_string()
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete_job(&self, id: Uuid) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute(
                "DELETE FROM pmtiles_jobs WHERE id = ?1",
                params![id.to_string()],
            )?;
            Ok(())
        })
    }
}

fn read_job(conn: &Connection, id: Uuid) -> SqlResult<Option<PmtilesJobRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, region_key, url, local_path, bytes_received, total_bytes, status, paused,
                min_lat, min_lon, max_lat, max_lon
         FROM pmtiles_jobs WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id.to_string()])?;
    if let Some(row) = rows.next()? {
        Ok(Some(map_row(row)?))
    } else {
        Ok(None)
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> SqlResult<PmtilesJobRecord> {
    let id_s: String = row.get(0)?;
    let status_s: String = row.get(6)?;
    Ok(PmtilesJobRecord {
        id: Uuid::parse_str(&id_s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        region_key: row.get(1)?,
        url: row.get(2)?,
        local_path: row.get(3)?,
        bytes_received: row.get::<_, i64>(4)? as u64,
        total_bytes: row.get::<_, Option<i64>>(5)?.map(|v| v as u64),
        status: parse_status(&status_s),
        paused: row.get::<_, i64>(7)? != 0,
        min_lat: row.get(8)?,
        min_lon: row.get(9)?,
        max_lat: row.get(10)?,
        max_lon: row.get(11)?,
    })
}

fn status_label(s: PmtilesJobStatus) -> &'static str {
    match s {
        PmtilesJobStatus::Pending => "pending",
        PmtilesJobStatus::Running => "running",
        PmtilesJobStatus::Paused => "paused",
        PmtilesJobStatus::Completed => "completed",
        PmtilesJobStatus::Cancelled => "cancelled",
        PmtilesJobStatus::Failed => "failed",
    }
}

fn parse_status(s: &str) -> PmtilesJobStatus {
    match s {
        "running" => PmtilesJobStatus::Running,
        "paused" => PmtilesJobStatus::Paused,
        "completed" => PmtilesJobStatus::Completed,
        "cancelled" => PmtilesJobStatus::Cancelled,
        "failed" => PmtilesJobStatus::Failed,
        _ => PmtilesJobStatus::Pending,
    }
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
