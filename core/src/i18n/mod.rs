//! Locale helpers shared by packs, place index, and guidance presentation.
//!
//! Name selection rules are documented in `docs/i18n/names.md`.

mod localized_name;
pub mod messages;

pub use localized_name::{
    is_mostly_latin_script, localized_name, localized_name_with_secondary, NameTags, SelectedName,
};
pub use messages::{encode as encode_ui_msg, encode1 as encode_ui_msg1, MSG_PREFIX};
