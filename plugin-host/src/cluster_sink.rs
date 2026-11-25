//! Host-owned backends for `vehicle_signal_publish`.
//!
//! Guests never open sockets: the trusted host routes `navi.cluster.v1` JSON
//! through one of these sinks (log, USB NDJSON buffer, Kuksa path log, or a
//! recorded JSON-UDP packet list).

use serde_json::Value;

/// Where a published cluster document goes after the guest calls
/// `vehicle_signal_publish`.
pub trait ClusterSink: Send {
    fn name(&self) -> &'static str;
    fn publish_cluster_json(&mut self, json: &str) -> Result<(), String>;
}

/// Default milestone sink: record a short preview via an internal buffer
/// (and optionally mirror to a callback).
#[derive(Debug, Default)]
pub struct LogSink {
    pub lines: Vec<String>,
}

impl ClusterSink for LogSink {
    fn name(&self) -> &'static str {
        "log"
    }

    fn publish_cluster_json(&mut self, json: &str) -> Result<(), String> {
        let preview: String = json.chars().take(240).collect();
        self.lines
            .push(format!("vehicle_signal_publish: {preview}"));
        Ok(())
    }
}

/// USB-serial framing stand-in: one UTF-8 `navi.cluster.v1` object per line
/// (NDJSON). Real USB open stays in the Android host; this buffer is what the
/// host would write once a port is open.
#[derive(Debug, Default)]
pub struct NdjsonBufferSink {
    pub lines: Vec<String>,
}

impl ClusterSink for NdjsonBufferSink {
    fn name(&self) -> &'static str {
        "usb_ndjson"
    }

    fn publish_cluster_json(&mut self, json: &str) -> Result<(), String> {
        let trimmed = json.trim();
        if trimmed.is_empty() {
            return Err("empty cluster json".into());
        }
        // One object per line; strip embedded newlines so framing stays intact.
        let line = trimmed.replace(['\n', '\r'], " ");
        self.lines.push(line);
        Ok(())
    }
}

/// Maps a cluster document to `Vehicle.Private.Navi.*` (and standard location /
/// speed) path/value pairs and records them. A live Kuksa.val gRPC client can
/// consume the same mapping later.
#[derive(Debug, Default)]
pub struct KuksaPathLogSink {
    pub updates: Vec<(String, String)>,
}

impl ClusterSink for KuksaPathLogSink {
    fn name(&self) -> &'static str {
        "kuksa"
    }

    fn publish_cluster_json(&mut self, json: &str) -> Result<(), String> {
        self.updates.extend(cluster_json_to_vss_updates(json)?);
        Ok(())
    }
}

/// Records payloads that a loopback JSON-UDP / WS sink would emit. Does **not**
/// bind sockets (keeps tests and default builds free of network side effects).
#[derive(Debug, Default)]
pub struct JsonUdpRecordSink {
    pub packets: Vec<String>,
}

impl ClusterSink for JsonUdpRecordSink {
    fn name(&self) -> &'static str {
        "json_udp"
    }

    fn publish_cluster_json(&mut self, json: &str) -> Result<(), String> {
        self.packets.push(json.to_string());
        Ok(())
    }
}

/// Sink kind selected from host settings (default: log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterSinkKind {
    Log,
    UsbNdjson,
    Kuksa,
    JsonUdp,
}

impl ClusterSinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::UsbNdjson => "usb_ndjson",
            Self::Kuksa => "kuksa",
            Self::JsonUdp => "json_udp",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "log" => Some(Self::Log),
            "usb_ndjson" | "usb" | "ndjson" => Some(Self::UsbNdjson),
            "kuksa" | "vss" => Some(Self::Kuksa),
            "json_udp" | "udp" | "json_ws" | "ws" => Some(Self::JsonUdp),
            _ => None,
        }
    }

    pub fn boxed_default(self) -> Box<dyn ClusterSink> {
        match self {
            Self::Log => Box::new(LogSink::default()),
            Self::UsbNdjson => Box::new(NdjsonBufferSink::default()),
            Self::Kuksa => Box::new(KuksaPathLogSink::default()),
            Self::JsonUdp => Box::new(JsonUdpRecordSink::default()),
        }
    }
}

