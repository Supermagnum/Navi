//! CoMaps MWM + Navi sidecar catalog / download layer.
//!
//! Gated by [`flag::mwm_client_enabled`] (`NAVI_MWM_CLIENT_ENABLED`, default
//! **on**). Set the env to `0`/`false`/`off` to force the classic
//! [`crate::pack_server`] (`.rkyv`) path for debugging. This module does not
//! remove or change the `.rkyv` implementation.
//!
//! Contract: navi-server `docs/client-fetch-mwm.md` (DocumentRoot-relative
//! `/mwm/…` URLs on the live host).

pub mod attribution;
pub mod catalog;
pub mod checksum;
pub mod comaps;
pub mod comaps_instructions;
pub mod comaps_router;
pub mod download;
pub mod fingerprint;
pub mod flag;
pub mod migration;
pub mod region_ui;
pub mod release;
pub mod sidecar;
#[cfg(test)]
mod sidecar_tests;
pub mod storage;

pub use attribution::{store_attribution_files, AttributionStore};
pub use catalog::{
    parse_catalog_json, CatalogLeaf, ComapsMeta, DisplayNameOverride, DisplayNameOverrides,
    MwmCatalog, MwmFingerprint, COUNTRY_GROUPS_ROUTABLE_TARGET, SCHEMA_V2,
};
pub use checksum::{
    sha1_base64_of_bytes, sha1_base64_of_path, sha256_hex_of_bytes, sha256_hex_of_path,
    verify_bytes_sha256, verify_path_sha1_base64, verify_path_sha256, ChecksumError,
};
pub use comaps::{
    discover_mirrors, fetch_and_verify_countries_txt, parse_countries_txt, percent_encode_leaf_id,
    verify_countries_txt_signature, ComapsDiscovery, CountriesTxt, CountriesTxtLeaf,
    COMAPS_COUNTRIES_TXT_PUBKEY_HEX, DEFAULT_METASERVER,
};
pub use comaps_instructions::{
    build_voice_prompts, enrich_turns_json, instructions_from_turns_json, ComapsTurnStep,
    NaviInstruction,
};
pub use comaps_router::{
    ComapsLatLon, ComapsProfile, ComapsRoute, ComapsRouter, ComapsRouterCode, ComapsRouterError,
};
pub use download::{
    download_leaf_set, download_world_overlays, fetch_current_catalog,
    leaf_ids_with_world_overlays, DownloadPlan, DownloadProgress, LeafDownloadResult,
    MwmDownloadError, DEFAULT_MWM_CONCURRENCY, NAVI_MWM_USER_AGENT, WORLD_OVERLAY_LEAF_IDS,
};
pub use fingerprint::{
    check_local_mwm_fingerprint, fingerprint_from_catalog_leaf, FingerprintMismatch,
    FingerprintStatus, FINGERPRINT_USER_WARNING,
};
pub use flag::{
    mwm_client_enabled, set_mwm_client_enabled_override, with_mwm_client_override,
    MWM_CLIENT_ENABLED_ENV,
};
pub use migration::{
    leaf_set_for_bake_id, leaf_set_for_region_id, parse_region_migration_json, RegionMigration,
    RegionMigrationEntry, SCHEMA_REGION_MIGRATION_V1,
};
pub use region_ui::{
    flatten_country_group_leaves, region_ui_from_catalog, RegionUiGroup, RegionUiLeaf,
};
pub use release::{
    abandon_staging, activate_release, assert_world_matches_active, begin_staging,
    cleanup_old_releases, mark_staging_complete, read_active_version, read_world_release,
    staging_is_complete, write_active_version, ActiveRelease, ReleaseSwitchError, WorldRelease,
};
pub use sidecar::{
    encode_elevation_el02, encode_test_sidecar_zstd, parse_elevation_el02, zigzag_decode_i32,
    zigzag_encode_i32, BoardwalkRecord, ElevationFeature, ElevationPoint, InspectCounts,
    SidecarError, SidecarHeader, SidecarReader, WetlandRecord, CLASS_HARD, CLASS_SOFT,
    ELEVATION_BODY_MAGIC_EL02, FORMAT_VERSION, MAGIC, REQUIRED_SECTIONS, SECTION_ATTRIBUTION,
    SECTION_BOARDWALK, SECTION_ELEVATION, SECTION_WETLAND,
};
pub use storage::{
    active_path, attribution_json_path, attribution_md_path, catalog_snapshot_path, leaf_mwm_path,
    leaf_sidecar_path, release_dir, staging_dir, world_coasts_mwm_path, world_mwm_path,
    world_release_path, MwmPaths, ACTIVE_FILE, MWM_ROOT, WORLD_COASTS_MWM_FILE, WORLD_MWM_FILE,
    WORLD_RELEASE_FILE,
};
