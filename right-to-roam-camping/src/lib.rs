//! Right-to-roam overnight camping — shared rule engine (no wasmtime).

mod candidates;
mod card;
mod engine;
mod fire;
mod host;
mod night_store;
pub mod packs;

pub use candidates::{
    find_road_track_junctions, probe_along_track, JunctionRank, RoadTrackSeed, ProbePoint,
    CORRIDOR_SEED_RADIUS_M, DEFAULT_TRACK_WALK_M, SERVICE_TRACK_MIN_CONTINUE_M,
};
pub use card::{CampingCard, DeclineKind, SuggestionList};
pub use engine::{suggest_overnight, ProbeLogEntry, SuggestInput, SuggestOutcome};
pub use fire::{
    fire_guidance_norway, FireGuidance, BARE_ROCK_NOTE, CAUTIOUS_FIRE_UNKNOWN_DATE,
    LEAVE_NO_TRACE_NOTE, PROTECTED_SPECIES_NOTE,
};
pub use host::{CampingHost, LocalDate, TravelMode};
pub use night_store::{location_id_from_lat_lon, NightStore, LOCATION_GRID_DEG};
pub use packs::{PackId, RulePack, Tier};

/// Layers the host cannot check yet — shown on every Tier A card.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NotCheckedLayers {
    pub protected_area: bool,
    pub landcover: bool,
}

impl NotCheckedLayers {
    pub fn both_unknown() -> Self {
        Self {
            protected_area: true,
            landcover: true,
        }
    }

    pub fn from_host_status(protected_ready: bool, landcover_ready: bool) -> Self {
        Self {
            protected_area: !protected_ready,
            landcover: !landcover_ready,
        }
    }

    pub fn card_notes(&self, farmland_filter_applies: bool) -> Vec<&'static str> {
        let mut notes = Vec::new();
        if self.protected_area {
            notes.push(
                "protected-area status not checked — national parks and nature reserves may have their own rules",
            );
        }
        if self.landcover {
            if farmland_filter_applies {
                notes.push(
                    "land cover not checked — do not camp on farmland, pasture or cultivated land",
                );
            } else {
                notes.push("land cover not checked");
            }
        }
        notes
    }
}

/// Spec disclaimer — every suggestion list and about screen.
pub const DISCLAIMER: &str = "This plugin provides informational guidance based on publicly described \
right-to-roam / outdoor-access rules (including Norwegian allemannsretten). \
It is not legal advice and not a compliance guarantee. Laws and local practice change; \
municipal fire bans, private land, and seasonal restrictions can be stricter than these summaries. \
The user remains responsible for checking official sources and complying with the law where they camp.";