/// Map a `navi.cluster.v1` (or guidance) document into VSS path updates.
pub fn cluster_json_to_vss_updates(json: &str) -> Result<Vec<(String, String)>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("cluster json: {e}"))?;
    let mut out = Vec::new();

    if let Some(pos) = v.get("position") {
        push_num(&mut out, "Vehicle.CurrentLocation.Latitude", pos.get("lat"));
        push_num(
            &mut out,
            "Vehicle.CurrentLocation.Longitude",
            pos.get("lon"),
        );
        push_num(
            &mut out,
            "Vehicle.CurrentLocation.Heading",
            pos.get("heading_deg"),
        );
        push_num(
            &mut out,
            "Vehicle.CurrentLocation.HorizontalAccuracy",
            pos.get("h_acc_m"),
        );
        push_num(&mut out, "Vehicle.Speed", pos.get("speed_kmh"));
    }

    let route_active = v
        .get("route_active")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    out.push((
        "Vehicle.Private.Navi.Route.Active".into(),
        bool_str(route_active),
    ));

    if let Some(m) = v.get("maneuver").filter(|m| !m.is_null()) {
        push_str(
            &mut out,
            "Vehicle.Private.Navi.Guidance.NextManeuver.Type",
            m.get("type"),
        );
        push_num(
            &mut out,
            "Vehicle.Private.Navi.Guidance.NextManeuver.DistanceM",
            m.get("distance_m"),
        );
        push_str(
            &mut out,
            "Vehicle.Private.Navi.Guidance.NextManeuver.StreetName",
            m.get("street_name"),
        );
        push_str(
            &mut out,
            "Vehicle.Private.Navi.Guidance.NextManeuver.IconKey",
            m.get("icon_key"),
        );
    }

    let overspeed = v
        .get("overspeed")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    out.push((
        "Vehicle.Private.Navi.Speed.Overspeed".into(),
        bool_str(overspeed),
    ));

    if let Some(eco) = v.get("eco") {
        let enabled = eco
            .get("enabled")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        out.push(("Vehicle.Private.Navi.Eco.Enabled".into(), bool_str(enabled)));
        push_str(
            &mut out,
            "Vehicle.Private.Navi.Eco.Profile",
            eco.get("profile"),
        );
    }

    if route_active {
        if let Some(trip) = v.get("trip").filter(|t| !t.is_null()) {
            push_str(&mut out, "Vehicle.Private.Navi.Trip.Eta", trip.get("eta"));
            push_num(
                &mut out,
                "Vehicle.Private.Navi.Trip.RemainingDurationS",
                trip.get("remaining_duration_s"),
            );
            push_num(
                &mut out,
                "Vehicle.Private.Navi.Trip.RemainingDistanceM",
                trip.get("remaining_distance_m"),
            );
        }
        if let Some(rest) = v.get("rest").filter(|r| !r.is_null()) {
            let active = rest.get("active").and_then(|x| x.as_bool()).unwrap_or(true);
            out.push(("Vehicle.Private.Navi.Rest.Active".into(), bool_str(active)));
            push_num(
                &mut out,
                "Vehicle.Private.Navi.Rest.MinutesToBreak",
                rest.get("minutes_to_break"),
            );
            push_num(
                &mut out,
                "Vehicle.Private.Navi.Rest.DistanceToBreakM",
                rest.get("distance_to_break_m"),
            );
        }
    } else {
        out.push(("Vehicle.Private.Navi.Rest.Active".into(), bool_str(false)));
    }

    match v.get("warning") {
        Some(w) if !w.is_null() => {
            out.push(("Vehicle.Private.Navi.Warning.Active".into(), bool_str(true)));
            push_str(
                &mut out,
                "Vehicle.Private.Navi.Warning.Category",
                w.get("category"),
            );
            push_str(
                &mut out,
                "Vehicle.Private.Navi.Warning.Phase",
                w.get("phase"),
            );
            push_num(
                &mut out,
                "Vehicle.Private.Navi.Warning.DistanceM",
                w.get("distance_m"),
            );
            push_str(&mut out, "Vehicle.Private.Navi.Warning.Code", w.get("code"));
            push_str(
                &mut out,
                "Vehicle.Private.Navi.Warning.IconKey",
                w.get("icon_key"),
            );
            push_str(
                &mut out,
                "Vehicle.Private.Navi.Warning.Label",
                w.get("label"),
            );
            push_str(
                &mut out,
                "Vehicle.Private.Navi.Warning.Source",
                w.get("source"),
            );
        }
        _ => {
            out.push((
                "Vehicle.Private.Navi.Warning.Active".into(),
                bool_str(false),
            ));
        }
    }

    if let Some(poly) = v
        .get("route")
        .and_then(|r| r.get("polyline"))
        .and_then(|p| p.as_str())
    {
        out.push((
            "Vehicle.Private.Navi.Route.Polyline".into(),
            poly.to_string(),
        ));
    }

    Ok(out)
}

