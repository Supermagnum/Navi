//! OSM POI spatial index (separate from routing graph).

mod categories;
mod classifier;
mod icons;
mod index;

pub use categories::PoiCategory;
pub use classifier::classify_tags;
pub use icons::osm_icon_key;
pub use index::{PoiIndex, PoiQuery, PoiRecord};
