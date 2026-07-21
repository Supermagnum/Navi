use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::routing::elevation::tile_id::HgtTileId;
use crate::storage::Storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobScope {
    Region,
    Country,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevationJobRecord {
    pub id: Uuid,
    pub scope: JobScope,
    pub country_code: Option<String>,
    pub bbox: [f64; 4],
    pub status: JobStatus,
    pub active_source: Option<String>,
    pub total_tiles: u32,
    pub completed_tiles: u32,
    pub paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileRecord {
    pub tile_id: String,
    pub source: Option<String>,
    pub status: TileStatus,
    pub bytes_received: u64,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub local_path: Option<String>,
}

pub struct ElevationJobStore<'a> {
    storage: &'a Storage,
}

impl<'a> ElevationJobStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn create_job(
        &self,
        scope: JobScope,
        country_code: Option<&str>,
        bbox: [f64; 4],
        tile_ids: &[HgtTileId],
    ) -> SqlResult<ElevationJobRecord> {
        let id = Uuid::new_v4();
        let now = now_rfc3339();
        self.storage.with_conn(|conn| {
            conn.execute(
                "INSERT INTO elevation_jobs
                 (id, scope, country_code, min_lat, min_lon, max_lat, max_lon, status,
                  total_tiles, completed_tiles, paused, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 0, ?10, ?10)",
                params![
                    id.to_string(),
                    scope_label(scope),
                    country_code,
                    bbox[0],
                    bbox[1],
                    bbox[2],
                    bbox[3],
                    status_label(JobStatus::Pending),
                    tile_ids.len() as i64,
                    now,
                ],
            )?;
            for tile in tile_ids {
                conn.execute(
                    "INSERT INTO elevation_job_tiles
                     (job_id, tile_id, status, bytes_received)
                     VALUES (?1, ?2, ?3, 0)",
                    params![
                        id.to_string(),
                        tile.to_string(),
                        tile_status_label(TileStatus::Pending),
                    ],
                )?;
            }
            Ok(())
        })?;
        self.get_job(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_job(&self, id: Uuid) -> SqlResult<Option<ElevationJobRecord>> {
        self.storage.with_conn(|conn| read_job(conn, id))
    }

    pub fn list_jobs(&self) -> SqlResult<Vec<ElevationJobRecord>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, scope, country_code, min_lat, min_lon, max_lat, max_lon, status,
                        active_source, total_tiles, completed_tiles, paused
                 FROM elevation_jobs ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(job_from_row(
                    row.get::<_, String>(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get::<_, i64>(9)? as u32,
                    row.get::<_, i64>(10)? as u32,
                    row.get::<_, i64>(11)? != 0,
                ))
            })?;
            rows.collect()
        })
    }

    pub fn set_job_status(&self, id: Uuid, status: JobStatus) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute(
                "UPDATE elevation_jobs SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status_label(status), now_rfc3339(), id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn set_paused(&self, id: Uuid, paused: bool) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            let status = if paused {
                status_label(JobStatus::Paused)
            } else {
                status_label(JobStatus::Running)
            };
            conn.execute(
                "UPDATE elevation_jobs SET paused = ?1, status = ?2, updated_at = ?3 WHERE id = ?4",
                params![paused as i64, status, now_rfc3339(), id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn set_active_source(&self, id: Uuid, source: &str) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute(
                "UPDATE elevation_jobs SET active_source = ?1, updated_at = ?2 WHERE id = ?3",
                params![source, now_rfc3339(), id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn pending_tiles(&self, job_id: Uuid) -> SqlResult<Vec<TileRecord>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT tile_id, source, status, bytes_received, total_bytes, etag, local_path
                 FROM elevation_job_tiles
                 WHERE job_id = ?1 AND status IN ('pending', 'downloading')
                 ORDER BY tile_id",
            )?;
            let rows = stmt.query_map(params![job_id.to_string()], tile_from_row)?;
            rows.collect()
        })
    }

    pub fn update_tile(
        &self,
        job_id: Uuid,
        tile_id: &str,
        status: TileStatus,
        source: Option<&str>,
        bytes_received: u64,
        total_bytes: Option<u64>,
        etag: Option<&str>,
        local_path: Option<&str>,
    ) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute(
                "UPDATE elevation_job_tiles
                 SET status = ?1, source = ?2, bytes_received = ?3, total_bytes = ?4,
                     etag = ?5, local_path = ?6
                 WHERE job_id = ?7 AND tile_id = ?8",
                params![
                    tile_status_label(status),
                    source,
                    bytes_received as i64,
                    total_bytes.map(|v| v as i64),
                    etag,
                    local_path,
                    job_id.to_string(),
                    tile_id,
                ],
            )?;
            if status == TileStatus::Completed {
                conn.execute(
                    "UPDATE elevation_jobs
                     SET completed_tiles = (
                         SELECT COUNT(*) FROM elevation_job_tiles
                         WHERE job_id = ?1 AND status = 'completed'
                     ),
                     updated_at = ?2
                     WHERE id = ?1",
                    params![job_id.to_string(), now_rfc3339()],
                )?;
            }
            Ok(())
        })
    }

    pub fn progress(&self, job_id: Uuid) -> SqlResult<(u32, u32)> {
        self.storage.with_conn(|conn| {
            let (completed, total): (i64, i64) = conn.query_row(
                "SELECT completed_tiles, total_tiles FROM elevation_jobs WHERE id = ?1",
                params![job_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            Ok((completed as u32, total as u32))
        })
    }
}

fn read_job(conn: &Connection, id: Uuid) -> SqlResult<Option<ElevationJobRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, scope, country_code, min_lat, min_lon, max_lat, max_lon, status,
                active_source, total_tiles, completed_tiles, paused
         FROM elevation_jobs WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id.to_string()])?;
    if let Some(row) = rows.next()? {
        Ok(Some(job_from_row(
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get::<_, i64>(9)? as u32,
            row.get::<_, i64>(10)? as u32,
            row.get::<_, i64>(11)? != 0,
        )))
    } else {
        Ok(None)
    }
}

