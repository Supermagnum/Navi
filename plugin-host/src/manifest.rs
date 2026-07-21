//! Plugin manifest: capabilities are checked at load time.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::abi::Capability;

/// On-disk plugin descriptor (`plugin.json` beside the `.wasm`, or standalone).
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    /// Exported entry function (default `plugin_main`).
    #[serde(default = "default_entry")]
    pub entry: String,
    /// Capabilities this plugin may use. Checked before the module is linked.
    pub capabilities: Vec<String>,
    /// Optional fuel budget override (instruction units).
    pub fuel_limit: Option<u64>,
    /// Optional wall-clock timeout override in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Relative or absolute path to the `.wasm` module.
    #[serde(default = "default_wasm")]
    pub wasm: String,
}

fn default_entry() -> String {
    "plugin_main".to_string()
}

fn default_wasm() -> String {
    "plugin.wasm".to_string()
}

impl PluginManifest {
    pub fn from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read plugin manifest {}", path.display()))?;
        let manifest: Self = serde_json::from_str(&text)
            .with_context(|| format!("parse plugin manifest {}", path.display()))?;
        if manifest.name.trim().is_empty() {
            bail!("plugin manifest missing name");
        }
        if manifest.capabilities.is_empty() {
            bail!("plugin manifest must declare at least one capability");
        }
        for cap in &manifest.capabilities {
            if Capability::parse(cap).is_none() {
                bail!("unknown capability in manifest: {cap}");
            }
        }
        Ok(manifest)
    }

    pub fn capability_set(&self) -> HashSet<Capability> {
        self.capabilities
            .iter()
            .filter_map(|s| Capability::parse(s))
            .collect()
    }
}
