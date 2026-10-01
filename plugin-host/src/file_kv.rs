//! File-backed plugin KV (real persistence for HostApi `plugin_kv`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::abi::{HostApi, PluginKvStatus, PoiWrite, Position};

/// JSON map on disk; survives process restart when the path is under app data.
#[derive(Debug, Clone)]
pub struct FilePluginKv {
    path: PathBuf,
    map: HashMap<String, String>,
}

impl FilePluginKv {
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let map = if path.is_file() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            HashMap::new()
        };
        Ok(Self { path, map })
    }

    pub fn flush(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&self.map)?;
        fs::write(path_atomic_tmp(&self.path), &text)?;
        fs::rename(path_atomic_tmp(&self.path), &self.path)?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned().filter(|s| !s.is_empty())
    }

    pub fn set(&mut self, key: &str, value: &str) -> std::io::Result<()> {
        if value.is_empty() {
            self.map.remove(key);
        } else {
            self.map.insert(key.to_string(), value.to_string());
        }
        self.flush()
    }

    /// Snapshot of keys currently stored (for night-store prune sweeps).
    pub fn keys(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn path_atomic_tmp(path: &Path) -> PathBuf {
    path.with_extension("tmp")
}

/// Minimal HostApi that only backs `plugin_kv` from [`FilePluginKv`].
pub struct FileKvHostApi {
    pub kv: FilePluginKv,
    pub available: bool,
}

impl HostApi for FileKvHostApi {
    fn position(&self) -> Option<Position> {
        None
    }
    fn poi_query(&self, _: f64, _: f64, _: f64) -> Vec<PoiWrite> {
        Vec::new()
    }
    fn poi_write(&mut self, _: PoiWrite) -> Result<(), String> {
        Ok(())
    }
    fn log(&mut self, _: &str) {}

    fn plugin_kv_status(&self) -> PluginKvStatus {
        if self.available {
            PluginKvStatus::Available
        } else {
            PluginKvStatus::Unavailable
        }
    }

    fn plugin_kv_get(&self, key: &str) -> Option<String> {
        if !self.available {
            return None;
        }
        self.kv.get(key)
    }

    fn plugin_kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if !self.available {
            return Err("plugin_kv unavailable".into());
        }
        self.kv.set(key, value).map_err(|e| e.to_string())
    }
}
