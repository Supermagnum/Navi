//! GPX 1.1 importer (GPX 1.0 tolerated).
//!
//! [`parse_gpx`] turns GPX text into a [`GpxDocument`] holding tracks, routes and waypoints.
//! Odd or unsupported content is never dropped silently: it is described in
//! [`GpxDocument::unsupported`] as short human readable strings.
//!
//! Extension namespaces (outdooractive, gpxdata, osmand, gpxx, ...) are ignored without error:
//! only elements in the GPX namespace (or without a namespace) are interpreted, and only at the
//! positions defined by the GPX schema.
//!
//! The geometry helpers ([`simplify`], [`simplify_to_max_points`], [`sample_via_points`]) are
//! iterative, so tracks with tens of thousands of points cannot overflow the stack.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use roxmltree::{Document, Node};
use thiserror::Error;

/// Mean Earth radius in metres used by the haversine distance.
const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Plausible elevation range in metres. Values outside it are no-data sentinels written by some
/// exporters (for example -9999 or -32768) and are ignored, otherwise they ruin ascent figures.
const ELE_RANGE_M: std::ops::RangeInclusive<f64> = -2_000.0..=20_000.0;

/// Elevation changes smaller than this are treated as noise by [`ascent_m`].
const ASCENT_THRESHOLD_M: f64 = 3.0;

/// Hard failures: nothing usable can be returned.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GpxImportError {
    /// The text is not well formed XML.
    #[error("invalid XML: {0}")]
    InvalidXml(String),
    /// Well formed XML whose root element is not `<gpx>`.
    #[error("not a GPX file: root element is <{root}>, expected <gpx>")]
    NotGpx { root: String },
    /// A GPX file without any usable track, route or waypoint.
    #[error("the GPX file contains no usable track, route or waypoint")]
    Empty,
    /// The input is a zip or gzip archive; unpack it first.
    #[error("compressed input (zip or gzip) is not supported; unpack the GPX file first")]
    Compressed,
    /// The input bytes are not valid UTF-8.
    #[error("the GPX file is not valid UTF-8")]
    NotUtf8,
}

/// One track, route or way point position.
#[derive(Debug, Clone, PartialEq)]
pub struct GpxPoint {
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    /// ISO 8601 text exactly as found in the file (trimmed).
    pub time: Option<String>,
}

impl GpxPoint {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            lat,
            lon,
            ele: None,
            time: None,
        }
    }
}

/// A `<trk>`: one or more segments of points.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GpxTrack {
    pub name: Option<String>,
    pub segments: Vec<Vec<GpxPoint>>,
}

impl GpxTrack {
    /// Total number of points over all segments.
    pub fn point_count(&self) -> usize {
        self.segments.iter().map(Vec::len).sum()
    }

    /// Sum of the haversine length of each segment. Gaps between segments are not counted.
    pub fn length_m(&self) -> f64 {
        self.segments.iter().map(|s| path_length_m(s)).sum()
    }

    /// Ascent over all segments (see [`ascent_m`]); `None` when no segment carries elevation.
    pub fn ascent_m(&self) -> Option<f64> {
        let mut total = 0.0;
        let mut any = false;
        for seg in &self.segments {
            if let Some(a) = ascent_m(seg) {
                total += a;
                any = true;
            }
        }
        any.then_some(total)
    }

    /// All segments joined into one polyline (cloned).
    pub fn flatten(&self) -> Vec<GpxPoint> {
        let mut out = Vec::with_capacity(self.point_count());
        for seg in &self.segments {
            out.extend(seg.iter().cloned());
        }
        out
    }
}

/// A `<rte>` with its `<rtept>` planning points.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GpxRoute {
    pub name: Option<String>,
    pub points: Vec<GpxPoint>,
}

impl GpxRoute {
    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    pub fn length_m(&self) -> f64 {
        path_length_m(&self.points)
    }

    pub fn ascent_m(&self) -> Option<f64> {
        ascent_m(&self.points)
    }
}

/// A `<wpt>`.
#[derive(Debug, Clone, PartialEq)]
pub struct GpxWaypoint {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    /// `<sym>` text (symbol name).
    pub symbol: Option<String>,
}

/// Geographic bounding box in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpxBounds {
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}

impl GpxBounds {
    /// `[min_lat, min_lon, max_lat, max_lon]`.
    pub fn as_array(&self) -> [f64; 4] {
        [self.min_lat, self.min_lon, self.max_lat, self.max_lon]
    }

    fn include(&mut self, lat: f64, lon: f64) {
        self.min_lat = self.min_lat.min(lat);
        self.max_lat = self.max_lat.max(lat);
        self.min_lon = self.min_lon.min(lon);
        self.max_lon = self.max_lon.max(lon);
    }
}

/// A parsed GPX file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GpxDocument {
    /// `<metadata><name>` (GPX 1.1) or `<gpx><name>` (GPX 1.0).
    pub name: Option<String>,
    /// `version` attribute of the root element, as written.
    pub version: Option<String>,
    pub tracks: Vec<GpxTrack>,
    pub routes: Vec<GpxRoute>,
    pub waypoints: Vec<GpxWaypoint>,
    /// Short human readable notes about unsupported, skipped or odd content.
    pub unsupported: Vec<String>,
}

impl GpxDocument {
    pub fn track_point_count(&self) -> usize {
        self.tracks.iter().map(GpxTrack::point_count).sum()
    }

    pub fn route_point_count(&self) -> usize {
        self.routes.iter().map(GpxRoute::point_count).sum()
    }

    pub fn waypoint_count(&self) -> usize {
        self.waypoints.len()
    }

    /// Track points + route points + waypoints.
    pub fn total_point_count(&self) -> usize {
        self.track_point_count() + self.route_point_count() + self.waypoint_count()
    }

