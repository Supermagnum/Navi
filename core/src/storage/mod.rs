//! SQLite persistence for elevation job metadata and configuration.

mod config_store;
mod elevation_jobs;
mod schema;

pub use config_store::ConfigStore;
pub use elevation_jobs::{
    ElevationJobRecord, ElevationJobStore, JobScope, JobStatus, TileRecord, TileStatus,
};
pub use schema::migrate;

use rusqlite::{Connection, Result as SqlResult};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Thread-safe SQLite access (T4 persistence tier).
#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub(crate) fn with_conn<F, T>(&self, f: F) -> SqlResult<T>
    where
        F: FnOnce(&Connection) -> SqlResult<T>,
    {
        let guard = self.conn.lock().expect("storage mutex poisoned");
        f(&guard)
    }
}
