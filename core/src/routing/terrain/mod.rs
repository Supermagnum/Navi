//! Off-trail least-cost path over a DEM + wetland cost surface.
//!
//! Used only as a gap-fill when the OSM foot graph cannot connect two points.
//! Prefer graph (on-trail) routing first; invoke this for genuine gaps only.

mod path;

pub use path::{least_cost_path, TerrainPath, TERRAIN_CELL_M, TERRAIN_MAX_GAP_M};
