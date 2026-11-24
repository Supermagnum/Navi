//! CoMaps metaserver discovery, `countries.txt` parse, Ed25519 verify.

use std::collections::BTreeMap;
use std::time::Duration;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use thiserror::Error;

use super::checksum::{sha256_hex_of_bytes, ChecksumError};
use crate::pack_server::{http_get_bytes, http_get_text, PackServerError};

/// CoMaps app `COUNTRIES_TXT_SIGNATURE_HEX` / catalog `countries_txt_pubkey_hex`.
pub const COMAPS_COUNTRIES_TXT_PUBKEY_HEX: &str =
    "91c0a9f6aa182371f047e256ab46489211acc2b51b13197fbe8c94eaa9749c7b";

pub const DEFAULT_METASERVER: &str = "https://cdn-us-1.comaps.app";

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum ComapsError {
    #[error("pack http: {0}")]
    Http(#[from] PackServerError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("countries.txt signature verification failed (fail closed)")]
    SignatureInvalid,
    #[error("countries.txt.sig must be exactly 64 bytes, got {0}")]
    BadSignatureLength(usize),
    #[error("invalid pubkey hex: {0}")]
    BadPubkey(String),
    #[error("map series {0:?} missing from maps.json")]
    MissingSeries(String),
    #[error("checksum: {0}")]
    Checksum(#[from] ChecksumError),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone)]
pub struct ComapsDiscovery {
    pub metaserver_url: String,
    pub map_series: String,
    pub release_version: u64,
    pub mirrors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CountriesTxt {
    pub release_version: u64,
    pub map_series: Option<String>,
    pub leaves: BTreeMap<String, CountriesTxtLeaf>,
    pub raw: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct CountriesTxtLeaf {
    pub id: String,
    pub size: u64,
    pub sha1_base64: String,
}

#[derive(Debug, Deserialize)]
struct MapsJson {
    #[serde(rename = "map-series")]
    map_series: BTreeMap<String, MapSeriesEntry>,
}

#[derive(Debug, Deserialize)]
struct MapSeriesEntry {
    latest: u64,
    #[serde(default)]
    status: Option<String>,
}

/// Preferred CoMaps map-series name + latest release when no navi catalog is available.
///
/// Picks the lexicographically newest series whose `status` is `active` (or missing).
/// Falls back to the newest series overall when none are marked active.
pub fn discover_preferred_map_series(metaserver_url: &str) -> Result<(String, u64), ComapsError> {
    let meta = metaserver_url.trim().trim_end_matches('/');
    let maps_url = format!("{meta}/meta/maps.json");
    let maps_text = http_get_text(&maps_url, DISCOVERY_TIMEOUT)?;
    let maps: MapsJson = serde_json::from_str(&maps_text)?;
    if maps.map_series.is_empty() {
        return Err(ComapsError::Other("maps.json has no map-series".into()));
    }
    let mut active: Vec<(&String, &MapSeriesEntry)> = maps
        .map_series
        .iter()
        .filter(|(_, e)| {
            e.status
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case("active"))
                .unwrap_or(true)
        })
        .collect();
    if active.is_empty() {
        active = maps.map_series.iter().collect();
    }
    active.sort_by(|(a, _), (b, _)| b.cmp(a));
    let (name, entry) = active[0];
    Ok((name.clone(), entry.latest))
}

#[derive(Debug, Deserialize)]
struct CountriesRoot {
    #[serde(default)]
    v: Option<u64>,
    #[serde(default)]
    map_series: Option<String>,
    #[serde(default)]
    g: Vec<CountriesNode>,
}

#[derive(Debug, Deserialize)]
struct CountriesNode {
    id: String,
    #[serde(default)]
    s: Option<u64>,
    #[serde(default)]
    sha1_base64: Option<String>,
    #[serde(default)]
    g: Vec<CountriesNode>,
}

/// URL-encode a leaf id like CoMaps (`quote` with safe `-_.`).
pub fn percent_encode_leaf_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for b in id.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

/// `GET {metaserver}/meta/maps.json` then `GET {metaserver}/servers` with
/// `X-OM-DataVersion`.
pub fn discover_mirrors(
    metaserver_url: &str,
    map_series: &str,
    release_version: Option<u64>,
) -> Result<ComapsDiscovery, ComapsError> {
    let meta = metaserver_url.trim().trim_end_matches('/');
    let maps_url = format!("{meta}/meta/maps.json");
    let maps_text = http_get_text(&maps_url, DISCOVERY_TIMEOUT)?;
    let maps: MapsJson = serde_json::from_str(&maps_text)?;
    let series = maps
        .map_series
        .get(map_series)
        .ok_or_else(|| ComapsError::MissingSeries(map_series.to_string()))?;
    let version = release_version.unwrap_or(series.latest);

    let servers_url = format!("{meta}/servers");
    let client = crate::download::shared_http_client();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| ComapsError::Other(e.to_string()))?;
    let body = rt.block_on(async {
        let resp = client
            .get(&servers_url)
            .timeout(DISCOVERY_TIMEOUT)
            .header(
                reqwest::header::USER_AGENT,
                super::download::NAVI_MWM_USER_AGENT,
            )
            .header("X-OM-DataVersion", version.to_string())
            .send()
            .await
            .map_err(|e| ComapsError::Other(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ComapsError::Http(PackServerError::Http(
                resp.status().as_u16(),
            )));
        }
        resp.text()
            .await
            .map_err(|e| ComapsError::Other(e.to_string()))
    })?;

    let mirrors: Vec<String> = serde_json::from_str(&body)?;
    let mirrors = mirrors
        .into_iter()
        .map(|m| m.trim().trim_end_matches('/').to_string())
        .filter(|m| !m.is_empty())
        .collect::<Vec<_>>();
    if mirrors.is_empty() {
        return Err(ComapsError::Other("empty mirror list from /servers".into()));
    }

    Ok(ComapsDiscovery {
        metaserver_url: meta.to_string(),
        map_series: map_series.to_string(),
        release_version: version,
        mirrors,
    })
}

/// Verify Ed25519 over raw `countries.txt` bytes. Fail closed on any error.
pub fn verify_countries_txt_signature(
    countries_txt: &[u8],
    signature_64: &[u8],
    pubkey_hex: &str,
) -> Result<(), ComapsError> {
    if signature_64.len() != 64 {
        return Err(ComapsError::BadSignatureLength(signature_64.len()));
    }
    let pk_bytes =
        hex::decode(pubkey_hex.trim()).map_err(|e| ComapsError::BadPubkey(e.to_string()))?;
    if pk_bytes.len() != 32 {
        return Err(ComapsError::BadPubkey(format!(
            "expected 32 bytes, got {}",
            pk_bytes.len()
        )));
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_bytes);
    let vk =
        VerifyingKey::from_bytes(&pk_arr).map_err(|e| ComapsError::BadPubkey(e.to_string()))?;
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(signature_64);
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(countries_txt, &sig)
        .map_err(|_| ComapsError::SignatureInvalid)?;
    Ok(())
}

fn collect_leaves(nodes: &[CountriesNode], out: &mut BTreeMap<String, CountriesTxtLeaf>) {
    for n in nodes {
        if let (Some(size), Some(sha1)) = (n.s, n.sha1_base64.as_ref()) {
            out.insert(
                n.id.clone(),
                CountriesTxtLeaf {
                    id: n.id.clone(),
                    size,
                    sha1_base64: sha1.clone(),
                },
            );
        }
        collect_leaves(&n.g, out);
    }
}

/// Parse CoMaps `countries.txt` (JSON body despite the `.txt` name).
pub fn parse_countries_txt(bytes: &[u8]) -> Result<CountriesTxt, ComapsError> {
    let root: CountriesRoot = serde_json::from_slice(bytes)?;
    let mut leaves = BTreeMap::new();
    collect_leaves(&root.g, &mut leaves);
    Ok(CountriesTxt {
        release_version: root.v.unwrap_or(0),
        map_series: root.map_series,
        leaves,
        raw: bytes.to_vec(),
    })
}

/// Download `countries.txt` + `.sig` from the first working mirror, verify
/// signature, optional catalog sha256/size, then parse.
pub fn fetch_and_verify_countries_txt(
    discovery: &ComapsDiscovery,
    expected_sha256: Option<&str>,
    expected_bytes: Option<u64>,
    pubkey_hex: &str,
) -> Result<CountriesTxt, ComapsError> {
    let rel = format!(
        "maps/{}/{}/countries.txt",
        discovery.map_series, discovery.release_version
    );
    let mut last_err = None;
    for mirror in &discovery.mirrors {
        let url = format!("{mirror}/{rel}");
        let sig_url = format!("{url}.sig");
        let txt = match http_get_bytes(&url, DISCOVERY_TIMEOUT) {
            Ok(b) => b,
            Err(e) => {
                last_err = Some(ComapsError::Http(e));
                continue;
            }
        };
        let sig = match http_get_bytes(&sig_url, DISCOVERY_TIMEOUT) {
            Ok(b) => b,
            Err(e) => {
                last_err = Some(ComapsError::Http(e));
                continue;
            }
        };
        if let Some(exp) = expected_bytes {
            if txt.len() as u64 != exp {
                last_err = Some(ComapsError::Checksum(ChecksumError::SizeMismatch {
                    got: txt.len() as u64,
                    expected: exp,
                }));
                continue;
            }
        }
        if let Some(exp) = expected_sha256 {
            let got = sha256_hex_of_bytes(&txt);
            if !got.eq_ignore_ascii_case(exp) {
                last_err = Some(ComapsError::Checksum(ChecksumError::Sha256Mismatch {
                    got,
                    expected: exp.to_string(),
                }));
                continue;
            }
        }
        verify_countries_txt_signature(&txt, &sig, pubkey_hex)?;
        return parse_countries_txt(&txt);
    }
    Err(last_err.unwrap_or_else(|| ComapsError::Other("all mirrors failed".into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_spaces_and_safe() {
        assert_eq!(
            percent_encode_leaf_id("Denmark_North Denmark Region"),
            "Denmark_North%20Denmark%20Region"
        );
        assert_eq!(percent_encode_leaf_id("Norway_Hedmark"), "Norway_Hedmark");
        assert_eq!(percent_encode_leaf_id("a-b_c.d"), "a-b_c.d");
    }

    #[test]
    fn signature_accepts_valid_and_rejects_tampered() {
        use ed25519_dalek::{Signer, SigningKey};

        let msg = b"countries-txt-body";
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let vk = signing.verifying_key();
        let pubkey_hex = hex::encode(vk.as_bytes());
        let sig = signing.sign(msg);
        verify_countries_txt_signature(msg, sig.to_bytes().as_ref(), &pubkey_hex).unwrap();

        // Tampered message fail closed.
        let err = verify_countries_txt_signature(b"tampered", sig.to_bytes().as_ref(), &pubkey_hex)
            .unwrap_err();
        assert!(matches!(err, ComapsError::SignatureInvalid));

        // Wrong length fail closed.
        let err =
            verify_countries_txt_signature(msg, &sig.to_bytes()[..63], &pubkey_hex).unwrap_err();
        assert!(matches!(err, ComapsError::BadSignatureLength(63)));

        // Real CoMaps pubkey rejects our synthetic signature.
        let err = verify_countries_txt_signature(
            msg,
            sig.to_bytes().as_ref(),
            COMAPS_COUNTRIES_TXT_PUBKEY_HEX,
        )
        .unwrap_err();
        assert!(matches!(err, ComapsError::SignatureInvalid));
    }

    #[test]
    fn parse_minimal_countries_json() {
        let raw = br#"{
          "id": "Countries",
          "v": 261105,
          "map_series": "2026.10.27",
          "g": [
            {"id": "World", "s": 1, "sha1_base64": "aaa="},
            {"id": "Norway", "g": [
              {"id": "Norway_Hedmark", "s": 102194292, "sha1_base64": "SYcBVbtm7e8BwcOGWpo0WV8bFK4="}
            ]}
          ]
        }"#;
        let c = parse_countries_txt(raw).unwrap();
        assert_eq!(c.release_version, 261105);
        assert_eq!(c.leaves.len(), 2);
        assert_eq!(c.leaves["Norway_Hedmark"].size, 102194292);
    }
}
