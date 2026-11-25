//! Host-owned backends for `vehicle_signal_publish`.
//!
//! Guests never open sockets: the trusted host routes `navi.cluster.v1` JSON
//! through one of these sinks (log, USB NDJSON buffer, live Kuksa TCP bridge,
//! or live JSON-UDP).

use std::io::Write;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use serde_json::{json, Value};

/// Where a published cluster document goes after the guest calls
/// `vehicle_signal_publish`.
pub trait ClusterSink: Send {
    fn name(&self) -> &'static str;
    fn publish_cluster_json(&mut self, json: &str) -> Result<(), String>;
}

/// Network endpoint for live JSON-UDP / Kuksa bridge sinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterEndpoint {
    pub host: String,
    pub port: u16,
}

impl ClusterEndpoint {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    pub fn socket_addr(&self) -> Result<SocketAddr, String> {
        let mut addrs = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| format!("resolve {}:{}: {e}", self.host, self.port))?;
        addrs
            .next()
            .ok_or_else(|| format!("no address for {}:{}", self.host, self.port))
    }

    pub fn display(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

impl Default for ClusterEndpoint {
    fn default() -> Self {
        Self::new("127.0.0.1", 8765)
    }
}

/// Default JSON-UDP endpoint (loopback).
pub fn default_json_udp_endpoint() -> ClusterEndpoint {
    ClusterEndpoint::new("127.0.0.1", 8765)
}

/// Default Kuksa / VSS bridge TCP endpoint (loopback).
///
/// Wire format: one UTF-8 JSON line per publish
/// `{"schema":"navi.vss.batch.v1","updates":[{"path":"...","value":"..."}]}`.
/// A small feeder can forward these into a Kuksa.val databroker.
pub fn default_kuksa_endpoint() -> ClusterEndpoint {
    ClusterEndpoint::new("127.0.0.1", 55556)
}

/// Default milestone sink: record a short preview via an internal buffer.
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

/// USB-serial framing: one UTF-8 `navi.cluster.v1` object per line (NDJSON).
/// Android drains [`NdjsonBufferSink::lines`] (via the session outbox) and writes
/// them to an open USB accessory / serial port.
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
        let line = trimmed.replace(['\n', '\r'], " ");
        self.lines.push(line);
        Ok(())
    }
}

/// Live Kuksa / VSS bridge: map cluster JSON to VSS paths and push a TCP NDJSON
/// batch to [`ClusterEndpoint`]. Also keeps an in-memory update log for tests.
pub struct KuksaLiveSink {
    pub endpoint: ClusterEndpoint,
    pub updates: Vec<(String, String)>,
    pub last_error: Option<String>,
    stream: Option<TcpStream>,
}

impl KuksaLiveSink {
    pub fn new(endpoint: ClusterEndpoint) -> Self {
        Self {
            endpoint,
            updates: Vec::new(),
            last_error: None,
            stream: None,
        }
    }

    fn ensure_stream(&mut self) -> Result<&mut TcpStream, String> {
        if self.stream.is_some() {
            return self
                .stream
                .as_mut()
                .ok_or_else(|| "kuksa stream missing".into());
        }
        let addr = self.endpoint.socket_addr()?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(400))
            .map_err(|e| format!("kuksa connect {}: {e}", self.endpoint.display()))?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
        self.stream = Some(stream);
        self.stream
            .as_mut()
            .ok_or_else(|| "kuksa stream missing".into())
    }
}

impl ClusterSink for KuksaLiveSink {
    fn name(&self) -> &'static str {
        "kuksa"
    }

    fn publish_cluster_json(&mut self, json: &str) -> Result<(), String> {
        let batch = cluster_json_to_vss_updates(json)?;
        self.updates.extend(batch.iter().cloned());
        let payload = json!({
            "schema": "navi.vss.batch.v1",
            "updates": batch.iter().map(|(p, v)| json!({"path": p, "value": v})).collect::<Vec<_>>(),
        })
        .to_string();
        let line = format!("{payload}\n");
        match self.ensure_stream().and_then(|s| {
            s.write_all(line.as_bytes())
                .and_then(|_| s.flush())
                .map_err(|e| format!("kuksa write: {e}"))
        }) {
            Ok(()) => {
                self.last_error = None;
                Ok(())
            }
            Err(e) => {
                self.stream = None;
                self.last_error = Some(e.clone());
                Err(e)
            }
        }
    }
}

