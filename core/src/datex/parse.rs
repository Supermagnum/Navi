//! Parse navi-server cached DATEX II SituationPublication XML (NPRA v3 shape).

use chrono::{DateTime, FixedOffset, Utc};
use roxmltree::Document;

use super::impact::{classify_impact, DatexClassifyFields, DatexImpact};

/// Coarse situation class derived from the DATEX `xsi:type` local name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SituationKind {
    Roadworks,
    Incident,
    Closure,
    SpeedManagement,
    Rerouting,
    Other,
}

impl SituationKind {
    pub fn from_xsi_type(local: &str) -> Self {
        match local {
            "MaintenanceWorks" | "ConstructionWorks" | "Roadworks" => Self::Roadworks,
            "Accident"
            | "VehicleObstruction"
            | "AnimalPresenceObstruction"
            | "EnvironmentalObstruction"
            | "GeneralObstruction"
            | "InfrastructureDamageObstruction"
            | "NonWeatherRelatedRoadConditions"
            | "PoorEnvironmentConditions"
            | "AbnormalTraffic"
            | "WeatherRelatedRoadConditions" => Self::Incident,
            "RoadOrCarriagewayOrLaneManagement" | "AuthorityOperation" => Self::Closure,
            "SpeedManagement" => Self::SpeedManagement,
            "ReroutingManagement" => Self::Rerouting,
            _ => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Roadworks => "roadworks",
            Self::Incident => "incident",
            Self::Closure => "closure",
            Self::SpeedManagement => "speed_management",
            Self::Rerouting => "rerouting",
            Self::Other => "other",
        }
    }
}

/// One DATEX situation record with display geometry and validity window.
#[derive(Debug, Clone, PartialEq)]
pub struct DatexSituation {
    pub id: String,
    pub kind: SituationKind,
    pub xsi_type: String,
    /// WGS84 points from `coordinatesForDisplay` (lat, lon). First point is primary.
    pub geometry: Vec<(f64, f64)>,
    pub valid_from: Option<DateTime<FixedOffset>>,
    pub valid_to: Option<DateTime<FixedOffset>>,
    pub road_number: Option<String>,
    pub location_description: Option<String>,
    pub comment: Option<String>,
    /// DATEX `severity` on the situation record (e.g. `none` / `low` / `high`).
    pub severity: Option<String>,
    /// DATEX `impact/numberOfLanesRestricted` when present.
    pub lanes_restricted: Option<u32>,
    /// DATEX `impact/delays/delayTimeValue` in seconds (DATEX Seconds).
    pub delay_time_secs: Option<f64>,
    /// True when an `impact/delays` element exists (numeric value may be absent).
    pub delays_present: bool,
    /// DATEX `windSpeed` (m/s) when present.
    pub wind_speed: Option<f64>,
    /// Planner classification from `xsi:type` plus fields.
    pub impact: DatexImpact,
    /// Cost multiplier used when [`DatexImpact::Penalize`].
    pub penalize_mult: f64,
    /// True when `xsi:type` is not in the known inventory (defaults to Ignore).
    pub unrecognized_xsi_type: bool,
}

impl DatexSituation {
    pub fn primary_lat_lon(&self) -> Option<(f64, f64)> {
        self.geometry.first().copied()
    }

    /// `true` when `now` falls inside `[valid_from, valid_to]` (open ends allowed).
    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        if let Some(start) = self.valid_from {
            if now < start.with_timezone(&Utc) {
                return false;
            }
        }
        if let Some(end) = self.valid_to {
            if now > end.with_timezone(&Utc) {
                return false;
            }
        }
        // Require at least one bound so empty validity does not count as forever-active.
        self.valid_from.is_some() || self.valid_to.is_some()
    }
}

/// Parse a full GetSituation XML body into situation records.
pub fn parse_situation_publication(xml: &str) -> Result<Vec<DatexSituation>, String> {
    let doc = Document::parse(xml).map_err(|e| format!("datex xml: {e}"))?;
    let mut out = Vec::new();
    for node in doc.descendants() {
        if local_name(node.tag_name().name()) != "situationRecord" {
            continue;
        }
        if let Some(sit) = parse_situation_record(node) {
            out.push(sit);
        }
    }
    Ok(out)
}

