//! Typed plan failures when corridor packs are incomplete.
//!
//! Distinguishes missing installed regions from a true “no route” on fully
//! covered data and from waypoint snap failures ([`SnapTooFar`]).

use crate::pack_server::corridor_regions::{ordered_regions_along_corridor, CatalogRegionEntry};
use crate::routing::graph::SnapTooFar;

/// Failure modes for a long-corridor plan attempt.
///
/// Keep this core-only for now: surfacing [`Self::MissingRegions`] through
/// UniFFI would need a `CorridorRouteResult` field or a new
/// `search_terminate_reason` token — see module docs in the long-trip report.
#[derive(Debug, Clone, PartialEq)]
pub enum RegionPlanError {
    /// Required catalog regions along the corridor are not installed, in
    /// first-crossing order.
    MissingRegions(Vec<String>),
    /// Graph is loaded for the corridor but A* found no path.
    NoRoute { detail: String },
    /// Snap exceeded the profile cap (unchanged [`SnapTooFar`] semantics).
    SnapTooFar(SnapTooFar),
}

impl RegionPlanError {
    pub fn is_missing_regions(&self) -> bool {
        matches!(self, Self::MissingRegions(_))
    }

    pub fn missing_regions(&self) -> Option<&[String]> {
        match self {
            Self::MissingRegions(r) => Some(r.as_slice()),
            _ => None,
        }
    }
}

impl std::fmt::Display for RegionPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRegions(regions) => {
                write!(f, "missing regions along route: {}", regions.join(", "))
            }
            Self::NoRoute { detail } => write!(f, "no route found ({detail})"),
            Self::SnapTooFar(s) => write!(
                f,
                "snap too far: nearest={:.0} m max={:.0} m",
                s.nearest_m, s.max_m
            ),
        }
    }
}

impl std::error::Error for RegionPlanError {}

impl From<SnapTooFar> for RegionPlanError {
    fn from(value: SnapTooFar) -> Self {
        Self::SnapTooFar(value)
    }
}

/// If any catalog regions along `waypoints` are not in `installed`, return
/// [`RegionPlanError::MissingRegions`]. Otherwise `Ok(())` — caller may plan
/// and must still surface snap / no-route distinctly.
pub fn ensure_corridor_regions_installed(
    waypoints: &[(f64, f64)],
    catalog: &[CatalogRegionEntry],
    installed: &[String],
    sample_step_km: f64,
) -> Result<(), RegionPlanError> {
    let missing = ordered_regions_along_corridor(waypoints, catalog, installed, sample_step_km);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(RegionPlanError::MissingRegions(missing))
    }
}

/// Map a post-coverage plan failure into a typed error.
///
/// Prefer calling [`ensure_corridor_regions_installed`] first. Use this when a
/// snap or A* failure is already in hand and coverage was confirmed complete.
pub fn region_plan_error_from_snap_or_no_route(
    snap: Option<SnapTooFar>,
    no_route_detail: impl Into<String>,
) -> RegionPlanError {
    if let Some(s) = snap {
        RegionPlanError::SnapTooFar(s)
    } else {
        RegionPlanError::NoRoute {
            detail: no_route_detail.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack_server::catalog_entries_from_ready_ids;

    fn all_road() -> Vec<(f64, f64)> {
        vec![
            (53.3340, 10.0450),
            (53.5510, 10.0000),
            (54.7830, 9.4330),
            (55.4900, 9.4700),
            (55.4000, 10.3900),
            (55.3500, 11.1300),
            (55.4100, 11.3800),
            (55.6760, 12.5680),
            (55.5700, 12.8500),
            (55.6050, 13.0000),
            (56.0500, 12.7000),
            (56.6700, 12.8600),
            (57.7100, 11.9700),
            (59.0900, 11.2500),
            (59.9100, 10.7500),
            (61.5929077, 10.3318551),
        ]
    }

    fn current_catalog() -> Vec<CatalogRegionEntry> {
        catalog_entries_from_ready_ids(&[
            "europe/germany/niedersachsen".into(),
            "europe/germany/hamburg".into(),
            "europe/germany/schleswig-holstein".into(),
            "europe/denmark".into(),
            "europe/sweden/skane".into(),
            "europe/sweden/halland".into(),
            "europe/sweden/vastra_gotaland".into(),
            "europe/norway/ostlandet".into(),
        ])
    }

    #[test]
    fn incomplete_niedersachsen_returns_seven_missing_regions() {
        let err = ensure_corridor_regions_installed(
            &all_road(),
            &current_catalog(),
            &["europe/germany/niedersachsen".into()],
            25.0,
        )
        .expect_err("must report missing");
        match err {
            RegionPlanError::MissingRegions(regions) => {
                assert_eq!(
                    regions,
                    vec![
                        "europe/germany/hamburg".to_string(),
                        "europe/germany/schleswig-holstein".to_string(),
                        "europe/denmark".to_string(),
                        "europe/sweden/skane".to_string(),
                        "europe/sweden/halland".to_string(),
                        "europe/sweden/vastra_gotaland".to_string(),
                        "europe/norway/ostlandet".to_string(),
                    ]
                );
            }
            other => panic!("expected MissingRegions, got {other:?}"),
        }
    }

    #[test]
    fn full_install_is_ok_and_snap_stays_distinct() {
        let installed = vec![
            "europe/germany/niedersachsen".into(),
            "europe/germany/hamburg".into(),
            "europe/germany/schleswig-holstein".into(),
            "europe/denmark".into(),
            "europe/sweden/skane".into(),
            "europe/sweden/halland".into(),
            "europe/sweden/vastra_gotaland".into(),
            "europe/norway/ostlandet".into(),
        ];
        ensure_corridor_regions_installed(&all_road(), &current_catalog(), &installed, 25.0)
            .expect("complete coverage");
        let snap = region_plan_error_from_snap_or_no_route(
            Some(SnapTooFar {
                nearest_m: 1200.0,
                max_m: 750.0,
            }),
            "unused",
        );
        assert!(matches!(snap, RegionPlanError::SnapTooFar(_)));
        assert!(!snap.is_missing_regions());
        let no_route = region_plan_error_from_snap_or_no_route(None, "disconnected");
        assert!(matches!(no_route, RegionPlanError::NoRoute { .. }));
        assert!(!no_route.is_missing_regions());
    }
}
