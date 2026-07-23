use rusqlite::{Connection, Result as SqlResult};

pub fn migrate(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS elevation_jobs (
            id TEXT PRIMARY KEY NOT NULL,
            scope TEXT NOT NULL,
            country_code TEXT,
            min_lat REAL NOT NULL,
            min_lon REAL NOT NULL,
            max_lat REAL NOT NULL,
            max_lon REAL NOT NULL,
            status TEXT NOT NULL,
            active_source TEXT,
            total_tiles INTEGER NOT NULL DEFAULT 0,
            completed_tiles INTEGER NOT NULL DEFAULT 0,
            paused INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS elevation_job_tiles (
            job_id TEXT NOT NULL,
            tile_id TEXT NOT NULL,
            source TEXT,
            status TEXT NOT NULL,
            bytes_received INTEGER NOT NULL DEFAULT 0,
            total_bytes INTEGER,
            etag TEXT,
            local_path TEXT,
            PRIMARY KEY (job_id, tile_id),
            FOREIGN KEY (job_id) REFERENCES elevation_jobs(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS app_config (
            key TEXT PRIMARY KEY NOT NULL,
            value_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS routes (
            id TEXT PRIMARY KEY NOT NULL,
            start_lat REAL NOT NULL,
            start_lon REAL NOT NULL,
            start_name TEXT,
            end_lat REAL NOT NULL,
            end_lon REAL NOT NULL,
            end_name TEXT,
            via_json TEXT NOT NULL DEFAULT '[]',
            profile TEXT NOT NULL,
            vehicle_json TEXT NOT NULL DEFAULT '{}',
            summary_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            last_break_lat REAL,
            last_break_lon REAL,
            last_overnight_lat REAL,
            last_overnight_lon REAL
        );

        CREATE INDEX IF NOT EXISTS idx_elevation_job_tiles_status
            ON elevation_job_tiles(job_id, status);

        CREATE TABLE IF NOT EXISTS pmtiles_jobs (
            id TEXT PRIMARY KEY NOT NULL,
            region_key TEXT NOT NULL,
            url TEXT NOT NULL,
            local_path TEXT NOT NULL,
            bytes_received INTEGER NOT NULL DEFAULT 0,
            total_bytes INTEGER,
            status TEXT NOT NULL,
            paused INTEGER NOT NULL DEFAULT 0,
            min_lat REAL,
            min_lon REAL,
            max_lat REAL,
            max_lon REAL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_pmtiles_jobs_region
            ON pmtiles_jobs(region_key);
        ",
    )
}
