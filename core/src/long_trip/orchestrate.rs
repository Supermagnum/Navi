//! Long-trip region acquisition state machine (download then index).

use std::collections::BTreeMap;

use super::estimate::{estimate_trip_disk_bytes, CatalogSizeLookup, SpaceCheck};
use super::volume::StorageVolume;

/// Corridor densify / catalog sample step (placeholder buffer from the spec).
pub const LONG_TRIP_CORRIDOR_BUFFER_KM: f64 = 25.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionTripState {
    Needed,
    Downloading,
    /// Pack installed — routable.
    Installed,
    Indexing,
    /// Place index built — searchable.
    Indexed,
    Failed(String),
    Paused,
    /// Removable volume gone mid-download.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongTripPlan {
    pub regions_in_order: Vec<String>,
    pub states: BTreeMap<String, RegionTripState>,
    pub start_region: String,
    pub allowed_countries: Option<Vec<String>>,
}

impl LongTripPlan {
    pub fn new(start_region: String, regions_in_order: Vec<String>) -> Self {
        let mut states = BTreeMap::new();
        for r in &regions_in_order {
            states.insert(r.clone(), RegionTripState::Needed);
        }
        if !states.contains_key(&start_region) {
            states.insert(start_region.clone(), RegionTripState::Needed);
        }
        Self {
            regions_in_order,
            states,
            start_region,
            allowed_countries: None,
        }
    }

    pub fn set_state(&mut self, region: &str, state: RegionTripState) {
        self.states.insert(region.to_string(), state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LongTripError {
    InsufficientSpace {
        needed: u64,
        free: u64,
        shortfall: u64,
    },
    OfflineForDownloads,
    Cancelled,
    Incomplete {
        missing: Vec<String>,
    },
    Catalog(String),
}

pub trait RegionDownloader {
    fn download(&mut self, region_id: &str) -> Result<(), String>;
}

pub trait RegionIndexer {
    fn index(&mut self, region_id: &str) -> Result<(), String>;
}

pub trait VolumeSource {
    fn volumes(&self) -> Vec<StorageVolume>;
}

/// Drive downloads (route order) then indexing (route order, after last download),
/// without blocking the caller’s ability to plan on already-Installed regions.
pub struct TripOrchestrator<D, I, V> {
    pub downloader: D,
    pub indexer: I,
    pub volumes: V,
    pub unmetered: bool,
    pub enabled: bool,
}

impl<D: RegionDownloader, I: RegionIndexer, V: VolumeSource> TripOrchestrator<D, I, V> {
    /// Free-space gate before any work. Uses the first mounted volume with the
    /// most free space (read-only list; no picker).
    pub fn check_space(
        &self,
        regions: &[String],
        sizes: &dyn CatalogSizeLookup,
    ) -> Result<SpaceCheck, LongTripError> {
        let vols: Vec<_> = self
            .volumes
            .volumes()
            .into_iter()
            .filter(|v| v.mounted)
            .collect();
        let free = vols.iter().map(|v| v.free_bytes).max().unwrap_or(0);
        Ok(estimate_trip_disk_bytes(regions, sizes, free))
    }

    /// Ensure start region is Installed+Indexed first (spec step 1).
    pub fn ensure_start(&mut self, plan: &mut LongTripPlan) -> Result<(), LongTripError> {
        if !self.enabled {
            return Ok(());
        }
        let start = plan.start_region.clone();
        self.download_one(plan, &start)?;
        self.index_one(plan, &start)?;
        Ok(())
    }

    /// Download all Needed regions in route order, then index them in order.
    /// Indexing starts only after the last download completes (except start,
    /// handled by [`Self::ensure_start`]).
    pub fn run_downloads_then_index(
        &mut self,
        plan: &mut LongTripPlan,
        sizes: &dyn CatalogSizeLookup,
    ) -> Result<(), LongTripError> {
        if !self.enabled {
            return Ok(());
        }
        if !self.unmetered {
            return Err(LongTripError::OfflineForDownloads);
        }
        let regions: Vec<String> = plan
            .regions_in_order
            .iter()
            .filter(|r| *r != &plan.start_region)
            .cloned()
            .collect();
        match self.check_space(&plan.regions_in_order, sizes)? {
            SpaceCheck::InsufficientSpace {
                needed,
                free,
                shortfall,
                ..
            } => {
                return Err(LongTripError::InsufficientSpace {
                    needed,
                    free,
                    shortfall,
                });
            }
            SpaceCheck::Ok(_) => {}
        }

        for id in &regions {
            if matches!(
                plan.states.get(id),
                Some(RegionTripState::Installed | RegionTripState::Indexed)
            ) {
                continue;
            }
            self.download_one(plan, id)?;
        }
        // Indexing starts only after every download in this batch completed.
        for id in &regions {
            if matches!(plan.states.get(id), Some(RegionTripState::Indexed)) {
                continue;
            }
            if !matches!(plan.states.get(id), Some(RegionTripState::Installed)) {
                continue;
            }
            self.index_one(plan, id)?;
        }
        Ok(())
    }

    pub fn cancel_pending(&mut self, plan: &mut LongTripPlan) {
        self.enabled = false;
        for st in plan.states.values_mut() {
            if matches!(
                st,
                RegionTripState::Needed
                    | RegionTripState::Downloading
                    | RegionTripState::Indexing
                    | RegionTripState::Paused
            ) {
                *st = RegionTripState::Paused;
            }
        }
        // Never delete Installed / Indexed data.
    }

    /// After process restart (or toggle back on): keep Installed/Indexed,
    /// re-queue Paused / Unavailable / Failed as Needed, and re-enable.
    pub fn resume_after_restart(&mut self, plan: &mut LongTripPlan) {
        self.enabled = true;
        for st in plan.states.values_mut() {
            if matches!(
                st,
                RegionTripState::Paused
                    | RegionTripState::Unavailable
                    | RegionTripState::Failed(_)
                    | RegionTripState::Downloading
                    | RegionTripState::Indexing
            ) {
                *st = RegionTripState::Needed;
            }
        }
    }

    pub fn on_card_removed_mid_download(&mut self, plan: &mut LongTripPlan) {
        for st in plan.states.values_mut() {
            if matches!(st, RegionTripState::Downloading) {
                *st = RegionTripState::Unavailable;
            }
        }
    }

    fn download_one(&mut self, plan: &mut LongTripPlan, id: &str) -> Result<(), LongTripError> {
        if !self.unmetered {
            plan.set_state(id, RegionTripState::Paused);
            return Err(LongTripError::OfflineForDownloads);
        }
        plan.set_state(id, RegionTripState::Downloading);
        match self.downloader.download(id) {
            Ok(()) => {
                plan.set_state(id, RegionTripState::Installed);
                Ok(())
            }
            Err(e) => {
                plan.set_state(id, RegionTripState::Failed(e));
                Err(LongTripError::Incomplete {
                    missing: vec![id.to_string()],
                })
            }
        }
    }

    fn index_one(&mut self, plan: &mut LongTripPlan, id: &str) -> Result<(), LongTripError> {
        plan.set_state(id, RegionTripState::Indexing);
        match self.indexer.index(id) {
            Ok(()) => {
                plan.set_state(id, RegionTripState::Indexed);
                Ok(())
            }
            Err(e) => {
                plan.set_state(id, RegionTripState::Failed(e));
                Err(LongTripError::Incomplete {
                    missing: vec![id.to_string()],
                })
            }
        }
    }
}
