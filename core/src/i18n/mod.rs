//! Locale helpers shared by packs, place index, and guidance presentation.
//!
//! Name selection rules are documented in `docs/i18n/names.md`.

mod localized_name;
pub mod messages;
pub mod tile_road_names;

pub use localized_name::{
    is_mostly_latin_script, localized_name, localized_name_with_secondary, NameTags, SelectedName,
};
pub use messages::{encode as encode_ui_msg, encode1 as encode_ui_msg1, MSG_PREFIX};
pub use tile_road_names::{
    apply_cached_street, build_route_road_name_cache, cached_localized_street,
    clear_active_route_road_name_cache, localize_maneuvers_json, localize_sim_samples_json,
    set_active_route_road_name_cache, StreetLabelQuery,
};