    /// Bounding box over every track point, route point and waypoint.
    pub fn bounds(&self) -> Option<GpxBounds> {
        let mut b: Option<GpxBounds> = None;
        let mut add = |lat: f64, lon: f64| match b.as_mut() {
            Some(bb) => bb.include(lat, lon),
            None => {
                b = Some(GpxBounds {
                    min_lat: lat,
                    min_lon: lon,
                    max_lat: lat,
                    max_lon: lon,
                })
            }
        };
        for t in &self.tracks {
            for p in t.segments.iter().flatten() {
                add(p.lat, p.lon);
            }
        }
        for r in &self.routes {
            for p in &r.points {
                add(p.lat, p.lon);
            }
        }
        for w in &self.waypoints {
            add(w.lat, w.lon);
        }
        b
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------------------------

/// Parse GPX text. See the module docs for what is supported and reported.
pub fn parse_gpx(text: &str) -> Result<GpxDocument, GpxImportError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).trim_start();
    if text.is_empty() {
        return Err(GpxImportError::Empty);
    }
    // Compressed bytes decoded lossily as text start with these code points.
    if text.starts_with("PK\u{3}\u{4}") || text.starts_with('\u{1f}') {
        return Err(GpxImportError::Compressed);
    }

    let xml = Document::parse(text).map_err(|e| GpxImportError::InvalidXml(e.to_string()))?;
    let root = xml.root_element();
    if root.tag_name().name() != "gpx" {
        return Err(GpxImportError::NotGpx {
            root: root.tag_name().name().to_string(),
        });
    }

    let mut p = Parser::new(root.tag_name().namespace());
    let mut doc = GpxDocument {
        version: root.attribute("version").map(|v| v.trim().to_string()),
        ..GpxDocument::default()
    };

    if let Some(enc) = declared_encoding(text) {
        if !enc.eq_ignore_ascii_case("utf-8") && !enc.eq_ignore_ascii_case("utf8") {
            p.notes.push(format!(
                "declared XML encoding '{enc}' ignored; input treated as UTF-8"
            ));
        }
    }
    match doc.version.as_deref() {
        Some("1.0") | Some("1.1") => {}
        Some(v) => p.notes.push(format!(
            "GPX version '{v}' is not supported (expected 1.0 or 1.1); parsed as 1.1"
        )),
        None => p
            .notes
            .push("GPX version attribute missing; assumed 1.1".to_string()),
    }
    if let Some(ns) = p.ns {
        if ns != NS_GPX_10 && ns != NS_GPX_11 {
            p.notes
                .push(format!("unrecognised GPX namespace '{ns}'; parsed anyway"));
        }
    }

    let mut legacy_name: Option<String> = None;
    let ns = p.ns;
    for child in root.children().filter(|n| in_gpx(ns, n)) {
        match child.tag_name().name() {
            "metadata" => {
                if let Some(name) = p.child_text(&child, "name") {
                    doc.name = Some(name);
                }
                p.scan_extensions(&child);
            }
            "name" => legacy_name = node_text(&child),
            "extensions" => p.scan_extension_node(&child),
            "trk" => {
                if let Some(t) = p.parse_track(&child, doc.tracks.len() + 1) {
                    doc.tracks.push(t);
                }
            }
            "rte" => {
                if let Some(r) = p.parse_route(&child, doc.routes.len() + 1) {
                    doc.routes.push(r);
                }
            }
            "wpt" => {
                if let Some(w) = p.parse_waypoint(&child) {
                    doc.waypoints.push(w);
                }
            }
            _ => {}
        }
    }
    if doc.name.is_none() {
        doc.name = legacy_name;
    }

    doc.unsupported = p.finish();

    if doc.tracks.is_empty() && doc.routes.is_empty() && doc.waypoints.is_empty() {
        return Err(GpxImportError::Empty);
    }
    Ok(doc)
}

/// Parse GPX bytes: detects zip/gzip archives, strips a UTF-8 BOM and validates UTF-8.
pub fn parse_gpx_bytes(bytes: &[u8]) -> Result<GpxDocument, GpxImportError> {
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(&[0x1f, 0x8b]) {
        return Err(GpxImportError::Compressed);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| GpxImportError::NotUtf8)?;
    parse_gpx(text)
}

const NS_GPX_10: &str = "http://www.topografix.com/GPX/1/0";
const NS_GPX_11: &str = "http://www.topografix.com/GPX/1/1";

/// `encoding="..."` of the XML declaration, when present.
fn declared_encoding(text: &str) -> Option<&str> {
    if !text.starts_with("<?xml") {
        return None;
    }
    let decl = &text[..text.find("?>")?];
    let idx = decl.find("encoding")?;
    let rest = decl[idx + "encoding".len()..]
        .trim_start()
        .strip_prefix('=')?;
    let rest = rest.trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &rest[1..];
    Some(&rest[..rest.find(quote)?])
}

/// Why a coordinate pair was rejected.
enum Skip {
    Invalid,
    Range,
}

/// Skip counters for one kind of point.
#[derive(Default)]
struct SkipCount {
    invalid: usize,
    range: usize,
}

impl SkipCount {
    fn add(&mut self, s: Skip) {
        match s {
            Skip::Invalid => self.invalid += 1,
            Skip::Range => self.range += 1,
        }
    }

    fn report(&self, what: &str, out: &mut Vec<String>) {
        if self.invalid > 0 {
            out.push(format!(
                "{} {what} skipped: missing or invalid lat/lon",
                self.invalid
            ));
        }
        if self.range > 0 {
            out.push(format!(
                "{} {what} skipped: lat/lon out of range",
                self.range
            ));
        }
    }
}

struct Parser<'a> {
    ns: Option<&'a str>,
    /// Notes known up front (version, encoding, namespace).
    notes: Vec<String>,
    track_skips: SkipCount,
    route_skips: SkipCount,
    waypoint_skips: SkipCount,
    item_notes: Vec<String>,
    bad_ele: usize,
    single_point_segments: usize,
    empty_segments: usize,
    osmand_route_seen: bool,
    garmin_rpt_seen: bool,
}