fn job_from_row(
    id: String,
    scope: String,
    country_code: Option<String>,
    min_lat: f64,
    min_lon: f64,
    max_lat: f64,
    max_lon: f64,
    status: String,
    active_source: Option<String>,
    total_tiles: u32,
    completed_tiles: u32,
    paused: bool,
) -> ElevationJobRecord {
    ElevationJobRecord {
        id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
        scope: parse_scope(&scope),
        country_code,
        bbox: [min_lat, min_lon, max_lat, max_lon],
        status: parse_job_status(&status),
        active_source,
        total_tiles,
        completed_tiles,
        paused,
    }
}

fn tile_from_row(row: &rusqlite::Row<'_>) -> SqlResult<TileRecord> {
    Ok(TileRecord {
        tile_id: row.get(0)?,
        source: row.get(1)?,
        status: parse_tile_status(&row.get::<_, String>(2)?),
        bytes_received: row.get::<_, i64>(3)? as u64,
        total_bytes: row.get::<_, Option<i64>>(4)?.map(|v| v as u64),
        etag: row.get(5)?,
        local_path: row.get(6)?,
    })
}

fn scope_label(scope: JobScope) -> &'static str {
    match scope {
        JobScope::Region => "region",
        JobScope::Country => "country",
    }
}

fn parse_scope(s: &str) -> JobScope {
    if s == "country" {
        JobScope::Country
    } else {
        JobScope::Region
    }
}

fn status_label(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Pending => "pending",
        JobStatus::Running => "running",
        JobStatus::Paused => "paused",
        JobStatus::Completed => "completed",
        JobStatus::Cancelled => "cancelled",
        JobStatus::Failed => "failed",
    }
}

fn parse_job_status(s: &str) -> JobStatus {
    match s {
        "running" => JobStatus::Running,
        "paused" => JobStatus::Paused,
        "completed" => JobStatus::Completed,
        "cancelled" => JobStatus::Cancelled,
        "failed" => JobStatus::Failed,
        _ => JobStatus::Pending,
    }
}

fn tile_status_label(status: TileStatus) -> &'static str {
    match status {
        TileStatus::Pending => "pending",
        TileStatus::Downloading => "downloading",
        TileStatus::Completed => "completed",
        TileStatus::Failed => "failed",
        TileStatus::Skipped => "skipped",
    }
}

fn parse_tile_status(s: &str) -> TileStatus {
    match s {
        "downloading" => TileStatus::Downloading,
        "completed" => TileStatus::Completed,
        "failed" => TileStatus::Failed,
        "skipped" => TileStatus::Skipped,
        _ => TileStatus::Pending,
    }
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
