//! Disk-space estimate for a long-trip region list.
//!
//! Reference (8-region Klecken trip, prior measurement):
//! - packs ≈ 10.43 GiB
//! - place index growth ≈ 2 GiB
//! - kept PBF extracts ≈ 3.84 GiB
//!
//! Peak disk during install is **not** measured on device; [`STAGING_SAFETY_FACTOR`]
//! (1.20) pads the sum. Flag this when tuning free-space UX.

/// place_index / packs from Klecken reference (2 / 10.43).
pub const PLACE_INDEX_RATIO: f64 = 2.0 / 10.43;
/// kept PBF / packs from Klecken reference (3.84 / 10.43).
pub const PBF_KEEP_RATIO: f64 = 3.84 / 10.43;
/// Staging pad — peak install footprint not measured.
pub const STAGING_SAFETY_FACTOR: f64 = 1.20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceReport {
    pub pack_bytes: u64,
    pub place_index_bytes: u64,
    pub pbf_keep_bytes: u64,
    pub needed_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpaceCheck {
    Ok(SpaceReport),
    InsufficientSpace {
        needed: u64,
        free: u64,
        shortfall: u64,
        report: SpaceReport,
    },
}

pub trait CatalogSizeLookup {
    fn pack_bytes_for_region(&self, region_id: &str) -> Option<u64>;
}

impl CatalogSizeLookup for [(String, u64)] {
    fn pack_bytes_for_region(&self, region_id: &str) -> Option<u64> {
        self.iter().find(|(id, _)| id == region_id).map(|(_, b)| *b)
    }
}

impl CatalogSizeLookup for Vec<(String, u64)> {
    fn pack_bytes_for_region(&self, region_id: &str) -> Option<u64> {
        self.iter().find(|(id, _)| id == region_id).map(|(_, b)| *b)
    }
}

/// Sum catalog pack sizes for `regions`, apply Klecken ratios + staging factor,
/// compare to `free_bytes`.
pub fn estimate_trip_disk_bytes(
    regions: &[String],
    sizes: &dyn CatalogSizeLookup,
    free_bytes: u64,
) -> SpaceCheck {
    let mut pack_bytes: u64 = 0;
    for id in regions {
        pack_bytes = pack_bytes.saturating_add(sizes.pack_bytes_for_region(id).unwrap_or(0));
    }
    let place_index_bytes = ((pack_bytes as f64) * PLACE_INDEX_RATIO).round() as u64;
    let pbf_keep_bytes = ((pack_bytes as f64) * PBF_KEEP_RATIO).round() as u64;
    let raw = pack_bytes
        .saturating_add(place_index_bytes)
        .saturating_add(pbf_keep_bytes);
    let needed_bytes = ((raw as f64) * STAGING_SAFETY_FACTOR).round() as u64;
    let report = SpaceReport {
        pack_bytes,
        place_index_bytes,
        pbf_keep_bytes,
        needed_bytes,
        free_bytes,
    };
    if needed_bytes > free_bytes {
        SpaceCheck::InsufficientSpace {
            needed: needed_bytes,
            free: free_bytes,
            shortfall: needed_bytes - free_bytes,
            report,
        }
    } else {
        SpaceCheck::Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortfall_when_free_below_needed() {
        let sizes = vec![("a".into(), 10_000_000_000u64)];
        let check = estimate_trip_disk_bytes(&["a".into()], &sizes, 1_000);
        match check {
            SpaceCheck::InsufficientSpace {
                needed,
                free,
                shortfall,
                ..
            } => {
                assert!(needed > free);
                assert_eq!(shortfall, needed - free);
            }
            other => panic!("expected InsufficientSpace, got {other:?}"),
        }
    }
}
