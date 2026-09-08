//! Read-only DATEX GETs against a navi-server DocumentRoot (no NPRA credentials).

use std::time::Duration;

use serde::Deserialize;

use crate::pack_server::{self, PackServerError, CONNECTIVITY_TIMEOUT};

use super::config::{DATEX_SITUATION_PATH, DATEX_SOURCE_PATH};

const SITUATION_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DatexSourceMeta {
    pub schema: Option<u32>,
    pub source: Option<String>,
    pub license: Option<String>,
    pub attribution: Option<String>,
    pub endpoints: Option<Vec<String>>,
}

/// DATEX-layer errors. Keep parse/malformed separate from
/// [`PackServerError`] (transport / HTTP only) — do not overload pack-server
/// variants for XML or source.json schema failures.
#[derive(Debug, thiserror::Error)]
pub enum DatexFetchError {
    #[error("datex unavailable (HTTP 404)")]
    Unavailable,
    #[error(transparent)]
    Pack(#[from] PackServerError),
    #[error("source.json: {0}")]
    Source(String),
}

/// Probe `/datex/source.json` via shared [`pack_server::probe_path`] (same
/// shape as [`pack_server::probe_current_json`], different path).
pub fn probe_datex_source(base: &str) -> Result<bool, PackServerError> {
    pack_server::probe_path(base, DATEX_SOURCE_PATH)
}

/// Fetch attribution metadata. HTTP 404 means DATEX is off or not yet cached.
pub fn fetch_source_meta(base: &str) -> Result<(DatexSourceMeta, String), DatexFetchError> {
    let url = format!("{}{}", base.trim_end_matches('/'), DATEX_SOURCE_PATH);
    let text = match pack_server::http_get_text(&url, CONNECTIVITY_TIMEOUT) {
        Ok(t) => t,
        Err(PackServerError::Http(404)) => return Err(DatexFetchError::Unavailable),
        Err(e) => return Err(e.into()),
    };
    let meta: DatexSourceMeta =
        serde_json::from_str(&text).map_err(|e| DatexFetchError::Source(e.to_string()))?;
    Ok((meta, text))
}

/// Fetch cached GetSituation XML (caller already confirmed source.json).
pub fn fetch_situation_xml_only(base: &str) -> Result<String, DatexFetchError> {
    let url = format!("{}{}", base.trim_end_matches('/'), DATEX_SITUATION_PATH);
    match pack_server::http_get_text(&url, SITUATION_TIMEOUT) {
        Ok(t) => Ok(t),
        Err(PackServerError::Http(404)) => Err(DatexFetchError::Unavailable),
        Err(e) => Err(e.into()),
    }
}

/// Fetch source.json then GetSituation.xml (legacy one-shot helper).
pub fn fetch_situation_xml(base: &str) -> Result<(DatexSourceMeta, String), DatexFetchError> {
    let (meta, _) = fetch_source_meta(base)?;
    let xml = fetch_situation_xml_only(base)?;
    Ok((meta, xml))
}
