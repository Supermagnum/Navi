//! OSM POI spatial index (separate from routing graph).

mod categories;
mod classifier;
mod corridor_band;
mod icons;
mod index;

pub use categories::PoiCategory;
pub use classifier::{classify_tags, rest_area_suitable_for_weekly};
pub use corridor_band::CorridorBand;
pub use icons::osm_icon_key;
pub use index::{PoiIndex, PoiOvernightLoadProfile, PoiQuery, PoiRecord};
