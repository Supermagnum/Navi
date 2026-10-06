//! Parse navi-server cached DATEX II SituationPublication XML (NPRA v3 shape).

use chrono::{DateTime, Datelike, FixedOffset, NaiveTime, Timelike, Utc, Weekday};
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
    /// Extra minutes used when [`DatexImpact::Penalize`].
    pub penalty_minutes: f64,
    /// True when `xsi:type` is not in the known inventory (defaults to Ignore).
    pub unrecognized_xsi_type: bool,
    /// `roadOrCarriagewayOrLaneManagementType` when present.
    pub management_type: Option<String>,
    /// DATEX `vehicleType` values (empty = all vehicles).
    pub vehicle_types: Vec<String>,
    /// Alert-C / coded direction if present (`positive` / `negative` / `both`).
    pub direction: Option<String>,
    /// Recurring validity windows (day + time-of-day). Empty = overall bounds only.
    pub valid_periods: Vec<DatexValidPeriod>,
}

/// One DATEX `validPeriod` (optional days + time-of-day, may wrap midnight).
#[derive(Debug, Clone, PartialEq)]
pub struct DatexValidPeriod {
    pub start: Option<DateTime<FixedOffset>>,
    pub end: Option<DateTime<FixedOffset>>,
    pub days: Vec<Weekday>,
    pub tod_start: Option<NaiveTime>,
    pub tod_end: Option<NaiveTime>,
}

impl DatexValidPeriod {
    /// `arrival` in UTC, compared using the period's offset when present, else UTC.
    pub fn contains_local(&self, arrival: DateTime<Utc>) -> bool {
        let local = if let Some(start) = self.start {
            arrival.with_timezone(start.offset())
        } else if let Some(end) = self.end {
            arrival.with_timezone(end.offset())
        } else {
            arrival.with_timezone(&FixedOffset::east_opt(0).unwrap())
        };
        if let Some(start) = self.start {
            if local < start {
                return false;
            }
        }
        if let Some(end) = self.end {
            if local > end {
                return false;
            }
        }
        if !self.days.is_empty() {
            let wd = weekday_from_chrono(local.weekday());
            if !self.days.contains(&wd) {
                return false;
            }
        }
        match (self.tod_start, self.tod_end) {
            (Some(a), Some(b)) => {
                let t = NaiveTime::from_hms_opt(local.hour(), local.minute(), local.second())
                    .unwrap_or(NaiveTime::MIN);
                if a <= b {
                    t >= a && t <= b
                } else {
                    t >= a || t <= b
                }
            }
            _ => true,
        }
    }
}

