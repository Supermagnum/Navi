//! Locale helpers shared by packs, place index, and guidance presentation.
//!
//! Name selection rules are documented in `docs/i18n/names.md`.

mod localized_name;

pub use localized_name::{
    is_mostly_latin_script, localized_name, localized_name_with_secondary, NameTags, SelectedName,
};
