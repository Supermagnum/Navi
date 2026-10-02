//! POI + danger-barrier archive body, promoted from Phase 2 PoC.

use std::collections::HashMap;

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

use crate::poi::{PoiCategory, PoiIndex, PoiRecord};
use crate::routing::safety::DangerBarrierIndex;

/// Little-endian ASCII "NVPB".
pub const MAGIC_POI_BARRIER: u32 = 0x4E_56_50_42;
/// v2 adds overnight building centroids for hiking allemannsretten checks.
pub const POI_BARRIER_FORMAT_VERSION: u32 = 2;

const CAT_WATER: u16 = 1 << 0;
const CAT_CABIN: u16 = 1 << 1;
const CAT_GENERAL: u16 = 1 << 2;
const CAT_NETWORK_HUT: u16 = 1 << 3;
const CAT_RESTROOM: u16 = 1 << 4;
const CAT_OVERNIGHT: u16 = 1 << 5;
const CAT_CRAFT: u16 = 1 << 6;
const CAT_TENT: u16 = 1 << 7;
const CAT_FISHING: u16 = 1 << 8;
const CAT_REST_AREA: u16 = 1 << 9;
const CAT_LODGING: u16 = 1 << 10;

#[derive(Archive, RkyvSerialize, RkyvDeserialize, Debug, Clone)]
pub struct FlatPoiBarrierPack {
    pub osm_ids: Vec<i64>,
    pub lats: Vec<f64>,
    pub lons: Vec<f64>,
    pub cat_masks: Vec<u16>,
    pub icon_keys: Vec<String>,
    pub names: Vec<String>,
    pub tag_offsets: Vec<u32>,
    pub tag_keys: Vec<String>,
    pub tag_vals: Vec<String>,
    pub seg_a_lon: Vec<f64>,
    pub seg_a_lat: Vec<f64>,
    pub seg_b_lon: Vec<f64>,
    pub seg_b_lat: Vec<f64>,
    pub glacier_offsets: Vec<u32>,
    pub glacier_lon: Vec<f64>,
    pub glacier_lat: Vec<f64>,
    /// Overnight building centroids for hiking safety (format v2+). Parallel
    /// `building_lats[i]` / `building_lons[i]`.
    pub building_lats: Vec<f64>,
    pub building_lons: Vec<f64>,
}

fn cat_mask(cats: &[PoiCategory]) -> u16 {
    let mut m = 0u16;
    for c in cats {
        m |= match c {
            PoiCategory::Water => CAT_WATER,
            PoiCategory::Cabin => CAT_CABIN,
            PoiCategory::General => CAT_GENERAL,
            PoiCategory::NetworkHut => CAT_NETWORK_HUT,
            PoiCategory::Restroom => CAT_RESTROOM,
            PoiCategory::OvernightFacility => CAT_OVERNIGHT,
            PoiCategory::CraftBrewery => CAT_CRAFT,
            PoiCategory::TentSite => CAT_TENT,
            PoiCategory::Fishing => CAT_FISHING,
            PoiCategory::RestArea => CAT_REST_AREA,
            PoiCategory::Lodging => CAT_LODGING,
        };
    }
    m
}

fn cats_from_mask(m: u16) -> Vec<PoiCategory> {
    let mut out = Vec::new();
    for (bit, cat) in [
        (CAT_WATER, PoiCategory::Water),
        (CAT_CABIN, PoiCategory::Cabin),
        (CAT_GENERAL, PoiCategory::General),
        (CAT_NETWORK_HUT, PoiCategory::NetworkHut),
        (CAT_RESTROOM, PoiCategory::Restroom),
        (CAT_OVERNIGHT, PoiCategory::OvernightFacility),
        (CAT_CRAFT, PoiCategory::CraftBrewery),
        (CAT_TENT, PoiCategory::TentSite),
        (CAT_FISHING, PoiCategory::Fishing),
        (CAT_REST_AREA, PoiCategory::RestArea),
        (CAT_LODGING, PoiCategory::Lodging),
    ] {
        if m & bit != 0 {
            out.push(cat);
        }
    }
    out
}

impl FlatPoiBarrierPack {
    pub fn empty() -> Self {
        Self {
            osm_ids: Vec::new(),
            lats: Vec::new(),
            lons: Vec::new(),
            cat_masks: Vec::new(),
            icon_keys: Vec::new(),
            names: Vec::new(),
            tag_offsets: vec![0],
            tag_keys: Vec::new(),
            tag_vals: Vec::new(),
            seg_a_lon: Vec::new(),
            seg_a_lat: Vec::new(),
            seg_b_lon: Vec::new(),
            seg_b_lat: Vec::new(),
            glacier_offsets: vec![0],
            glacier_lon: Vec::new(),
            glacier_lat: Vec::new(),
            building_lats: Vec::new(),
            building_lons: Vec::new(),
        }
    }