fn weekday_from_chrono(w: Weekday) -> Weekday {
    w
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

    /// Recurring day/time windows, then overall `[valid_from, valid_to]`.
    pub fn is_active_at_arrival(&self, arrival: DateTime<Utc>) -> bool {
        if !self.is_active_at(arrival) && !self.valid_periods.is_empty() {
            // Overall window may still contain `arrival`; is_active_at already
            // checked overall bounds. Recurring can only further restrict.
        }
        if !self.is_active_at(arrival) {
            return false;
        }
        if self.valid_periods.is_empty() {
            return true;
        }
        self.valid_periods
            .iter()
            .any(|p| p.contains_local(arrival))
    }

    pub fn has_recurring_windows(&self) -> bool {
        self.valid_periods
            .iter()
            .any(|p| !p.days.is_empty() || p.tod_start.is_some() || p.tod_end.is_some())
    }

    pub fn validity_text(&self) -> String {
        let mut parts = Vec::new();
        if let Some(s) = self.valid_from {
            parts.push(format!("from {}", s));
        }
        if let Some(e) = self.valid_to {
            parts.push(format!("to {}", e));
        }
        for p in &self.valid_periods {
            let days = if p.days.is_empty() {
                String::new()
            } else {
                p.days
                    .iter()
                    .map(|d| format!("{d:?}"))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let tod = match (p.tod_start, p.tod_end) {
                (Some(a), Some(b)) => format!(" {a}-{b}"),
                _ => String::new(),
            };
            if !days.is_empty() || !tod.is_empty() {
                parts.push(format!("{days}{tod}"));
            }
        }
        parts.join("; ")
    }

    /// True when the validity window overlaps `[window_start, window_end]`.
    /// Used at plan time so a sit that becomes active 5–30 min before the
    /// vehicle would reach it is still applied to A*, not dropped as "not now".
    pub fn is_active_during(&self, window_start: DateTime<Utc>, window_end: DateTime<Utc>) -> bool {
        if window_end < window_start {
            return false;
        }
        if let Some(start) = self.valid_from {
            if window_end < start.with_timezone(&Utc) {
                return false;
            }
        }
        if let Some(end) = self.valid_to {
            if window_start > end.with_timezone(&Utc) {
                return false;
            }
        }
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
    let management_type = first_text_local(node, "roadOrCarriagewayOrLaneManagementType")
        .or_else(|| first_text_local(node, "networkManagementType"));
    let vehicle_types = collect_local_texts(node, "vehicleType");
    let direction = first_text_local(node, "alertCDirectionCoded")
        .or_else(|| first_text_local(node, "directionCoded"))
        .or_else(|| first_text_local(node, "linearDirection"));
    let valid_periods = parse_valid_periods(node);
    let classified = classify_impact(&DatexClassifyFields {
        xsi_type: &xsi_type,
        lanes_restricted,
        severity: severity.as_deref(),
        comment: comment.as_deref(),
        location_description: location_description.as_deref(),
        delay_time_secs,
        delays_present,
        wind_speed,
        management_type: management_type.as_deref(),
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
        penalty_minutes: classified.penalty_minutes,
        unrecognized_xsi_type: classified.unrecognized_xsi_type,
        management_type,
        vehicle_types,
        direction,
        valid_periods,
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

fn collect_local_texts(node: roxmltree::Node<'_, '_>, local: &str) -> Vec<String> {
    node.descendants()
        .filter(|n| local_name(n.tag_name().name()) == local)
        .filter_map(|n| n.text().map(|t| t.trim().to_string()))
        .filter(|t| !t.is_empty())
        .collect()
}

fn parse_valid_periods(node: roxmltree::Node<'_, '_>) -> Vec<DatexValidPeriod> {
    let mut out = Vec::new();
    for n in node.descendants() {
        if local_name(n.tag_name().name()) != "validPeriod" {
            continue;
        }
        let mut start = None;
        let mut end = None;
        let mut days = Vec::new();
        let mut tod_start = None;
        let mut tod_end = None;
        for c in n.descendants() {
            match local_name(c.tag_name().name()) {
                "startOfPeriod" => start = c.text().and_then(parse_datex_time),
                "endOfPeriod" => end = c.text().and_then(parse_datex_time),
                "startTimeOfPeriod" => tod_start = c.text().and_then(parse_tod),
                "endTimeOfPeriod" => tod_end = c.text().and_then(parse_tod),
                "applicableDay" => {
                    if let Some(d) = c.text().and_then(parse_weekday) {
                        days.push(d);
                    }
                }
                _ => {}
            }
        }
        out.push(DatexValidPeriod {
            start,
            end,
            days,
            tod_start,
            tod_end,
        });
    }
    out
}

fn parse_tod(s: &str) -> Option<NaiveTime> {
    let t = s.trim();
    let core = t.split(['+', '-']).next().unwrap_or(t);
    let core = core.trim_end_matches('Z');
    NaiveTime::parse_from_str(core, "%H:%M:%S%.f")
        .ok()
        .or_else(|| NaiveTime::parse_from_str(core, "%H:%M:%S").ok())
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    match s.trim().to_ascii_lowercase().as_str() {
        "monday" => Some(Weekday::Mon),
        "tuesday" => Some(Weekday::Tue),
        "wednesday" => Some(Weekday::Wed),
        "thursday" => Some(Weekday::Thu),
        "friday" => Some(Weekday::Fri),
        "saturday" => Some(Weekday::Sat),
        "sunday" => Some(Weekday::Sun),
        _ => None,
    }
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
