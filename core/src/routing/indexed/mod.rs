//! Preprocess-once indexed map packs (`rkyv` + `memmap2`).
//!
//! Phase 4 production format locked from Phase 1c/2 PoCs. See
//! [`docs/indexed-map-format-plan.md`](../../../../docs/indexed-map-format-plan.md).

mod convert;
mod graph_pack;
mod header;
mod io;
mod load;
mod manifest;
mod poi_barrier_extract;
mod poi_barrier_pack;
mod wetland_pack;

pub use convert::{convert_region_packs, ConvertOptions, ConvertReport};
pub use graph_pack::{FlatGraphPack, GRAPH_FORMAT_VERSION, MAGIC_GRAPH};
pub use header::{read_preamble, Preamble, PREAMBLE_LEN};
pub use io::{archive_payload_offset, write_archive_atomic};
pub use load::{
    fingerprint_pbf_for_packs, load_graph_pack, load_graph_pack_bbox, load_poi_barrier_pack,
    load_wetland_pack, try_load_graph_for_plan, try_load_graph_for_plan_bbox,
    try_load_poi_barrier_for_plan, try_load_wetland_for_plan, PackLoadError, PackedPlanData,
};
pub use manifest::{
    bbox_intersects, graph_pack_filename, graph_tile_filename, manifest_path,
    poi_barrier_pack_filename, wetland_pack_filename, wetland_tile_filename, GraphTileEntry,
    NaviManifest, PackStatus, GRAPH_PROFILE_BICYCLE, GRAPH_PROFILE_CAR, GRAPH_PROFILE_FOOT,
    GRAPH_PROFILE_TRUCK,
};
pub use poi_barrier_pack::{FlatPoiBarrierPack, MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION};
pub use wetland_pack::{FlatWetlandPack, MAGIC_WETLAND, WETLAND_FORMAT_VERSION};