    pub fn from_parts(
        records: &[PoiRecord],
        segments: &[(f64, f64, f64, f64)],
        glaciers: &[Vec<[f64; 2]>],
        overnight_buildings: &[(f64, f64)],
    ) -> Self {
        let mut pack = Self::empty();
        pack.tag_offsets.clear();
        pack.tag_offsets.push(0);
        pack.glacier_offsets.clear();
        pack.glacier_offsets.push(0);

        for r in records {
            pack.osm_ids.push(r.osm_id);
            pack.lats.push(r.lat);
            pack.lons.push(r.lon);
            pack.cat_masks.push(cat_mask(&r.categories));
            pack.icon_keys.push(r.icon_key.clone());
            pack.names.push(r.name.clone().unwrap_or_default());
            for (k, v) in &r.tags {
                pack.tag_keys.push(k.clone());
                pack.tag_vals.push(v.clone());
            }
            pack.tag_offsets.push(pack.tag_keys.len() as u32);
        }
        for &(a_lon, a_lat, b_lon, b_lat) in segments {
            pack.seg_a_lon.push(a_lon);
            pack.seg_a_lat.push(a_lat);
            pack.seg_b_lon.push(b_lon);
            pack.seg_b_lat.push(b_lat);
        }
        for ring in glaciers {
            for p in ring {
                pack.glacier_lon.push(p[0]);
                pack.glacier_lat.push(p[1]);
            }
            pack.glacier_offsets.push(pack.glacier_lon.len() as u32);
        }
        for &(lat, lon) in overnight_buildings {
            pack.building_lats.push(lat);
            pack.building_lons.push(lon);
        }
        pack
    }

    pub fn to_poi_index(&self) -> PoiIndex {
        self.to_poi_index_bbox(None, true)
    }

    /// Hydrate POI records, optionally keeping only points inside `bbox`
    /// `[min_lat, min_lon, max_lat, max_lon]`.
    ///
    /// `include_overnight_buildings`: hiking allemannsretten samples. Motor
    /// plans should pass `false` — Ostlandet packs store ~2M building centroids
    /// that dominate hydrate time on emulator SD (~15 s) and are unused for
    /// soft-break POI search.
    pub fn to_poi_index_bbox(
        &self,
        bbox: Option<[f64; 4]>,
        include_overnight_buildings: bool,
    ) -> PoiIndex {
        let mut index = PoiIndex::new();
        for i in 0..self.osm_ids.len() {
            let lat = self.lats[i];
            let lon = self.lons[i];
            if let Some(b) = bbox {
                if lat < b[0] || lat > b[2] || lon < b[1] || lon > b[3] {
                    continue;
                }
            }
            let a = self.tag_offsets[i] as usize;
            let b = self.tag_offsets[i + 1] as usize;
            let mut tags = HashMap::new();
            for t in a..b {
                tags.insert(self.tag_keys[t].clone(), self.tag_vals[t].clone());
            }
            let name = if self.names[i].is_empty() {
                None
            } else {
                Some(self.names[i].clone())
            };
            index.insert_record(PoiRecord {
                osm_id: self.osm_ids[i],
                lat,
                lon,
                categories: cats_from_mask(self.cat_masks[i]),
                icon_key: self.icon_keys[i].clone(),
                tags,
                name,
            });
        }
        if include_overnight_buildings {
            let n = self.building_lats.len().min(self.building_lons.len());
            let mut buildings = Vec::with_capacity(n);
            for i in 0..n {
                let lat = self.building_lats[i];
                let lon = self.building_lons[i];
                if let Some(b) = bbox {
                    if lat < b[0] || lat > b[2] || lon < b[1] || lon > b[3] {
                        continue;
                    }
                }
                buildings.push((lat, lon));
            }
            index.set_overnight_buildings(buildings);
        }
        index
    }

    pub fn to_barrier_index(&self) -> DangerBarrierIndex {
        self.to_barrier_index_bbox(None)
    }

    /// Hydrate barrier segments / glaciers, optionally keeping geometry that
    /// touches `bbox` `[min_lat, min_lon, max_lat, max_lon]`.
    pub fn to_barrier_index_bbox(&self, bbox: Option<[f64; 4]>) -> DangerBarrierIndex {
        let segs = (0..self.seg_a_lon.len()).filter_map(|i| {
            let a_lon = self.seg_a_lon[i];
            let a_lat = self.seg_a_lat[i];
            let b_lon = self.seg_b_lon[i];
            let b_lat = self.seg_b_lat[i];
            if let Some(b) = bbox {
                let min_lat = a_lat.min(b_lat);
                let max_lat = a_lat.max(b_lat);
                let min_lon = a_lon.min(b_lon);
                let max_lon = a_lon.max(b_lon);
                if max_lat < b[0] || min_lat > b[2] || max_lon < b[1] || min_lon > b[3] {
                    return None;
                }
            }
            Some((a_lon, a_lat, b_lon, b_lat))
        });
        let mut glaciers = Vec::new();
        for g in 0..self.glacier_offsets.len().saturating_sub(1) {
            let a = self.glacier_offsets[g] as usize;
            let b = self.glacier_offsets[g + 1] as usize;
            let mut ring = Vec::with_capacity(b - a);
            let mut any_in = bbox.is_none();
            for i in a..b {
                let lon = self.glacier_lon[i];
                let lat = self.glacier_lat[i];
                if let Some(bb) = bbox {
                    if lat >= bb[0] && lat <= bb[2] && lon >= bb[1] && lon <= bb[3] {
                        any_in = true;
                    }
                }
                ring.push([lon, lat]);
            }
            if any_in {
                glaciers.push(ring);
            }
        }
        DangerBarrierIndex::from_segments(segs, glaciers)
    }
}