fn bool_str(b: bool) -> String {
    if b {
        "true".into()
    } else {
        "false".into()
    }
}

fn push_num(out: &mut Vec<(String, String)>, path: &str, v: Option<&Value>) {
    if let Some(n) = v.and_then(|x| x.as_f64()) {
        out.push((path.into(), format!("{n}")));
    }
}

fn push_str(out: &mut Vec<(String, String)>, path: &str, v: Option<&Value>) {
    if let Some(s) = v.and_then(|x| x.as_str()) {
        out.push((path.into(), s.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndjson_strips_newlines() {
        let mut s = NdjsonBufferSink::default();
        s.publish_cluster_json("{\"schema\":\"navi.cluster.v1\"\n}")
            .unwrap();
        assert_eq!(s.lines.len(), 1);
        assert!(!s.lines[0].contains('\n'));
    }

    #[test]
    fn kuksa_maps_warning_and_clears_when_null() {
        let json = r#"{
            "schema":"navi.cluster.v1",
            "route_active":true,
            "overspeed":false,
            "position":{"lat":60.68,"lon":11.34,"speed_kmh":40.0},
            "warning":{"category":"road_sign","phase":"appear","code":"109","icon_key":"no_sign_109","label":"School"},
            "eco":{"enabled":false,"profile":null},
            "route":{"polyline":""}
        }"#;
        let updates = cluster_json_to_vss_updates(json).unwrap();
        assert!(updates
            .iter()
            .any(|(p, v)| p == "Vehicle.Private.Navi.Warning.Active" && v == "true"));
        assert!(updates
            .iter()
            .any(|(p, v)| p == "Vehicle.Private.Navi.Warning.Code" && v == "109"));

        let cleared = cluster_json_to_vss_updates(
            r#"{"schema":"navi.cluster.v1","route_active":false,"overspeed":false,"warning":null}"#,
        )
        .unwrap();
        assert!(cleared
            .iter()
            .any(|(p, v)| p == "Vehicle.Private.Navi.Warning.Active" && v == "false"));
        assert!(!cleared
            .iter()
            .any(|(p, _)| p == "Vehicle.Private.Navi.Warning.Code"));
    }

    #[test]
    fn sink_kind_parses() {
        assert_eq!(ClusterSinkKind::parse("log"), Some(ClusterSinkKind::Log));
        assert_eq!(
            ClusterSinkKind::parse("usb"),
            Some(ClusterSinkKind::UsbNdjson)
        );
        assert_eq!(
            ClusterSinkKind::parse("kuksa"),
            Some(ClusterSinkKind::Kuksa)
        );
        assert_eq!(
            ClusterSinkKind::parse("json_udp"),
            Some(ClusterSinkKind::JsonUdp)
        );
    }
}
