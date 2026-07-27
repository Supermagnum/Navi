//! Shared next-maneuver guidance for approach UI and voice prompts.
//!
//! Distances are meters. Thresholds match [`docs/approach-instructions.md`].

use serde::{Deserialize, Serialize};

/// Appear when distance_m ≤ this and still above urgency.
pub const APPROACH_APPEAR_M: f64 = 750.0;
/// Urgency styling when distance_m ≤ this (and above hide).
pub const APPROACH_URGENCY_M: f64 = 150.0;
/// Hide when distance_m ≤ this (maneuver effectively passed).
pub const APPROACH_HIDE_M: f64 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManeuverKind {
    Left,
    Right,
    Straight,
    SlightLeft,
    SlightRight,
    SharpLeft,
    SharpRight,
    Roundabout,
    ExitLeft,
    ExitRight,
    MergeLeft,
    MergeRight,
    UTurn,
    Destination,
    KeepLeft,
    KeepRight,
    Unknown,
}

impl ManeuverKind {
    /// Icon key stem for the Navit `nav_*` set (without `_bk` / `_wh`).
    pub fn icon_key(self) -> &'static str {
        match self {
            Self::Left | Self::SlightLeft => "nav_left_1",
            Self::SharpLeft => "nav_left_3",
            Self::Right | Self::SlightRight => "nav_right_1",
            Self::SharpRight => "nav_right_3",
            Self::Straight => "nav_straight",
            Self::Roundabout => "nav_roundabout_r1",
            Self::ExitLeft => "nav_exit_left",
            Self::ExitRight => "nav_exit_right",
            Self::MergeLeft => "nav_merge_left",
            Self::MergeRight => "nav_merge_right",
            Self::UTurn => "nav_turnaround_left",
            Self::Destination => "nav_destination",
            Self::KeepLeft => "nav_keep_left",
            Self::KeepRight => "nav_keep_right",
            Self::Unknown => "nav_straight",
        }
    }

    /// Roundabout icon for exit 1..=3 (first/second/third), else generic.
    pub fn roundabout_icon_key(exit: Option<u8>) -> &'static str {
        match exit {
            Some(1) => "nav_roundabout_r1",
            Some(2) => "nav_roundabout_r2",
            Some(3) => "nav_roundabout_r3",
            _ => "nav_roundabout_r1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApproachPhase {
    Hidden,
    Appear,
    Urgency,
}

/// Live next-maneuver snapshot. Voice guidance and the approach box must share
/// this publisher — do not keep a second independent distance clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavGuidance {
    pub active: bool,
    pub kind: ManeuverKind,
    pub distance_m: f64,
    /// Preferred next-street label: OSM `name`, else `ref`. Empty = omit line.
    pub next_street: Option<String>,
    /// Roundabout exit index (1 = first). None when not a roundabout.
    pub roundabout_exit: Option<u8>,
}

impl Default for NavGuidance {
    fn default() -> Self {
        Self {
            active: false,
            kind: ManeuverKind::Unknown,
            distance_m: f64::INFINITY,
            next_street: None,
            roundabout_exit: None,
        }
    }
}

impl NavGuidance {
    pub fn phase(&self) -> ApproachPhase {
        if !self.active {
            return ApproachPhase::Hidden;
        }
        if !self.distance_m.is_finite() || self.distance_m > APPROACH_APPEAR_M {
            return ApproachPhase::Hidden;
        }
        if self.distance_m <= APPROACH_HIDE_M {
            return ApproachPhase::Hidden;
        }
        if self.distance_m <= APPROACH_URGENCY_M {
            ApproachPhase::Urgency
        } else {
            ApproachPhase::Appear
        }
    }

    pub fn icon_key(&self) -> &'static str {
        if self.kind == ManeuverKind::Roundabout {
            ManeuverKind::roundabout_icon_key(self.roundabout_exit)
        } else {
            self.kind.icon_key()
        }
    }

    /// Format distance for display. `prefer_metric`: metres / km vs feet / miles.
    pub fn format_distance(&self, prefer_metric: bool) -> String {
        format_distance_m(self.distance_m, prefer_metric)
    }
}