impl<'a> Parser<'a> {
    fn new(ns: Option<&'a str>) -> Self {
        Self {
            ns,
            notes: Vec::new(),
            track_skips: SkipCount::default(),
            route_skips: SkipCount::default(),
            waypoint_skips: SkipCount::default(),
            item_notes: Vec::new(),
            bad_ele: 0,
            single_point_segments: 0,
            empty_segments: 0,
            osmand_route_seen: false,
            garmin_rpt_seen: false,
        }
    }

    /// True for elements in the GPX namespace of the root (or without a namespace).
    fn is_gpx_element(&self, n: &Node) -> bool {
        in_gpx(self.ns, n)
    }

    fn child<'n, 'i>(&self, node: &Node<'n, 'i>, name: &str) -> Option<Node<'n, 'i>> {
        node.children()
            .find(|c| self.is_gpx_element(c) && c.tag_name().name() == name)
    }

    fn child_text(&self, node: &Node, name: &str) -> Option<String> {
        self.child(node, name).and_then(|c| node_text(&c))
    }

    fn read_coords(&self, node: &Node) -> Result<(f64, f64), Skip> {
        let lat = parse_finite(node.attribute("lat")).ok_or(Skip::Invalid)?;
        let lon = parse_finite(node.attribute("lon")).ok_or(Skip::Invalid)?;
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(Skip::Range);
        }
        Ok((lat, lon))
    }

    fn read_ele(&mut self, node: &Node) -> Option<f64> {
        let text = self.child_text(node, "ele")?;
        match parse_finite(Some(&text)).filter(|v| ELE_RANGE_M.contains(v)) {
            Some(v) => Some(v),
            None => {
                self.bad_ele += 1;
                None
            }
        }
    }

    /// Parse a `trkpt` / `rtept`. `None` when skipped (counted in `skips`).
    fn parse_point(&mut self, node: &Node, kind: PointKind) -> Option<GpxPoint> {
        match self.read_coords(node) {
            Ok((lat, lon)) => Some(GpxPoint {
                lat,
                lon,
                ele: self.read_ele(node),
                time: self.child_text(node, "time"),
            }),
            Err(s) => {
                match kind {
                    PointKind::Track => self.track_skips.add(s),
                    PointKind::Route => self.route_skips.add(s),
                }
                None
            }
        }
    }

    fn parse_track(&mut self, trk: &Node, index: usize) -> Option<GpxTrack> {
        let name = self.child_text(trk, "name");
        let mut segments: Vec<Vec<GpxPoint>> = Vec::new();
        let mut empty = 0usize;
        let mut single = 0usize;
        let ns = self.ns;
        for child in trk.children().filter(|n| in_gpx(ns, n)) {
            match child.tag_name().name() {
                "trkseg" => {
                    let mut pts = Vec::new();
                    for pt in child
                        .children()
                        .filter(|n| in_gpx(ns, n) && n.tag_name().name() == "trkpt")
                    {
                        if let Some(p) = self.parse_point(&pt, PointKind::Track) {
                            pts.push(p);
                        }
                    }
                    self.scan_extensions(&child);
                    match pts.len() {
                        0 => empty += 1,
                        1 => {
                            single += 1;
                            segments.push(pts);
                        }
                        _ => segments.push(pts),
                    }
                }
                "extensions" => self.scan_extension_node(&child),
                _ => {}
            }
        }
        if segments.is_empty() {
            self.item_notes.push(format!(
                "track {} has no points and was skipped",
                label(name.as_deref(), index)
            ));
            return None;
        }
        self.empty_segments += empty;
        self.single_point_segments += single;
        Some(GpxTrack { name, segments })
    }

    fn parse_route(&mut self, rte: &Node, index: usize) -> Option<GpxRoute> {
        let name = self.child_text(rte, "name");
        let mut points = Vec::new();
        let ns = self.ns;
        for child in rte.children().filter(|n| in_gpx(ns, n)) {
            match child.tag_name().name() {
                "rtept" => {
                    if let Some(p) = self.parse_point(&child, PointKind::Route) {
                        points.push(p);
                    }
                    self.scan_extensions(&child);
                }
                "extensions" => self.scan_extension_node(&child),
                _ => {}
            }
        }
        if points.is_empty() {
            self.item_notes.push(format!(
                "route {} has no points and was skipped",
                label(name.as_deref(), index)
            ));
            return None;
        }
        Some(GpxRoute { name, points })
    }

    fn parse_waypoint(&mut self, wpt: &Node) -> Option<GpxWaypoint> {
        match self.read_coords(wpt) {
            Ok((lat, lon)) => Some(GpxWaypoint {
                name: self.child_text(wpt, "name"),
                desc: self.child_text(wpt, "desc"),
                lat,
                lon,
                ele: self.read_ele(wpt),
                symbol: self.child_text(wpt, "sym"),
            }),
            Err(s) => {
                self.waypoint_skips.add(s);
                None
            }
        }
    }

    /// Inspect the `<extensions>` child of `node`, if any.
    fn scan_extensions(&mut self, node: &Node) {
        if let Some(ext) = self.child(node, "extensions") {
            self.scan_extension_node(&ext);
        }
    }

    /// Look for route geometry carried in extensions. The content is otherwise ignored.
    fn scan_extension_node(&mut self, ext: &Node) {
        if self.osmand_route_seen && self.garmin_rpt_seen {
            return;
        }
        for n in ext.descendants().filter(Node::is_element) {
            let name = n.tag_name().name();
            if name.eq_ignore_ascii_case("route") {
                let osmand_ns = n
                    .tag_name()
                    .namespace()
                    .is_some_and(|ns| ns.to_ascii_lowercase().contains("osmand"));
                let has_segments = n
                    .descendants()
                    .any(|d| d.is_element() && d.tag_name().name().eq_ignore_ascii_case("segment"));
                if osmand_ns || has_segments {
                    self.osmand_route_seen = true;
                }
            } else if name == "RoutePointExtension" || name == "rpt" {
                self.garmin_rpt_seen = true;
            }
        }
    }

    /// Combine all notes into the final `unsupported` list.
    fn finish(mut self) -> Vec<String> {
        let mut out = std::mem::take(&mut self.notes);
        self.track_skips.report("track points", &mut out);
        self.route_skips.report("route points", &mut out);
        self.waypoint_skips.report("waypoints", &mut out);
        out.append(&mut self.item_notes);
        if self.empty_segments > 0 {
            out.push(format!(
                "{} empty track segment(s) ignored",
                self.empty_segments
            ));
        }
        if self.single_point_segments > 0 {
            out.push(format!(
                "{} track segment(s) contain a single point (kept, cannot form a line)",
                self.single_point_segments
            ));
        }
        if self.bad_ele > 0 {
            out.push(format!(
                "{} point(s) with an invalid <ele> value (non-numeric or out of range); elevation ignored",
                self.bad_ele
            ));
        }
        if self.osmand_route_seen {
            out.push(
                "route geometry in <extensions> (OsmAnd route data) is ignored; \
                 only track/route points are used"
                    .to_string(),
            );
        }
        if self.garmin_rpt_seen {
            out.push(
                "route shaping points in <extensions> (Garmin RoutePointExtension) are ignored"
                    .to_string(),
            );
        }
        out
    }
}

