//! Host-owned plugin enable / disable inventory.
//!
//! Per [`docs/plugins.md`](../../docs/plugins.md): every installed plugin has a
//! user-facing on/off control; disabled plugins are not invoked. This store is
//! **not** a WASM capability — Android/desktop hosts own it.
//!
//! Existing Weather / DATEX toggles in `MapHudPrefs` predate this path and are
//! left alone; they could migrate here later once those features call through
//! the same registry.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// One discovered plugin and its enable state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginListEntry {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    /// Absolute path to the plugin directory (manifest + wasm).
    pub dir: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct EnableFile {
    /// `name` → enabled. Missing names default to **disabled** (opt-in).
    #[serde(default)]
    enabled: HashMap<String, bool>,
}

/// Persistable enable registry keyed by plugin manifest `name`.
#[derive(Debug, Clone)]
pub struct PluginEnableStore {
    path: PathBuf,
    state: EnableFile,
}

impl PluginEnableStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let state = if path.is_file() {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("read plugin enable store {}", path.display()))?;
            serde_json::from_str(&text)
                .with_context(|| format!("parse plugin enable store {}", path.display()))?
        } else {
            EnableFile::default()
        };
        Ok(Self { path, state })
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        self.state.enabled.get(name).copied().unwrap_or(false)
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        self.state.enabled.insert(name.to_string(), enabled);
        self.flush()
    }

    pub fn flush(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.path, text)
            .with_context(|| format!("write plugin enable store {}", self.path.display()))?;
        Ok(())
    }

    /// List plugins found under `plugins_root` (each subdir with `plugin.json`).
    pub fn list_installed(&self, plugins_root: &Path) -> Result<Vec<PluginListEntry>> {
        let mut out = Vec::new();
        if !plugins_root.is_dir() {
            return Ok(out);
        }
        for entry in fs::read_dir(plugins_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let dir = entry.path();
            let manifest_path = dir.join("plugin.json");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest = crate::manifest::PluginManifest::from_path(&manifest_path)?;
            out.push(PluginListEntry {
                enabled: self.is_enabled(&manifest.name),
                name: manifest.name,
                version: manifest.version,
                dir: dir.display().to_string(),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
}

/// Convenience: list + mutate using a store file beside the plugins root.
pub fn plugin_list(plugins_root: &Path, store_path: &Path) -> Result<Vec<PluginListEntry>> {
    let store = PluginEnableStore::open(store_path)?;
    store.list_installed(plugins_root)
}

pub fn plugin_set_enabled(
    store_path: &Path,
    name: &str,
    enabled: bool,
) -> Result<()> {
    let mut store = PluginEnableStore::open(store_path)?;
    store.set_enabled(name, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn defaults_disabled_until_user_enables() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("enable.json");
        let store = PluginEnableStore::open(&store_path).unwrap();
        assert!(!store.is_enabled("right_to_roam_camping"));
    }

    #[test]
    fn set_enabled_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("enable.json");
        {
            let mut store = PluginEnableStore::open(&store_path).unwrap();
            store.set_enabled("right_to_roam_camping", true).unwrap();
        }
        let store = PluginEnableStore::open(&store_path).unwrap();
        assert!(store.is_enabled("right_to_roam_camping"));
    }

    #[test]
    fn list_reads_manifest_names() {
        let root = tempfile::tempdir().unwrap();
        let plug = root.path().join("demo");
        fs::create_dir_all(&plug).unwrap();
        let mut f = fs::File::create(plug.join("plugin.json")).unwrap();
        write!(
            f,
            r#"{{"name":"demo_plug","version":"0.1.0","capabilities":["log"],"wasm":"plugin.wasm"}}"#
        )
        .unwrap();
        let store_path = root.path().join("enable.json");
        let list = plugin_list(root.path(), &store_path).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "demo_plug");
        assert!(!list[0].enabled);
        plugin_set_enabled(&store_path, "demo_plug", true).unwrap();
        let list = plugin_list(root.path(), &store_path).unwrap();
        assert!(list[0].enabled);
    }
}
