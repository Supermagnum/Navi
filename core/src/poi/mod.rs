//! OSM POI spatial index (separate from routing graph).

mod categories;
mod classifier;
mod corridor_band;
mod icons;
mod index;
mod lookahead;

pub use categories::PoiCategory;
pub use classifier::{classify_tags, rest_area_suitable_for_weekly};
pub use corridor_band::CorridorBand;
pub use icons::osm_icon_key;
pub use index::{PoiIndex, PoiOvernightLoadProfile, PoiQuery, PoiRecord};
pub use lookahead::{
    categories_qualify_for_lookahead, category_wire_name, format_lookahead_label,
    guest_cone_filter, in_poi_lookahead_cone, open_now_at, poi_index_from_tagged_json,
    primary_lookahead_category, query_poi_lookahead, OpenNow, PoiLookaheadHit,
    POI_LOOKAHEAD_CONE_HALF_WIDTH_DEG, POI_LOOKAHEAD_CONE_M, POI_LOOKAHEAD_DEFAULT_ENABLED,
    POI_LOOKAHEAD_STRICT_HOURS_UNKNOWN_DEFAULT,
};