/// True for elements in the GPX namespace `ns` (or without a namespace).
fn in_gpx(ns: Option<&str>, n: &Node) -> bool {
    n.is_element()
        && match n.tag_name().namespace() {
            None => true,
            Some(x) => Some(x) == ns,
        }
}

#[derive(Clone, Copy)]
enum PointKind {
    Track,
    Route,
}

fn label(name: Option<&str>, index: usize) -> String {
    match name {
        Some(n) => format!("'{n}'"),
        None => format!("#{index}"),
    }
}

fn parse_finite(s: Option<&str>) -> Option<f64> {
    let v: f64 = s?.trim().parse().ok()?;
    v.is_finite().then_some(v)
}

/// Concatenated, trimmed text content of an element; `None` when empty.
fn node_text(n: &Node) -> Option<String> {
    let mut buf = String::new();
    for c in n.children().filter(Node::is_text) {
        if let Some(t) = c.text() {
            buf.push_str(t);
        }
    }
    let t = buf.trim();
    (!t.is_empty()).then(|| t.to_string())
}

// ---------------------------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------------------------

/// Great circle distance in metres.
fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().min(1.0).asin()
}

/// Haversine length of a polyline in metres.
pub fn path_length_m(points: &[GpxPoint]) -> f64 {
    points
        .windows(2)
        .map(|w| haversine_m(w[0].lat, w[0].lon, w[1].lat, w[1].lon))
        .sum()
}

/// Total climb in metres from `<ele>` values.
///
/// Changes smaller than 3 m are ignored to suppress GPS/barometer noise. Points without
/// elevation are skipped. `None` when fewer than two points carry elevation.
pub fn ascent_m(points: &[GpxPoint]) -> Option<f64> {
    let mut reference: Option<f64> = None;
    let mut seen = 0usize;
    let mut ascent = 0.0;
    for ele in points.iter().filter_map(|p| p.ele) {
        seen += 1;
        match reference {
            None => reference = Some(ele),
            Some(r) if ele >= r + ASCENT_THRESHOLD_M => {
                ascent += ele - r;
                reference = Some(ele);
            }
            Some(r) if ele <= r - ASCENT_THRESHOLD_M => reference = Some(ele),
            Some(_) => {}
        }
    }
    (seen >= 2).then_some(ascent)
}

/// Local equirectangular projection to metres, good enough for point to line distances over a
/// single track.
fn project_m(points: &[GpxPoint]) -> Vec<(f64, f64)> {
    if points.is_empty() {
        return Vec::new();
    }
    let mean_lat = points.iter().map(|p| p.lat).sum::<f64>() / points.len() as f64;
    let k = mean_lat.to_radians().cos();
    points
        .iter()
        .map(|p| {
            (
                p.lon.to_radians() * EARTH_RADIUS_M * k,
                p.lat.to_radians() * EARTH_RADIUS_M,
            )
        })
        .collect()
}

/// Squared distance from `p` to the segment `a`-`b`.
fn seg_dist2(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    let (ex, ey) = (p.0 - cx, p.1 - cy);
    ex * ex + ey * ey
}

/// Index and squared distance of the point between `s` and `e` (exclusive) farthest from the
/// chord `s`-`e`.
fn farthest(xy: &[(f64, f64)], s: usize, e: usize) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (i, &p) in xy.iter().enumerate().take(e).skip(s + 1) {
        let d = seg_dist2(p, xy[s], xy[e]);
        if best.is_none_or(|(_, bd)| d > bd) {
            best = Some((i, d));
        }
    }
    best
}

/// Douglas-Peucker simplification with a distance tolerance in metres.
///
/// Iterative (explicit stack). First and last point are always kept. A non-positive tolerance
/// removes only points that lie exactly on the line between their neighbours.
pub fn simplify(points: &[GpxPoint], tolerance_m: f64) -> Vec<GpxPoint> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let tol = if tolerance_m.is_finite() && tolerance_m > 0.0 {
        tolerance_m
    } else {
        0.0
    };
    let tol2 = tol * tol;
    let xy = project_m(points);
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((s, e)) = stack.pop() {
        if e <= s + 1 {
            continue;
        }
        if let Some((idx, d2)) = farthest(&xy, s, e) {
            if d2 > tol2 {
                keep[idx] = true;
                stack.push((s, idx));
                stack.push((idx, e));
            }
        }
    }
    points
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| p.clone())
        .collect()
}

