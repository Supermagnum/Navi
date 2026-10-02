//! Preprocess-once indexed map packs (`rkyv` + `memmap2`).
//!
//! Phase 4 production format locked from Phase 1c/2 PoCs. See
//! [`docs/indexed-map-format-plan.md`](../../../../docs/indexed-map-format-plan.md).

mod convert;
mod corridor_cache;
mod graph_pack;
mod graph_pack_v8;
mod header;
mod io;
mod load;
mod manifest;
mod poi_barrier_cache;
mod poi_barrier_extract;
mod poi_barrier_pack;
mod wetland_pack;

pub use crate::routing::region_lock::{
    acquire_plan_fallback, cleanup_spills_for_pid, convert_lock_held,
    holding_convert_lock_on_thread, is_convert_in_progress_err, region_id_for_pbf,
    try_acquire_convert, ConvertAcquire, RegionLockGuard, RegionLockKind, RegionLockPhase,
    REGION_CONVERT_IN_PROGRESS,
};
pub use convert::{convert_region_packs, ConvertOptions, ConvertReport};
pub use corridor_cache::{
    corridor_cache_clear, corridor_cache_get, corridor_cache_insert_owned, corridor_cache_stats,
    corridor_cache_take, CorridorCacheKey, CORRIDOR_CACHE_MAX_BYTES,
};
pub use poi_barrier_cache::{
    poi_barrier_cache_clear, poi_barrier_cache_stats, PoiBarrierCacheKey,
};
pub use graph_pack::{
    graph_format_version_accepted, preferred_graph_format_version, ArchivedFlatGraphPack,
    FlatGraphPack, GRAPH_FORMAT_VERSION, GRAPH_FORMAT_VERSION_V8, MAGIC_GRAPH,
};
pub use graph_pack_v8::{ArchivedFlatGraphPackV8, FlatGraphPackV8};
pub use header::{read_preamble, Preamble, PREAMBLE_LEN};
pub use io::{archive_payload_offset, write_archive_atomic};
pub use load::{
    fingerprint_pbf_for_packs, load_graph_pack, load_graph_pack_bbox, load_graph_pack_clips,
    load_poi_barrier_pack, load_poi_barrier_pack_bbox, load_wetland_pack, merge_tile_graphs,
    try_load_graph_for_plan, try_load_graph_for_plan_bbox, try_load_graph_for_plan_corridor,
    try_load_graph_for_plan_corridor_with_pack_dirs, try_load_poi_barrier_for_plan,
    try_load_poi_barrier_for_plan_bbox, try_load_poi_barrier_for_plan_bbox_with_pack_dirs,
    try_load_poi_pack_covering_point, try_load_poi_pack_covering_point_with_pack_dirs,
    try_load_wetland_for_plan, PackLoadError, PackedPlanData,
};
pub use manifest::{
    bbox_intersects, graph_pack_filename, graph_tile_filename, manifest_path,
    poi_barrier_pack_filename, server_install_path, server_install_present, wetland_pack_filename,
    wetland_tile_filename, GraphTileEntry, NaviManifest, PackStatus, GRAPH_PROFILE_BICYCLE,
    GRAPH_PROFILE_CAR, GRAPH_PROFILE_FOOT, GRAPH_PROFILE_TRUCK, SERVER_INSTALL_SUFFIX,
};
pub use poi_barrier_pack::{FlatPoiBarrierPack, MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION};
pub use wetland_pack::{FlatWetlandPack, MAGIC_WETLAND, WETLAND_FORMAT_VERSION};
