//! Wetland polygon archive body (Phase 4b).
//!
//! Separate from POI/barrier so OSM wetland classification can regenerate
//! independently. Boardwalk carve-out stays on graph edges (`is_boardwalk_crossing`);
//! this pack only stores SoftAvoid/HardAvoid rings.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

use crate::routing::wetland::{WetlandClass, WetlandIndex};

/// Little-endian ASCII "NVWL".
pub const MAGIC_WETLAND: u32 = 0x4E_56_57_4C;
pub const WETLAND_FORMAT_VERSION: u32 = 1;

const CLASS_SOFT: u8 = 1;
const CLASS_HARD: u8 = 2;

#[derive(Archive, RkyvSerialize, RkyvDeserialize, Debug, Clone)]
pub struct FlatWetlandPack {
    /// Prefix sums into `ring_lon`/`ring_lat` (length = rings + 1).
    pub ring_offsets: Vec<u32>,
    pub ring_lon: Vec<f64>,
    pub ring_lat: Vec<f64>,
    /// Per-ring class: 1 = SoftAvoid, 2 = HardAvoid.
    pub ring_class: Vec<u8>,
}

impl FlatWetlandPack {
    pub fn empty() -> Self {
        Self {
            ring_offsets: vec![0],
            ring_lon: Vec::new(),
            ring_lat: Vec::new(),
            ring_class: Vec::new(),
        }
    }

    pub fn from_wetland_index(index: &WetlandIndex) -> Self {
        let mut pack = Self::empty();
        pack.ring_offsets.clear();
        pack.ring_offsets.push(0);
        for (class, ring) in index.rings_as_parts() {
            let class_u8 = match class {
                WetlandClass::SoftAvoid => CLASS_SOFT,
                WetlandClass::HardAvoid => CLASS_HARD,
            };
            for pt in ring {
                pack.ring_lon.push(pt[0]);
                pack.ring_lat.push(pt[1]);
            }
            pack.ring_offsets.push(pack.ring_lon.len() as u32);
            pack.ring_class.push(class_u8);
        }
        pack
    }

    pub fn ring_count(&self) -> usize {
        self.ring_class.len()
    }

    /// Materialize a [`WetlandIndex`], optionally keeping only rings that touch `bbox`.
    pub fn to_wetland_index(&self, bbox: Option<[f64; 4]>) -> WetlandIndex {
        let mut parts = Vec::with_capacity(self.ring_class.len());
        for i in 0..self.ring_class.len() {
            let start = self.ring_offsets[i] as usize;
            let end = self.ring_offsets[i + 1] as usize;
            if end < start + 3 || end > self.ring_lon.len() || end > self.ring_lat.len() {
                continue;
            }
            let class = match self.ring_class[i] {
                CLASS_HARD => WetlandClass::HardAvoid,
                _ => WetlandClass::SoftAvoid,
            };
            let mut ring = Vec::with_capacity(end - start);
            let mut any_in = bbox.is_none();
            for j in start..end {
                let lon = self.ring_lon[j];
                let lat = self.ring_lat[j];
                if let Some(b) = bbox {
                    if lat >= b[0] && lat <= b[2] && lon >= b[1] && lon <= b[3] {
                        any_in = true;
                    }
                }
                ring.push([lon, lat]);
            }
            if !any_in {
                continue;
            }
            parts.push((class, ring));
        }
        WetlandIndex::from_parts(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_soft_hard_rings() {
        let idx = WetlandIndex::from_parts(vec![
            (
                WetlandClass::HardAvoid,
                vec![
                    [10.0, 60.0],
                    [10.2, 60.0],
                    [10.2, 60.2],
                    [10.0, 60.2],
                    [10.0, 60.0],
                ],
            ),
            (
                WetlandClass::SoftAvoid,
                vec![
                    [11.0, 61.0],
                    [11.1, 61.0],
                    [11.1, 61.1],
                    [11.0, 61.1],
                    [11.0, 61.0],
                ],
            ),
        ]);
        let pack = FlatWetlandPack::from_wetland_index(&idx);
        assert_eq!(pack.ring_count(), 2);
        let back = pack.to_wetland_index(None);
        assert_eq!(back.ring_count(), 2);
        assert_eq!(back.class_at(60.1, 10.1), Some(WetlandClass::HardAvoid));
        assert_eq!(back.class_at(61.05, 11.05), Some(WetlandClass::SoftAvoid));
        // Bbox clip drops the soft ring.
        let clipped = pack.to_wetland_index(Some([59.9, 9.9, 60.3, 10.3]));
        assert_eq!(clipped.ring_count(), 1);
        assert_eq!(clipped.class_at(60.1, 10.1), Some(WetlandClass::HardAvoid));
        assert_eq!(clipped.class_at(61.05, 11.05), None);
    }
}