/// Douglas-Peucker with an automatic tolerance: keeps the `max_points` most significant points
/// (first and last always included). Equivalent to [`simplify`] with the smallest tolerance that
/// yields at most `max_points` points. Fewer points are returned when the remaining ones lie on
/// the line between their neighbours.
pub fn simplify_to_max_points(points: &[GpxPoint], max_points: usize) -> Vec<GpxPoint> {
    let max_points = max_points.max(2);
    if points.len() <= max_points {
        return points.to_vec();
    }
    let xy = project_m(points);
    let last = points.len() - 1;
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[last] = true;
    let mut kept = 2usize;

    // Max-heap on squared distance; non-negative f64 bit patterns order like the values.
    let mut heap: BinaryHeap<(u64, Reverse<usize>, usize, usize)> = BinaryHeap::new();
    let push = |heap: &mut BinaryHeap<_>, s: usize, e: usize| {
        if let Some((idx, d2)) = farthest(&xy, s, e) {
            heap.push((d2.to_bits(), Reverse(idx), s, e));
        }
    };
    push(&mut heap, 0, last);
    while kept < max_points {
        let Some((bits, Reverse(idx), s, e)) = heap.pop() else {
            break;
        };
        if f64::from_bits(bits) <= 0.0 {
            break;
        }
        keep[idx] = true;
        kept += 1;
        push(&mut heap, s, idx);
        push(&mut heap, idx, e);
    }
    points
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| p.clone())
        .collect()
}