/// Live JSON-UDP: send each cluster document as one UDP datagram (+ record).
pub struct JsonUdpLiveSink {
    pub endpoint: ClusterEndpoint,
    pub packets: Vec<String>,
    pub last_error: Option<String>,
    socket: Option<UdpSocket>,
}

impl JsonUdpLiveSink {
    pub fn new(endpoint: ClusterEndpoint) -> Self {
        Self {
            endpoint,
            packets: Vec::new(),
            last_error: None,
            socket: None,
        }
    }

    fn ensure_socket(&mut self) -> Result<&UdpSocket, String> {
        if self.socket.is_none() {
            let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("udp bind: {e}"))?;
            let _ = sock.set_write_timeout(Some(Duration::from_millis(200)));
            self.socket = Some(sock);
        }
        self.socket
            .as_ref()
            .ok_or_else(|| "udp socket missing".into())
    }
}

impl ClusterSink for JsonUdpLiveSink {
    fn name(&self) -> &'static str {
        "json_udp"
    }

    fn publish_cluster_json(&mut self, json: &str) -> Result<(), String> {
        let trimmed = json.trim();
        if trimmed.is_empty() {
            return Err("empty cluster json".into());
        }
        self.packets.push(trimmed.to_string());
        let addr = self.endpoint.socket_addr()?;
        let endpoint_label = self.endpoint.display();
        let send_result = {
            let sock = self.ensure_socket()?;
            sock.send_to(trimmed.as_bytes(), addr)
                .map_err(|e| format!("udp send {endpoint_label}: {e}"))
        };
        match send_result {
            Ok(_) => {
                self.last_error = None;
                Ok(())
            }
            Err(e) => {
                self.socket = None;
                self.last_error = Some(e.clone());
                Err(e)
            }
        }
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

/// Endpoints for live network sinks.
#[derive(Debug, Clone)]
pub struct ClusterSinkEndpoints {
    pub json_udp: ClusterEndpoint,
    pub kuksa: ClusterEndpoint,
}

impl Default for ClusterSinkEndpoints {
    fn default() -> Self {
        Self {
            json_udp: default_json_udp_endpoint(),
            kuksa: default_kuksa_endpoint(),
        }
    }
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
        self.boxed_with_endpoints(&ClusterSinkEndpoints::default())
    }

    pub fn boxed_with_endpoints(self, endpoints: &ClusterSinkEndpoints) -> Box<dyn ClusterSink> {
        match self {
            Self::Log => Box::new(LogSink::default()),
            Self::UsbNdjson => Box::new(NdjsonBufferSink::default()),
            Self::Kuksa => Box::new(KuksaLiveSink::new(endpoints.kuksa.clone())),
            Self::JsonUdp => Box::new(JsonUdpLiveSink::new(endpoints.json_udp.clone())),
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
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

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

    #[test]
    fn json_udp_live_sends_datagram() {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        sock.set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let mut sink = JsonUdpLiveSink::new(ClusterEndpoint::new("127.0.0.1", port));
        let payload = r#"{"schema":"navi.cluster.v1","route_active":false}"#;
        sink.publish_cluster_json(payload).unwrap();
        let mut buf = [0u8; 2048];
        let (n, _) = sock.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], payload.as_bytes());
        assert_eq!(sink.packets.len(), 1);
    }

    #[test]
    fn kuksa_live_writes_vss_batch_line() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let received = Arc::new(Mutex::new(String::new()));
        let received2 = Arc::clone(&received);
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            use std::io::{BufRead, BufReader};
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            *received2.lock().unwrap() = line;
        });
        let mut sink = KuksaLiveSink::new(ClusterEndpoint::new("127.0.0.1", port));
        sink.publish_cluster_json(
            r#"{"schema":"navi.cluster.v1","route_active":false,"overspeed":false,"warning":null}"#,
        )
        .unwrap();
        // Drop the TCP stream so the server read completes promptly if needed.
        drop(sink);
        for _ in 0..40 {
            if !received.lock().unwrap().is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let got = received.lock().unwrap().clone();
        assert!(got.contains("navi.vss.batch.v1"), "{got}");
        assert!(got.contains("Vehicle.Private.Navi.Route.Active"), "{got}");
        assert!(got.contains('\n'), "{got}");
    }
}
