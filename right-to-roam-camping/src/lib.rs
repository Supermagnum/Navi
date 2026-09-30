//! Right-to-roam overnight camping — shared rule engine.
//!
//! Plain Rust (no wasmtime / WASI). Unit tests and a future thin WASM guest both
//! call into this crate. Host capabilities live in `navi-plugin-host`; this crate
//! consumes HostApi *views* (JSON / structs) and never opens network or
//! filesystem itself.
//!
//! Phase 1: payload types for “layer not checked” guidance. Packs and seed
//! ranking land in later phases.

use serde::{Deserialize, Serialize};

/// Single visible field for layers the host cannot check yet.
///
/// When a layer becomes `Ready`, clear the matching flag so the card text
/// disappears automatically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NotCheckedLayers {
    /// Protected-area / national-park status unknown.
    pub protected_area: bool,
    /// Forest / farmland / beach / alpine landcover unknown.
    pub landcover: bool,
}

impl NotCheckedLayers {
    /// Both layers unknown — typical Phase 1 host.
    pub fn both_unknown() -> Self {
        Self {
            protected_area: true,
            landcover: true,
        }
    }

    /// Card lines for Tier A packs (NO, SE, FI, IS, EE) when layers are missing.
    pub fn card_notes(&self, farmland_filter_applies: bool) -> Vec<&'static str> {
        let mut notes = Vec::new();
        if self.protected_area {
            notes.push(
                "protected-area status not checked — national parks and nature reserves may have their own rules",
            );
        }
        if self.landcover && farmland_filter_applies {
            notes.push(
                "land cover not checked — do not camp on farmland, pasture or cultivated land",
            );
        }
        notes
    }

    /// Unknown never counts as a pass for Tier B conditions.
    pub fn blocks_tier_b_protected_or_landcover(&self) -> bool {
        self.protected_area || self.landcover
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use driver_break_core::{territory_override_at, TerritoryOverride};

    #[test]
    fn not_checked_notes_for_sweden_include_farmland() {
        let n = NotCheckedLayers::both_unknown();
        let notes = n.card_notes(true);
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("protected-area"));
        assert!(notes[1].contains("farmland"));
    }

    #[test]
    fn not_checked_notes_for_norway_omit_farmland_line() {
        let n = NotCheckedLayers::both_unknown();
        let notes = n.card_notes(false);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("protected-area"));
    }

    #[test]
    fn sj_override_visible_to_camping_crate() {
        assert_eq!(
            territory_override_at(78.2232, 15.6267),
            Some(TerritoryOverride::Confident("sj"))
        );
    }
}
