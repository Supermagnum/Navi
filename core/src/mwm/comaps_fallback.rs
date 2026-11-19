//! Catalog resolve when navi-server MWM leaves are missing or unreachable.
//!
//! Order (vehicle-limits coherent):
//! 1. Live navi catalog with a non-empty `leaves` list (full sidecars + coverage).
//! 2. On-disk `mwm/catalog-cache.json` or active `releases/{v}/catalog.json`
//!    (keeps prior `vehicle_limits_coverage` + sidecar URLs; soft-fail sidecars
//!    if the host is still down).
//! 3. CoMaps-direct: metaserver + verified `countries.txt` → synthetic catalog
//!    with `no_sidecar` leaves. Routing/maps work; VL honesty banners report
//!    unavailable until a matching Navi sidecar exists (never invents coverage).
//!
//! Does not reintroduce `.rkyv` / Geofabrik pack convert. MWMs come from CoMaps
//! mirrors; VL data remains the Navi sidecar path when available.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::catalog::{
    CatalogLeaf, ComapsMeta, CountryGroup, CountryGroupLeaf, DisplayNameOverrides, MwmCatalog,
    SCHEMA_V2,
};
use super::checksum::sha256_hex_of_bytes;
use super::comaps::{
    discover_mirrors, discover_preferred_map_series, fetch_and_verify_countries_txt,
    COMAPS_COUNTRIES_TXT_PUBKEY_HEX, DEFAULT_METASERVER,
};
use super::download::{fetch_current_catalog_text, MwmDownloadError};
use super::flag::mwm_client_enabled;
use super::release::read_active_version;
use super::storage::{catalog_snapshot_path, release_dir, MwmPaths};

/// Where [`resolve_mwm_catalog`] obtained the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogResolveSource {
    /// Live `{origin}/mwm/current.json` with leaves.
    NaviServer,
    /// `catalog-cache.json` or active release snapshot.
    DiskCache,
    /// Synthetic catalog from CoMaps mirrors (no Navi sidecars / VL coverage).
    ComapsDirect,
}