/// Evenly spaced points along a path, used to route along a track.
///
/// Returns the first point, up to `max_vias` intermediate points, and the last point. Intermediate
/// points are interpolated on the path at equal distances; the spacing is never below
/// `min_spacing_m` (fewer intermediates are returned for short paths). Interpolated points carry
/// an elevation only when both neighbours have one, and never a time.
pub fn sample_via_points(
    points: &[GpxPoint],
    max_vias: usize,
    min_spacing_m: f64,
) -> Vec<GpxPoint> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let first = points[0].clone();
    let last = points[points.len() - 1].clone();

    let mut cum = Vec::with_capacity(points.len());
    let mut total = 0.0;
    cum.push(0.0);
    for w in points.windows(2) {
        total += haversine_m(w[0].lat, w[0].lon, w[1].lat, w[1].lon);
        cum.push(total);
    }

    let by_spacing = if min_spacing_m.is_finite() && min_spacing_m > 0.0 {
        ((total / min_spacing_m).floor() as usize).saturating_sub(1)
    } else {
        usize::MAX
    };
    let n = max_vias.min(by_spacing);
    if n == 0 || total <= 0.0 {
        return vec![first, last];
    }

    let step = total / (n + 1) as f64;
    let mut out = Vec::with_capacity(n + 2);
    out.push(first);
    let mut seg = 1usize;
    for k in 1..=n {
        let target = step * k as f64;
        while seg < cum.len() - 1 && cum[seg] < target {
            seg += 1;
        }
        let (a, b) = (&points[seg - 1], &points[seg]);
        let span = cum[seg] - cum[seg - 1];
        let f = if span > 0.0 {
            ((target - cum[seg - 1]) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out.push(GpxPoint {
            lat: a.lat + (b.lat - a.lat) * f,
            lon: a.lon + (b.lon - a.lon) * f,
            ele: match (a.ele, b.ele) {
                (Some(x), Some(y)) => Some(x + (y - x) * f),
                _ => None,
            },
            time: None,
        });
    }
    out.push(last);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>"#;

    fn gpx11(body: &str) -> String {
        format!(
            r#"{HEAD}<gpx version="1.1" creator="t" xmlns="http://www.topografix.com/GPX/1/1">{body}</gpx>"#
        )
    }

    fn line(n: usize) -> Vec<GpxPoint> {
        (0..n)
            .map(|i| GpxPoint::new(60.0 + i as f64 * 0.001, 10.0))
            .collect()
    }

    #[test]
    fn parses_track_with_two_segments() {
        let doc = parse_gpx(&gpx11(
            r#"<metadata><name>Hike</name></metadata>
            <trk><name>T1</name>
              <trkseg><trkpt lat="60.0" lon="10.0"><ele>100</ele><time>2026-01-01T10:00:00Z</time></trkpt>
                      <trkpt lat="60.1" lon="10.0"><ele>110.5</ele></trkpt></trkseg>
              <trkseg><trkpt lat="60.2" lon="10.0"/><trkpt lat="60.3" lon="10.0"/></trkseg>
            </trk>"#,
        ))
        .unwrap();
        assert_eq!(doc.name.as_deref(), Some("Hike"));
        assert_eq!(doc.version.as_deref(), Some("1.1"));
        assert_eq!(doc.tracks.len(), 1);
        let t = &doc.tracks[0];
        assert_eq!(t.segments.len(), 2);
        assert_eq!(t.point_count(), 4);
        assert_eq!(t.segments[0][0].ele, Some(100.0));
        assert_eq!(
            t.segments[0][0].time.as_deref(),
            Some("2026-01-01T10:00:00Z")
        );
        assert_eq!(t.segments[0][1].ele, Some(110.5));
        assert!(doc.unsupported.is_empty(), "{:?}", doc.unsupported);
        // Two 0.1 degree legs, the gap between the segments is not counted.
        let expect = 2.0 * 0.1_f64.to_radians() * EARTH_RADIUS_M;
        assert!((t.length_m() - expect).abs() < 1.0);
        assert_eq!(doc.total_point_count(), 4);
        let b = doc.bounds().unwrap();
        assert_eq!(b.as_array(), [60.0, 10.0, 60.3, 10.0]);
    }

    #[test]
    fn parses_route_and_waypoints() {
        let doc = parse_gpx(&gpx11(
            r#"<wpt lat="59.9" lon="10.7"><name>Caf&#233; &amp; Bar</name><desc>Line1
Line2</desc><ele>12</ele><sym>Restaurant</sym></wpt>
            <rte><name>R</name><rtept lat="59.9" lon="10.7"><name>A</name></rtept>
              <rtept lat="60.0" lon="10.8"/></rte>"#,
        ))
        .unwrap();
        assert_eq!(doc.waypoints.len(), 1);
        let w = &doc.waypoints[0];
        assert_eq!(w.name.as_deref(), Some("Caf\u{e9} & Bar"));
        assert_eq!(w.desc.as_deref(), Some("Line1\nLine2"));
        assert_eq!(w.symbol.as_deref(), Some("Restaurant"));
        assert_eq!(w.ele, Some(12.0));
        assert_eq!(doc.routes.len(), 1);
        assert_eq!(doc.routes[0].points.len(), 2);
        assert_eq!(doc.routes[0].points[0].lat, 59.9);
        assert_eq!(doc.total_point_count(), 3);
        assert!(doc.routes[0].length_m() > 10_000.0);
    }

    #[test]
    fn unicode_and_cdata_names() {
        let doc = parse_gpx(&gpx11(
            r#"<wpt lat="62.0" lon="12.0"><name><![CDATA[Sota S&ter]]></name></wpt>
               <wpt lat="62.1" lon="12.0"><name>Sota S&#230;ter &#8211; hytte</name></wpt>"#,
        ))
        .unwrap();
        assert_eq!(doc.waypoints[0].name.as_deref(), Some("Sota S&ter"));
        assert_eq!(
            doc.waypoints[1].name.as_deref(),
            Some("Sota S\u{e6}ter \u{2013} hytte")
        );
    }

    #[test]
    fn bom_and_leading_whitespace_tolerated() {
        let text = format!(
            "\u{feff}\n  {}",
            gpx11(r#"<wpt lat="1" lon="2"><name>x</name></wpt>"#)
        );
        let doc = parse_gpx(&text).unwrap();
        assert_eq!(doc.waypoints.len(), 1);
        let bytes = format!(
            "\u{feff}{}",
            gpx11(r#"<wpt lat="1" lon="2"><name>x</name></wpt>"#)
        );
        assert!(parse_gpx_bytes(bytes.as_bytes()).is_ok());
    }

    #[test]
    fn foreign_namespaces_are_ignored_silently() {
        let text = format!(
            r#"{HEAD}<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"
              xmlns:gpxdata="http://www.cluetrust.com/XML/GPXDATA/1/0"
              xmlns:oa="http://www.outdooractive.com/GPX/Extensions/1">
              <metadata><name>M</name><author><name>Someone</name></author>
                <extensions><oa:oaCategory>x</oa:oaCategory></extensions></metadata>
              <trk><name>T</name><extensions><oa:thing><oa:name>wrong</oa:name></oa:thing></extensions>
                <trkseg><trkpt lat="60" lon="10"><ele>1</ele><extensions><gpxdata:hr>90</gpxdata:hr></extensions></trkpt>
                        <trkpt lat="60.1" lon="10"><oa:ele>9999</oa:ele></trkpt></trkseg></trk>
            </gpx>"#
        );
        let doc = parse_gpx(&text).unwrap();
        assert_eq!(doc.name.as_deref(), Some("M"));
        assert_eq!(doc.tracks[0].name.as_deref(), Some("T"));
        assert_eq!(doc.tracks[0].segments[0][1].ele, None);
        assert!(doc.unsupported.is_empty(), "{:?}", doc.unsupported);
    }

    #[test]
    fn osmand_route_extension_is_noted_once() {
        let text = format!(
            r#"{HEAD}<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"
              xmlns:osmand="https://osmand.net">
              <trk><trkseg><trkpt lat="60" lon="10"/><trkpt lat="60.1" lon="10"/></trkseg>
                <extensions><osmand:route><osmand:segment id="1"/></osmand:route></extensions></trk>
              <extensions><osmand:route><osmand:segment id="2"/></osmand:route></extensions>
            </gpx>"#
        );
        let doc = parse_gpx(&text).unwrap();
        let n = doc
            .unsupported
            .iter()
            .filter(|s| s.contains("OsmAnd"))
            .count();
        assert_eq!(n, 1, "{:?}", doc.unsupported);
    }

    #[test]
    fn garmin_route_point_extension_is_noted() {
        let text = format!(
            r#"{HEAD}<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"
              xmlns:gpxx="http://www.garmin.com/xmlschemas/GpxExtensions/v3">
              <rte><rtept lat="60" lon="10"><extensions><gpxx:RoutePointExtension>
                <gpxx:rpt lat="60.01" lon="10.01"/></gpxx:RoutePointExtension></extensions></rtept></rte>
            </gpx>"#
        );
        let doc = parse_gpx(&text).unwrap();
        assert!(doc.unsupported.iter().any(|s| s.contains("Garmin")));
    }

    #[test]
    fn invalid_coordinates_are_skipped_and_counted() {
        let doc = parse_gpx(&gpx11(
            r#"<trk><name>T</name><trkseg>
                <trkpt lat="60" lon="10"/>
                <trkpt lat="abc" lon="10"/>
                <trkpt lon="10"/>
                <trkpt lat="NaN" lon="10"/>
                <trkpt lat="91" lon="10"/>
                <trkpt lat="60" lon="200"/>
                <trkpt lat="60.1" lon="10"><ele>x</ele></trkpt>
                <trkpt lat="60.2" lon="10"><ele>-9999</ele></trkpt>
              </trkseg></trk>
              <wpt lat="95" lon="0"/><wpt lon="0"/><wpt lat="1" lon="2"/>"#,
        ))
        .unwrap();
        assert_eq!(doc.track_point_count(), 3);
        assert_eq!(doc.waypoint_count(), 1);
        assert_eq!(doc.tracks[0].ascent_m(), None);
        let u = doc.unsupported.join(" | ");
        assert!(
            u.contains("3 track points skipped: missing or invalid lat/lon"),
            "{u}"
        );
        assert!(
            u.contains("2 track points skipped: lat/lon out of range"),
            "{u}"
        );
        assert!(
            u.contains("1 waypoints skipped: missing or invalid lat/lon"),
            "{u}"
        );
        assert!(
            u.contains("1 waypoints skipped: lat/lon out of range"),
            "{u}"
        );
        assert!(u.contains("2 point(s) with an invalid <ele>"), "{u}");
    }

    #[test]
    fn empty_and_single_point_segments_reported() {
        let doc = parse_gpx(&gpx11(
            r#"<trk><name>Empty</name><trkseg/></trk>
               <trk><name>Ok</name>
                 <trkseg><trkpt lat="60" lon="10"/></trkseg>
                 <trkseg/>
                 <trkseg><trkpt lat="60" lon="10"/><trkpt lat="60.1" lon="10"/></trkseg></trk>
               <rte><name>NoPts</name></rte>
               <wpt lat="1" lon="1"/>"#,
        ))
        .unwrap();
        assert_eq!(doc.tracks.len(), 1);
        assert_eq!(doc.tracks[0].segments.len(), 2);
        let u = doc.unsupported.join(" | ");
        assert!(u.contains("track 'Empty' has no points"), "{u}");
        assert!(u.contains("route 'NoPts' has no points"), "{u}");
        assert!(u.contains("1 empty track segment(s)"), "{u}");
        assert!(
            u.contains("1 track segment(s) contain a single point"),
            "{u}"
        );
    }

    #[test]
    fn gpx_10_is_tolerated() {
        let text = format!(
            r#"{HEAD}<gpx version="1.0" creator="old" xmlns="http://www.topografix.com/GPX/1/0">
            <name>Old</name>
            <wpt lat="60" lon="10"><name>W</name><sym>Flag</sym></wpt>
            <trk><trkseg><trkpt lat="60" lon="10"/><trkpt lat="60.1" lon="10"/></trkseg></trk></gpx>"#
        );
        let doc = parse_gpx(&text).unwrap();
        assert_eq!(doc.version.as_deref(), Some("1.0"));
        assert_eq!(doc.name.as_deref(), Some("Old"));
        assert_eq!(doc.tracks.len(), 1);
        assert_eq!(doc.waypoints.len(), 1);
        assert!(doc.unsupported.is_empty(), "{:?}", doc.unsupported);
    }

    #[test]
    fn other_version_is_reported_but_parsed() {
        let text = format!(
            r#"{HEAD}<gpx version="2.0" xmlns="http://www.topografix.com/GPX/1/1">
            <wpt lat="60" lon="10"/></gpx>"#
        );
        let doc = parse_gpx(&text).unwrap();
        assert!(doc.unsupported.iter().any(|s| s.contains("version '2.0'")));
        let text = format!(
            r#"{HEAD}<gpx xmlns="http://www.topografix.com/GPX/1/1"><wpt lat="1" lon="1"/></gpx>"#
        );
        let doc = parse_gpx(&text).unwrap();
        assert!(doc
            .unsupported
            .iter()
            .any(|s| s.contains("version attribute missing")));
    }

    #[test]
    fn declared_non_utf8_encoding_is_reported() {
        let text = r#"<?xml version="1.0" encoding="ISO-8859-1"?>
            <gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><wpt lat="1" lon="1"/></gpx>"#;
        let doc = parse_gpx(text).unwrap();
        assert!(doc.unsupported.iter().any(|s| s.contains("ISO-8859-1")));
    }

    #[test]
    fn hard_errors() {
        assert_eq!(parse_gpx(""), Err(GpxImportError::Empty));
        assert_eq!(parse_gpx("\u{feff}  \n"), Err(GpxImportError::Empty));
        assert!(matches!(
            parse_gpx("<gpx><trk>"),
            Err(GpxImportError::InvalidXml(_))
        ));
        assert!(matches!(
            parse_gpx("not xml at all"),
            Err(GpxImportError::InvalidXml(_))
        ));
        assert_eq!(
            parse_gpx(r#"<?xml version="1.0"?><kml xmlns="http://www.opengis.net/kml/2.2"/>"#),
            Err(GpxImportError::NotGpx {
                root: "kml".to_string()
            })
        );
        assert_eq!(
            parse_gpx(&gpx11("<metadata><name>x</name></metadata>")),
            Err(GpxImportError::Empty)
        );
        assert_eq!(
            parse_gpx("PK\u{3}\u{4}abc"),
            Err(GpxImportError::Compressed)
        );
        assert_eq!(
            parse_gpx_bytes(&[0x1f, 0x8b, 8, 0]),
            Err(GpxImportError::Compressed)
        );
        assert_eq!(
            parse_gpx_bytes(b"PK\x03\x04\x14\x00"),
            Err(GpxImportError::Compressed)
        );
        assert_eq!(
            parse_gpx_bytes(&[0xff, 0xfe, 0x00]),
            Err(GpxImportError::NotUtf8)
        );
        for e in [
            GpxImportError::Empty,
            GpxImportError::Compressed,
            GpxImportError::NotUtf8,
            GpxImportError::NotGpx { root: "x".into() },
            GpxImportError::InvalidXml("boom".into()),
        ] {
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn all_invalid_points_is_empty() {
        assert_eq!(
            parse_gpx(&gpx11(
                r#"<trk><trkseg><trkpt lat="x" lon="y"/></trkseg></trk>"#
            )),
            Err(GpxImportError::Empty)
        );
    }

    #[test]
    fn ascent_ignores_noise() {
        let mk = |e: &[f64]| -> Vec<GpxPoint> {
            e.iter()
                .map(|&ele| GpxPoint {
                    ele: Some(ele),
                    ..GpxPoint::new(60.0, 10.0)
                })
                .collect()
        };
        assert_eq!(ascent_m(&mk(&[100.0, 101.0, 100.0, 101.5])), Some(0.0));
        assert_eq!(ascent_m(&mk(&[100.0, 110.0, 105.0, 120.0])), Some(25.0));
        assert_eq!(ascent_m(&mk(&[100.0])), None);
        assert_eq!(ascent_m(&line(5)), None);
    }

    #[test]
    fn length_matches_known_distance() {
        // One degree of latitude is about 111.2 km.
        let pts = vec![GpxPoint::new(60.0, 10.0), GpxPoint::new(61.0, 10.0)];
        let d = path_length_m(&pts);
        assert!((d - 111_195.0).abs() < 100.0, "{d}");
    }

    #[test]
    fn simplify_keeps_corner_and_ends() {
        let pts = vec![
            GpxPoint::new(60.0, 10.0),
            GpxPoint::new(60.0005, 10.0),
            GpxPoint::new(60.001, 10.0),
            GpxPoint::new(60.001, 10.001),
            GpxPoint::new(60.001, 10.002),
        ];
        let s = simplify(&pts, 1.0);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], pts[0]);
        assert_eq!(s[1], pts[2]);
        assert_eq!(s[2], pts[4]);
        // A tolerance beyond the corner offset collapses it to the chord.
        assert_eq!(simplify(&pts, 1_000.0).len(), 2);
        assert_eq!(simplify(&pts[..2], 1.0).len(), 2);
        assert!(simplify(&[], 1.0).is_empty());
    }

    #[test]
    fn simplify_handles_large_track_without_recursion() {
        let pts: Vec<GpxPoint> = (0..60_000)
            .map(|i| {
                let t = i as f64;
                GpxPoint::new(
                    60.0 + t * 1e-5,
                    10.0 + (t * 0.002).sin() * 0.01 + (t * 0.7).sin() * 0.00005,
                )
            })
            .collect();
        let s = simplify(&pts, 1.0);
        assert!(s.len() > 100 && s.len() < pts.len());
        assert_eq!(s.first(), pts.first());
        assert_eq!(s.last(), pts.last());
        let m = simplify_to_max_points(&pts, 500);
        assert_eq!(m.len(), 500);
        assert_eq!(m.first(), pts.first());
        assert_eq!(m.last(), pts.last());
    }

    #[test]
    fn simplify_to_max_points_limits_and_keeps_order() {
        let pts: Vec<GpxPoint> = (0..2_000)
            .map(|i| {
                let t = i as f64 / 100.0;
                GpxPoint::new(
                    60.0 + t.sin() * 0.01 + i as f64 * 1e-5,
                    10.0 + i as f64 * 1e-4,
                )
            })
            .collect();
        let m = simplify_to_max_points(&pts, 100);
        assert_eq!(m.len(), 100);
        let mut last_lon = f64::MIN;
        for p in &m {
            assert!(p.lon > last_lon);
            last_lon = p.lon;
        }
        // Small input is returned unchanged; collinear input stops early.
        assert_eq!(simplify_to_max_points(&pts[..50], 100).len(), 50);
        let straight = simplify_to_max_points(&line(1_000), 100);
        assert_eq!(straight.len(), 2);
        // max_points below 2 is clamped.
        assert_eq!(simplify_to_max_points(&pts, 0).len(), 2);
    }

    #[test]
    fn sample_via_points_even_spacing() {
        let pts = line(101); // about 11.1 km straight line
        let total = path_length_m(&pts);
        let v = sample_via_points(&pts, 4, 100.0);
        assert_eq!(v.len(), 6);
        assert_eq!(v[0], pts[0]);
        assert_eq!(v[5], pts[100]);
        let step = total / 5.0;
        for i in 1..5 {
            let d = haversine_m(v[0].lat, v[0].lon, v[i].lat, v[i].lon);
            assert!((d - step * i as f64).abs() < 1.0, "via {i}: {d}");
        }
    }

    #[test]
    fn sample_via_points_respects_min_spacing_and_edges() {
        let pts = line(101);
        // 11.1 km with 5 km minimum spacing allows exactly one intermediate point.
        let v = sample_via_points(&pts, 10, 5_000.0);
        assert_eq!(v.len(), 3);
        // Spacing larger than the path: first and last only.
        assert_eq!(sample_via_points(&pts, 10, 50_000.0).len(), 2);
        assert_eq!(sample_via_points(&pts, 0, 0.0).len(), 2);
        // Without a spacing constraint max_vias is honoured.
        assert_eq!(sample_via_points(&pts, 7, 0.0).len(), 9);
        // Degenerate inputs.
        assert!(sample_via_points(&[], 3, 10.0).is_empty());
        assert_eq!(sample_via_points(&pts[..1], 3, 10.0).len(), 1);
        assert_eq!(sample_via_points(&pts[..2], 3, 10.0).len(), 2);
        let same = vec![GpxPoint::new(60.0, 10.0); 5];
        assert_eq!(sample_via_points(&same, 3, 0.0).len(), 2);
    }

    #[test]
    fn sample_via_points_interpolates_elevation() {
        let mut pts = vec![GpxPoint::new(60.0, 10.0), GpxPoint::new(60.02, 10.0)];
        pts[0].ele = Some(0.0);
        pts[1].ele = Some(100.0);
        pts.push(GpxPoint::new(60.04, 10.0));
        pts[2].ele = Some(200.0);
        let v = sample_via_points(&pts, 1, 0.0);
        assert_eq!(v.len(), 3);
        assert!((v[1].ele.unwrap() - 100.0).abs() < 1.0);
        assert!(v[1].time.is_none());
    }

    #[test]
    fn large_synthetic_file_parses() {
        let mut s = String::from(
            r#"<?xml version="1.0"?><gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg>"#,
        );
        for i in 0..30_000 {
            s.push_str(&format!(
                r#"<trkpt lat="{:.6}" lon="{:.6}"><ele>{}</ele><time>2026-01-01T00:00:00Z</time></trkpt>"#,
                60.0 + i as f64 * 1e-5,
                10.0 + (i as f64 * 0.01).sin() * 1e-3,
                100 + i % 50
            ));
        }
        s.push_str("</trkseg></trk></gpx>");
        let doc = parse_gpx(&s).unwrap();
        assert_eq!(doc.track_point_count(), 30_000);
        assert!(doc.tracks[0].ascent_m().is_some());
    }
}