pub fn format_distance_m(distance_m: f64, prefer_metric: bool) -> String {
    if !distance_m.is_finite() || distance_m < 0.0 {
        return String::new();
    }
    if prefer_metric {
        if distance_m < 1000.0 {
            format!("{distance_m:.0} m")
        } else {
            format!("{:.1} km", distance_m / 1000.0)
        }
    } else {
        let feet = distance_m * 3.28084;
        if feet < 1000.0 {
            format!("{feet:.0} ft")
        } else {
            format!("{:.1} mi", distance_m / 1609.344)
        }
    }
}

/// Prefer colloquial name over systematic ref (Navit surfaces both; Navi box uses one line).
pub fn prefer_street_label(name: Option<&str>, systematic_ref: Option<&str>) -> Option<String> {
    let n = name.map(str::trim).filter(|s| !s.is_empty());
    if let Some(n) = n {
        return Some(n.to_string());
    }
    systematic_ref
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Label for the road the vehicle is **currently on**.
///
/// Order: OSM `name`, else `ref`, else human highway-class label (never a raw
/// `highway=*` tag). Used by the bottom HUD “Currently on …” line — distinct
/// from approach-box next-street (which omits when name/ref are unknown).
pub fn current_road_label(
    name: Option<&str>,
    systematic_ref: Option<&str>,
    highway: Option<&str>,
) -> String {
    if let Some(s) = prefer_street_label(name, systematic_ref) {
        return s;
    }
    crate::routing::eta::highway_class_display_label(highway).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_match_locked_thresholds() {
        let mut g = NavGuidance {
            active: true,
            kind: ManeuverKind::Right,
            distance_m: 800.0,
            next_street: Some("Testvegen".into()),
            roundabout_exit: None,
        };
        assert_eq!(g.phase(), ApproachPhase::Hidden);
        g.distance_m = 750.0;
        assert_eq!(g.phase(), ApproachPhase::Appear);
        g.distance_m = 151.0;
        assert_eq!(g.phase(), ApproachPhase::Appear);
        g.distance_m = 150.0;
        assert_eq!(g.phase(), ApproachPhase::Urgency);
        g.distance_m = 26.0;
        assert_eq!(g.phase(), ApproachPhase::Urgency);
        g.distance_m = 25.0;
        assert_eq!(g.phase(), ApproachPhase::Hidden);
        g.active = false;
        g.distance_m = 100.0;
        assert_eq!(g.phase(), ApproachPhase::Hidden);
    }

    #[test]
    fn prefer_name_over_ref() {
        assert_eq!(
            prefer_street_label(Some("Kirkegata"), Some("Fv2")),
            Some("Kirkegata".into())
        );
        assert_eq!(prefer_street_label(None, Some("E6")), Some("E6".into()));
        assert_eq!(
            prefer_street_label(Some("  "), Some("E6")),
            Some("E6".into())
        );
        assert_eq!(prefer_street_label(None, None), None);
    }

    #[test]
    fn current_road_prefers_name_then_ref_then_class() {
        assert_eq!(
            current_road_label(Some("Storgata"), Some("Fv2"), Some("residential")),
            "Storgata"
        );
        assert_eq!(current_road_label(None, Some("E6"), Some("trunk")), "E6");
        assert_eq!(
            current_road_label(None, None, Some("service")),
            "Service road"
        );
        assert_eq!(current_road_label(Some("  "), None, Some("path")), "Path");
    }

    #[test]
    fn current_road_preserves_norwegian_special_chars() {
        let label = current_road_label(Some("Mjøsvegen"), None, Some("tertiary"));
        assert_eq!(label, "Mjøsvegen");
        assert!(label.contains('ø'));
        let label2 = current_road_label(Some("Trollåsveien"), None, None);
        assert!(label2.contains('å'));
        let label3 = current_road_label(Some("Kjølberggata"), None, None);
        assert!(label3.contains('ø'));
        // æ in real Østlandet place data (Ævongsli / camping names).
        let label4 = current_road_label(Some("Ævongsli"), None, Some("residential"));
        assert!(label4.contains('Æ') || label4.contains('æ'));
    }
}