impl CatalogResolveSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NaviServer => "navi-server",
            Self::DiskCache => "disk-cache",
            Self::ComapsDirect => "comaps-direct",
        }
    }

    /// Soft-fail missing sidecars so maps still install; VL honesty covers the gap.
    pub fn allow_missing_sidecar(self) -> bool {
        !matches!(self, Self::NaviServer)
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedCatalog {
    pub catalog: MwmCatalog,
    pub source: CatalogResolveSource,
    /// Original JSON when known (live fetch or disk); used to preserve coverage fields.
    pub raw_json: Option<String>,
}

fn catalog_usable(cat: &MwmCatalog) -> bool {
    !cat.leaves.is_empty()
}

fn catalog_cache_path(data_dir: &Path) -> std::path::PathBuf {
    MwmPaths::new(data_dir).root.join("catalog-cache.json")
}

fn load_catalog_file(path: &Path) -> Option<(MwmCatalog, String)> {
    let text = fs::read_to_string(path).ok()?;
    let cat = super::catalog::parse_catalog_json(&text).ok()?;
    catalog_usable(&cat).then_some((cat, text))
}

fn load_disk_catalog(data_dir: &Path) -> Option<(MwmCatalog, String)> {
    if let Some(hit) = load_catalog_file(&catalog_cache_path(data_dir)) {
        return Some(hit);
    }
    let active = read_active_version(data_dir).ok().flatten()?;
    load_catalog_file(&catalog_snapshot_path(&release_dir(
        data_dir,
        &active.version,
    )))
}

/// Resolve a catalog for install / region list without depending solely on live navi leaves.
pub fn resolve_mwm_catalog(
    navi_origin: &str,
    data_dir: Option<&Path>,
) -> Result<ResolvedCatalog, MwmDownloadError> {
    if !mwm_client_enabled() {
        return Err(MwmDownloadError::Disabled);
    }

    match fetch_current_catalog_text(navi_origin) {
        Ok((cat, text)) if catalog_usable(&cat) => {
            if let Some(dir) = data_dir {
                let cache = catalog_cache_path(dir);
                let _ = fs::create_dir_all(cache.parent().unwrap_or(dir));
                let _ = fs::write(&cache, &text);
            }
            return Ok(ResolvedCatalog {
                catalog: cat,
                source: CatalogResolveSource::NaviServer,
                raw_json: Some(text),
            });
        }
        Ok(_) => {
            // Empty leaves — fall through.
        }
        Err(_) => {
            // Unreachable / parse — fall through.
        }
    }

    if let Some(dir) = data_dir {
        if let Some((cat, text)) = load_disk_catalog(dir) {
            return Ok(ResolvedCatalog {
                catalog: cat,
                source: CatalogResolveSource::DiskCache,
                raw_json: Some(text),
            });
        }
    }

    let cat = fetch_comaps_direct_catalog(data_dir)?;
    let raw = serde_json::to_string(&cat).map_err(|e| MwmDownloadError::Other(e.to_string()))?;
    Ok(ResolvedCatalog {
        catalog: cat,
        source: CatalogResolveSource::ComapsDirect,
        raw_json: Some(raw),
    })
}

/// Build a synthetic catalog from CoMaps (maps only; VL coverage omitted on purpose).
pub fn fetch_comaps_direct_catalog(
    data_dir: Option<&Path>,
) -> Result<MwmCatalog, MwmDownloadError> {
    // Prefer the CoMaps pin from a prior navi catalog so on-disk sidecars can still match.
    let pinned = data_dir
        .and_then(load_disk_catalog)
        .map(|(c, _)| (c.comaps.map_series.clone(), c.comaps.release_version));

    let (map_series, release_version) = if let Some((series, ver)) = pinned {
        (series, ver)
    } else {
        discover_preferred_map_series(DEFAULT_METASERVER)
            .map_err(|e| MwmDownloadError::Other(format!("comaps maps.json: {e}")))?
    };

    let discovery = discover_mirrors(DEFAULT_METASERVER, &map_series, Some(release_version))
        .map_err(|e| MwmDownloadError::Other(format!("mirror discovery: {e}")))?;
    let countries =
        fetch_and_verify_countries_txt(&discovery, None, None, COMAPS_COUNTRIES_TXT_PUBKEY_HEX)
            .map_err(|e| MwmDownloadError::Other(format!("countries.txt: {e}")))?;

    Ok(synthesize_catalog_from_countries(
        &discovery.map_series,
        discovery.release_version,
        &countries.raw,
        &countries,
    ))
}

#[derive(Debug, Deserialize)]
struct CountriesRoot {
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

fn is_file_leaf(n: &CountriesNode) -> bool {
    n.s.is_some() && n.sha1_base64.is_some()
}

fn collect_group_tree(nodes: &[CountriesNode], parent: Option<&str>) -> Vec<CountryGroup> {
    let mut out = Vec::new();
    for n in nodes {
        if is_file_leaf(n) {
            continue;
        }
        if n.g.is_empty() {
            continue;
        }
        let mut leaves = Vec::new();
        let mut subgroups = Vec::new();
        for child in &n.g {
            if is_file_leaf(child) {
                leaves.push(CountryGroupLeaf {
                    id: child.id.clone(),
                    display_name: Some(child.id.clone()),
                    comaps_display_name: Some(child.id.clone()),
                    display_name_overridden: Some(false),
                    comaps_bytes: child.s,
                    mwm_bytes: child.s,
                    sidecar_bytes: None,
                    total_bytes: child.s,
                });
            } else {
                subgroups.extend(collect_group_tree(std::slice::from_ref(child), Some(&n.id)));
            }
        }
        // Nested-only groups (Europe → Norway → leaves): keep the outer shell only when it
        // has direct leaves; otherwise promote subgroups.
        if leaves.is_empty() && !subgroups.is_empty() {
            out.extend(subgroups);
            continue;
        }
        if leaves.is_empty() && subgroups.is_empty() {
            continue;
        }
        let leaf_count = leaves.len() as u64;
        out.push(CountryGroup {
            id: n.id.clone(),
            display_name: Some(n.id.clone()),
            parent_groups: parent.map(|p| vec![p.to_string()]).unwrap_or_default(),
            leaf_count: Some(leaf_count),
            leaves,
            subgroups,
        });
    }
    out
}

/// Pure builder used by [`fetch_comaps_direct_catalog`] and unit tests.
pub fn synthesize_catalog_from_countries(
    map_series: &str,
    release_version: u64,
    countries_raw: &[u8],
    countries: &super::comaps::CountriesTxt,
) -> MwmCatalog {
    let version = release_version.to_string();
    let countries_sha = sha256_hex_of_bytes(countries_raw);
    let root: CountriesRoot =
        serde_json::from_slice(countries_raw).unwrap_or(CountriesRoot { g: Vec::new() });
    let country_groups = collect_group_tree(&root.g, None);

    let mut region_to_leaves: BTreeMap<String, Vec<String>> = BTreeMap::new();
    fn walk_regions(groups: &[CountryGroup], into: &mut BTreeMap<String, Vec<String>>) {
        for g in groups {
            let mut ids: Vec<String> = g.leaves.iter().map(|l| l.id.clone()).collect();
            fn collect_sub(subs: &[CountryGroup], ids: &mut Vec<String>) {
                for s in subs {
                    for l in &s.leaves {
                        ids.push(l.id.clone());
                    }
                    collect_sub(&s.subgroups, ids);
                }
            }
            collect_sub(&g.subgroups, &mut ids);
            ids.sort();
            ids.dedup();
            if !ids.is_empty() {
                into.insert(g.id.to_ascii_lowercase(), ids);
            }
            walk_regions(&g.subgroups, into);
        }
    }
    walk_regions(&country_groups, &mut region_to_leaves);

    // All CoMaps-direct leaves skip Navi sidecar download. Matching on-disk sidecars from a
    // prior navi install are still loaded by session bootstrap when fingerprints match; VL
    // honesty uses leaves_missing_vehicle_limits + coverage store. Coverage is omitted so we
    // never claim match rates without a Navi sidecar publish.
    let mut leaves: Vec<CatalogLeaf> = countries
        .leaves
        .values()
        .map(|l| CatalogLeaf {
            id: l.id.clone(),
            sidecar_url: None,
            sidecar_bytes: None,
            sidecar_sha256: None,
            comaps_file: Some(format!("{}.mwm", l.id)),
            comaps_bytes: Some(l.size),
            comaps_sha1_base64: Some(l.sha1_base64.clone()),
            mwm_bytes: Some(l.size),
            mwm_sha256: None,
            mwm_fingerprint: None,
            shared: false,
            shared_with_regions: vec![],
            no_sidecar: true,
            elevation_policy_version: None,
            vehicle_limits_coverage: None,
        })
        .collect();
    leaves.sort_by(|a, b| a.id.cmp(&b.id));

    MwmCatalog {
        schema: SCHEMA_V2.to_string(),
        mode: "comaps-direct-fallback".into(),
        version,
        comaps: ComapsMeta {
            map_series: map_series.to_string(),
            release_version,
            countries_txt_sha256: countries_sha,
            countries_txt_bytes: countries_raw.len() as u64,
            countries_txt_sig_verified: Some(true),
            countries_txt_sig_sha256: None,
            countries_txt_pubkey_hex: COMAPS_COUNTRIES_TXT_PUBKEY_HEX.to_string(),
            metaserver_url: DEFAULT_METASERVER.to_string(),
            maps_json_url: Some(format!("{DEFAULT_METASERVER}/meta/maps.json")),
            note: Some(
                "CoMaps-direct fallback: MWM from mirrors; Navi sidecars/VL coverage absent until a navi catalog is available"
                    .into(),
            ),
        },
        generator: None,
        limitation: Some(
            "comaps-direct-fallback: vehicle limits apply only when a matching Navi sidecar is already on device; otherwise honesty banners report unavailable"
                .into(),
        ),
        leaves,
        region_to_leaves,
        shared_leaves: BTreeMap::new(),
        country_groups,
        country_groups_url: None,
        display_name_overrides: DisplayNameOverrides::default(),
        region_migration_url: None,
        world_leaf_count: Some(countries.leaves.len() as u64),
        elevation_policy_version: None,
        non_routable_leaves: vec!["World".into(), "WorldCoasts".into()],
    }
}

/// Snapshot JSON for `releases/{version}/catalog.json` (VL coverage store reads this).
pub fn catalog_snapshot_json(resolved: &ResolvedCatalog) -> Result<String, MwmDownloadError> {
    if let Some(raw) = &resolved.raw_json {
        return Ok(raw.clone());
    }
    serde_json::to_string(&resolved.catalog).map_err(|e| MwmDownloadError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mwm::comaps::parse_countries_txt;
    use crate::mwm::flag::with_mwm_client_override;
    use tempfile::tempdir;

    fn sample_countries() -> (Vec<u8>, crate::mwm::comaps::CountriesTxt) {
        let raw = br#"{
          "id": "Countries",
          "v": 261105,
          "map_series": "2026.10.27",
          "g": [
            {"id": "World", "s": 10, "sha1_base64": "aaa="},
            {"id": "WorldCoasts", "s": 11, "sha1_base64": "bbb="},
            {"id": "Europe", "g": [
              {"id": "Norway", "g": [
                {"id": "Norway_Hedmark", "s": 100, "sha1_base64": "ccc="},
                {"id": "Norway_Oppland", "s": 101, "sha1_base64": "ddd="}
              ]}
            ]}
          ]
        }"#;
        let c = parse_countries_txt(raw).unwrap();
        (raw.to_vec(), c)
    }

    #[test]
    fn synthesize_omits_vl_coverage_and_marks_no_sidecar() {
        let (raw, countries) = sample_countries();
        let cat = synthesize_catalog_from_countries("2026.10.27", 261105, &raw, &countries);
        assert_eq!(cat.mode, "comaps-direct-fallback");
        assert!(cat.leaves.len() >= 4);
        for leaf in &cat.leaves {
            assert!(leaf.no_sidecar, "{}", leaf.id);
            assert!(leaf.vehicle_limits_coverage.is_none());
            assert!(leaf.sidecar_url.is_none());
        }
        let norway = cat.region_to_leaves.get("norway").expect("norway region");
        assert!(norway.contains(&"Norway_Hedmark".to_string()));
        assert!(norway.contains(&"Norway_Oppland".to_string()));
        assert!(cat
            .limitation
            .as_deref()
            .unwrap()
            .contains("vehicle limits"));
    }

    #[test]
    fn empty_navi_falls_through_to_disk_cache() {
        with_mwm_client_override(Some(true), || {
            let dir = tempdir().unwrap();
            let cache = catalog_cache_path(dir.path());
            fs::create_dir_all(cache.parent().unwrap()).unwrap();
            let example = include_str!("../../tests/fixtures/mwm/catalog-v2.example.json");
            fs::write(&cache, example).unwrap();
            let resolved =
                resolve_mwm_catalog("https://example.invalid", Some(dir.path())).unwrap();
            assert_eq!(resolved.source, CatalogResolveSource::DiskCache);
            assert!(catalog_usable(&resolved.catalog));
            // Cached navi catalog keeps sidecar metadata for VL when the host returns.
            let hedmark = resolved.catalog.leaf("Norway_Hedmark").unwrap();
            assert!(!hedmark.no_sidecar);
            assert!(hedmark.sidecar_url.is_some());
            assert!(resolved.source.allow_missing_sidecar());
        });
    }

    #[test]
    fn navi_server_source_does_not_soft_fail_sidecars() {
        assert!(!CatalogResolveSource::NaviServer.allow_missing_sidecar());
        assert!(CatalogResolveSource::DiskCache.allow_missing_sidecar());
        assert!(CatalogResolveSource::ComapsDirect.allow_missing_sidecar());
    }
}