fn parse_situation_record(node: roxmltree::Node<'_, '_>) -> Option<DatexSituation> {
    let id = node
        .attribute("id")
        .map(str::to_string)
        .unwrap_or_else(|| "unknown".into());
    let xsi_type = node
        .attribute(("http://www.w3.org/2001/XMLSchema-instance", "type"))
        .or_else(|| {
            node.attributes()
                .find(|a| a.name() == "type" || a.name().ends_with(":type"))
                .map(|a| a.value())
        })
        .map(strip_type_prefix)
        .unwrap_or_else(|| "Unknown".into());

    let kind = SituationKind::from_xsi_type(&xsi_type);
    let geometry = collect_display_coords(node);
    if geometry.is_empty() {
        return None;
    }

    let mut valid_from = None;
    let mut valid_to = None;
    for n in node.descendants() {
        match local_name(n.tag_name().name()) {
            "overallStartTime" => {
                if let Some(t) = n.text().and_then(parse_datex_time) {
                    valid_from = Some(t);
                }
            }
            "overallEndTime" => {
                if let Some(t) = n.text().and_then(parse_datex_time) {
                    valid_to = Some(t);
                }
            }
            _ => {}
        }
    }

    let road_number = first_text_local(node, "roadNumber");
    let location_description = first_nested_value(node, "locationDescription");
    let comment = first_nested_value(node, "comment");
    let severity = first_text_local(node, "severity");
    let lanes_restricted =
        first_text_local(node, "numberOfLanesRestricted").and_then(|s| s.parse::<u32>().ok());
    let delay_time_secs =
        first_text_local(node, "delayTimeValue").and_then(|s| s.parse::<f64>().ok());
    let delays_present = node
        .descendants()
        .any(|n| local_name(n.tag_name().name()) == "delays");
    let wind_speed = first_text_local(node, "windSpeed").and_then(|s| s.parse::<f64>().ok());
    let classified = classify_impact(&DatexClassifyFields {
        xsi_type: &xsi_type,
        lanes_restricted,
        severity: severity.as_deref(),
        comment: comment.as_deref(),
        location_description: location_description.as_deref(),
        delay_time_secs,
        delays_present,
        wind_speed,
    });

    Some(DatexSituation {
        id,
        kind,
        xsi_type,
        geometry,
        valid_from,
        valid_to,
        road_number,
        location_description,
        comment,
        severity,
        lanes_restricted,
        delay_time_secs,
        delays_present,
        wind_speed,
        impact: classified.impact,
        penalize_mult: classified.penalize_mult,
        unrecognized_xsi_type: classified.unrecognized_xsi_type,
    })
}

fn collect_display_coords(node: roxmltree::Node<'_, '_>) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for n in node.descendants() {
        if local_name(n.tag_name().name()) != "coordinatesForDisplay" {
            continue;
        }
        let mut lat = None;
        let mut lon = None;
        for c in n.children().filter(|c| c.is_element()) {
            match local_name(c.tag_name().name()) {
                "latitude" => lat = c.text().and_then(|t| t.trim().parse().ok()),
                "longitude" => lon = c.text().and_then(|t| t.trim().parse().ok()),
                _ => {}
            }
        }
        if let (Some(la), Some(lo)) = (lat, lon) {
            out.push((la, lo));
        }
    }
    out
}

fn first_text_local(node: roxmltree::Node<'_, '_>, local: &str) -> Option<String> {
    node.descendants()
        .find(|n| local_name(n.tag_name().name()) == local)
        .and_then(|n| n.text())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

fn first_nested_value(node: roxmltree::Node<'_, '_>, wrapper_local: &str) -> Option<String> {
    let wrap = node
        .descendants()
        .find(|n| local_name(n.tag_name().name()) == wrapper_local)?;
    wrap.descendants()
        .find(|n| local_name(n.tag_name().name()) == "value")
        .and_then(|n| n.text())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

fn parse_datex_time(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    DateTime::parse_from_rfc3339(s)
        .ok()
        .or_else(|| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%z").ok())
        .or_else(|| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%z").ok())
}

fn strip_type_prefix(v: &str) -> String {
    v.rsplit(':').next().unwrap_or(v).to_string()
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_mapping_covers_live_and_unused_types() {
        assert_eq!(
            SituationKind::from_xsi_type("MaintenanceWorks"),
            SituationKind::Roadworks
        );
        assert_eq!(
            SituationKind::from_xsi_type("Accident"),
            SituationKind::Incident
        );
        assert_eq!(
            SituationKind::from_xsi_type("RoadOrCarriagewayOrLaneManagement"),
            SituationKind::Closure
        );
        assert_eq!(
            SituationKind::from_xsi_type("SpeedManagement"),
            SituationKind::SpeedManagement
        );
        assert_eq!(
            SituationKind::from_xsi_type("ReroutingManagement"),
            SituationKind::Rerouting
        );
        assert_eq!(
            SituationKind::from_xsi_type("TransitInformation"),
            SituationKind::Other
        );
        assert_eq!(
            SituationKind::from_xsi_type("WeatherRelatedRoadConditions"),
            SituationKind::Incident
        );
        assert_eq!(
            SituationKind::from_xsi_type("CompletelyFutureSituationType"),
            SituationKind::Other
        );
    }
}
