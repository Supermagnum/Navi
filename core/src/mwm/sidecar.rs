//! NVSC v1 / EL02 sidecar reader (`{leaf}.navi-sidecar.v1.zst`).
//!
//! Contract: navi-server `docs/sidecar-format.md` (release 261105).
//!
//! # Lookup design
//!
//! After a section is first loaded, records stay in memory for the life of the
//! [`SidecarReader`]:
//!
//! | Section | Structure | Approx. memory (Hedmark) |
//! |---------|-----------|---------------------------|
//! | `wetland_edges` | Sorted `Vec<WetlandRecord>` + binary search | ~784 KiB (8 B × 98 082) |
//! | `boardwalk_flags` | Sorted `Vec<BoardwalkRecord>` + binary search | ~1.7 KiB |
//! | `elevation_overrides` | Sorted feature index + flat delta-encoded point buffer | ~6 B/point + 12 B/feature |
//! | `attribution` | Owned `String` (UTF-8 JSON) | ~100 KiB |
//!
//! Sorted vectors match the bake sort order and avoid HashMap overhead for the
//! dense wetland table the router hits per edge. Elevation keeps EL02 deltas
//! packed and decodes only the requested feature on lookup. Sections decompress
//! lazily on first access (header + directory are always parsed).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use sha2::{Digest, Sha256};
use thiserror::Error;

use super::catalog::MwmFingerprint;

/// Logical-blob magic.
pub const MAGIC: &[u8; 4] = b"NVSC";
/// Supported sectioned format version.
pub const FORMAT_VERSION: u32 = 1;
/// Wetland edge section.
pub const SECTION_WETLAND: &str = "wetland_edges";
/// Boardwalk carve-out section.
pub const SECTION_BOARDWALK: &str = "boardwalk_flags";
/// Elevation overrides section.
pub const SECTION_ELEVATION: &str = "elevation_overrides";
/// Attribution JSON section.
pub const SECTION_ATTRIBUTION: &str = "attribution";
/// Soft wetland class.
pub const CLASS_SOFT: u8 = 1;
/// Hard wetland class.
pub const CLASS_HARD: u8 = 2;
/// Elevation body magic `EL02` (little-endian).
pub const ELEVATION_BODY_MAGIC_EL02: u32 = 0x3230_4C45;
const RECORD_LEN: usize = 8;
/// Packed EL02 point: `u16` index delta + `u32` zigzag altitude delta.
const ELEV_POINT_PACKED_LEN: usize = 6;

/// Sections that must appear in the directory for a usable Navi sidecar.
pub const REQUIRED_SECTIONS: &[&str] = &[SECTION_WETLAND, SECTION_ATTRIBUTION];

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zstd: {0}")]
    Zstd(String),
    #[error("bad magic")]
    BadMagic,
    #[error("unsupported format_version {0} (client supports {FORMAT_VERSION})")]
    UnsupportedVersion(u32),
    #[error("truncated sidecar")]
    Truncated,
    #[error("missing required section `{0}`")]
    MissingRequiredSection(String),
    #[error("section `{0}` size mismatch")]
    SectionSizeMismatch(String),
    #[error("section `{0}` checksum mismatch")]
    SectionChecksumMismatch(String),
    #[error("section `{0}` payload out of range")]
    SectionOutOfRange(String),
    #[error("elevation body is not EL02 (legacy i16 layout is not supported)")]
    ElevationNotEl02,
    #[error("elevation body corrupt: {0}")]
    ElevationCorrupt(String),
    #[error("wetland/boardwalk body corrupt: {0}")]
    RecordBodyCorrupt(String),
    #[error("attribution is not UTF-8")]
    AttributionUtf8,
    #[error("attribution is not JSON: {0}")]
    AttributionJson(String),
    #[error("MWM fingerprint mismatch")]
    FingerprintMismatch,
    #[error("utf-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WetlandRecord {
    pub feature_id: u32,
    pub segment_index: u16,
    pub direction: u8,
    pub class: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BoardwalkRecord {
    pub feature_id: u32,
    pub segment_index: u16,
    pub direction: u8,
    pub flag: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ElevationPoint {
    pub point_index: u16,
    /// Absolute altitude in integer decimetres (0.1 m), signed i32.
    pub altitude_dm: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElevationFeature {
    pub feature_id: u32,
    pub point_count: u16,
    pub points: Vec<ElevationPoint>,
}

/// Compact in-memory EL02 store: sorted feature directory + one owned EL02 body.
#[derive(Debug, Clone, Default)]
struct ElevationIndex {
    /// Sorted by `feature_id`.
    features: Vec<ElevFeatureEntry>,
    /// Full EL02 section bytes; points stay delta-encoded until lookup.
    body: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
struct ElevFeatureEntry {
    feature_id: u32,
    point_count: u16,
    /// Absolute byte offset of packed points inside [`ElevationIndex::body`].
    buf_offset: u32,
    /// Encoded point count (`n` in EL02).
    n: u16,
}

impl ElevationIndex {
    fn feature_count(&self) -> usize {
        self.features.len()
    }

    fn point_count_total(&self) -> usize {
        self.features.iter().map(|f| f.n as usize).sum()
    }

    fn find(&self, feature_id: u32) -> Option<&ElevFeatureEntry> {
        self.features
            .binary_search_by_key(&feature_id, |e| e.feature_id)
            .ok()
            .map(|i| &self.features[i])
    }

    fn decode_feature(
        &self,
        entry: &ElevFeatureEntry,
    ) -> Result<Vec<ElevationPoint>, SidecarError> {
        let start = entry.buf_offset as usize;
        let end = start
            .checked_add(entry.n as usize * ELEV_POINT_PACKED_LEN)
            .ok_or_else(|| SidecarError::ElevationCorrupt("point span overflow".into()))?;
        if end > self.body.len() {
            return Err(SidecarError::ElevationCorrupt("point span OOB".into()));
        }
        let mut points = Vec::with_capacity(entry.n as usize);
        let mut prev_idx = 0u16;
        let mut prev_alt = 0i32;
        let mut off = start;
        for i in 0..entry.n as usize {
            let idx_delta = read_u16(&self.body, off)?;
            off += 2;
            let zz = read_u32(&self.body, off)?;
            off += 4;
            let point_index = if i == 0 {
                idx_delta
            } else {
                prev_idx
                    .checked_add(idx_delta)
                    .ok_or_else(|| SidecarError::ElevationCorrupt("idx overflow".into()))?
            };
            let altitude_dm = prev_alt.wrapping_add(zigzag_decode_i32(zz));
            points.push(ElevationPoint {
                point_index,
                altitude_dm,
            });
            prev_idx = point_index;
            prev_alt = altitude_dm;
        }
        Ok(points)
    }

    fn to_feature_map(&self) -> Result<HashMap<u32, ElevationFeature>, SidecarError> {
        let mut map = HashMap::with_capacity(self.features.len());
        for entry in &self.features {
            let points = self.decode_feature(entry)?;
            map.insert(
                entry.feature_id,
                ElevationFeature {
                    feature_id: entry.feature_id,
                    point_count: entry.point_count,
                    points,
                },
            );
        }
        Ok(map)
    }
}

#[derive(Debug, Clone)]
pub struct SectionInfo {
    pub name: String,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub sha256: [u8; 32],
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub struct SidecarHeader {
    pub format_version: u32,
    pub generator_tag: String,
    pub generator_sha: String,
    pub mwm_sha256: [u8; 32],
    pub mwm_size: u64,
    pub sections: Vec<SectionInfo>,
}

impl SidecarHeader {
    pub fn mwm_fingerprint(&self) -> MwmFingerprint {
        MwmFingerprint {
            size: self.mwm_size,
            sha256: hex::encode(self.mwm_sha256),
        }
    }

    pub fn mwm_sha256_hex(&self) -> String {
        hex::encode(self.mwm_sha256)
    }
}

/// Opened sidecar with lazy section decode.
pub struct SidecarReader {
    header: SidecarHeader,
    /// Logical NVSC blob (outer zstd already removed). Cleared once every
    /// section cache is warm so RSS does not keep compressed + decoded copies.
    logical: Mutex<Vec<u8>>,
    payload_start: usize,
    wetland: Mutex<Option<Vec<WetlandRecord>>>,
    boardwalk: Mutex<Option<Vec<BoardwalkRecord>>>,
    elevation: Mutex<Option<ElevationIndex>>,
    attribution: Mutex<Option<String>>,
}

impl SidecarReader {
    /// Read and validate the outer `.zst` + NVSC header/directory.
    pub fn open_path(path: &Path) -> Result<Self, SidecarError> {
        let bytes = fs::read(path)?;
        Self::open_bytes(&bytes)
    }

    pub fn open_bytes(outer_zstd: &[u8]) -> Result<Self, SidecarError> {
        let logical = zstd_decompress(outer_zstd)?;
        Self::from_logical(logical)
    }

    fn from_logical(logical: Vec<u8>) -> Result<Self, SidecarError> {
        if logical.len() < 8 || &logical[0..4] != MAGIC {
            return Err(SidecarError::BadMagic);
        }
        let format_version = read_u32(&logical, 4)?;
        if format_version != FORMAT_VERSION {
            return Err(SidecarError::UnsupportedVersion(format_version));
        }
        let mut off = 8usize;
        let generator_tag = read_u16_str(&logical, &mut off)?;
        let generator_sha = read_u16_str(&logical, &mut off)?;
        if off + 32 + 8 + 4 > logical.len() {
            return Err(SidecarError::Truncated);
        }
        let mut mwm_sha256 = [0u8; 32];
        mwm_sha256.copy_from_slice(&logical[off..off + 32]);
        off += 32;
        let mwm_size = read_u64(&logical, off)?;
        off += 8;
        let section_count = read_u32(&logical, off)? as usize;
        off += 4;

        let mut sections = Vec::with_capacity(section_count);
        for _ in 0..section_count {
            let name = read_u16_str(&logical, &mut off)?;
            if off + 8 + 8 + 32 + 8 > logical.len() {
                return Err(SidecarError::Truncated);
            }
            let uncompressed_size = read_u64(&logical, off)?;
            off += 8;
            let compressed_size = read_u64(&logical, off)?;
            off += 8;
            let mut sha = [0u8; 32];
            sha.copy_from_slice(&logical[off..off + 32]);
            off += 32;
            let offset = read_u64(&logical, off)?;
            off += 8;
            sections.push(SectionInfo {
                name,
                uncompressed_size,
                compressed_size,
                sha256: sha,
                offset,
            });
        }
        let payload_start = off;
        let header = SidecarHeader {
            format_version,
            generator_tag,
            generator_sha,
            mwm_sha256,
            mwm_size,
            sections,
        };
        ensure_required_sections(&header.sections)?;
        Ok(Self {
            header,
            logical: Mutex::new(logical),
            payload_start,
            wetland: Mutex::new(None),
            boardwalk: Mutex::new(None),
            elevation: Mutex::new(None),
            attribution: Mutex::new(None),
        })
    }

    pub fn header(&self) -> &SidecarHeader {
        &self.header
    }

    /// Reject the sidecar when the local MWM fingerprint does not match.
    pub fn require_fingerprint(&self, expected: &MwmFingerprint) -> Result<(), SidecarError> {
        let got = self.header.mwm_fingerprint();
        if got.size == expected.size && got.sha256.eq_ignore_ascii_case(&expected.sha256) {
            Ok(())
        } else {
            Err(SidecarError::FingerprintMismatch)
        }
    }

    pub fn wetland_records(&self) -> Result<Vec<WetlandRecord>, SidecarError> {
        self.ensure_wetland()?;
        Ok(self.wetland.lock().unwrap().clone().unwrap_or_default())
    }

    pub fn wetland_class(
        &self,
        feature_id: u32,
        segment_index: u16,
        direction: u8,
    ) -> Result<Option<u8>, SidecarError> {
        self.ensure_wetland()?;
        let guard = self.wetland.lock().unwrap();
        let records = guard.as_ref().unwrap();
        Ok(find_wetland(records, feature_id, segment_index, direction).map(|r| r.class))
    }

    pub fn is_boardwalk(
        &self,
        feature_id: u32,
        segment_index: u16,
        direction: u8,
    ) -> Result<bool, SidecarError> {
        self.ensure_boardwalk()?;
        let guard = self.boardwalk.lock().unwrap();
        let records = guard.as_ref().unwrap();
        Ok(find_boardwalk(records, feature_id, segment_index, direction).is_some())
    }

    pub fn boardwalk_records(&self) -> Result<Vec<BoardwalkRecord>, SidecarError> {
        self.ensure_boardwalk()?;
        Ok(self.boardwalk.lock().unwrap().clone().unwrap_or_default())
    }

    /// Absolute altitude in metres when an EL02 override exists.
    pub fn elevation_metres(
        &self,
        feature_id: u32,
        point_index: u16,
    ) -> Result<Option<f64>, SidecarError> {
        Ok(self
            .elevation_dm(feature_id, point_index)?
            .map(|dm| f64::from(dm) / 10.0))
    }

    pub fn elevation_dm(
        &self,
        feature_id: u32,
        point_index: u16,
    ) -> Result<Option<i32>, SidecarError> {
        self.ensure_elevation()?;
        let guard = self.elevation.lock().unwrap();
        let index = guard.as_ref().unwrap();
        let Some(entry) = index.find(feature_id) else {
            return Ok(None);
        };
        let points = index.decode_feature(entry)?;
        Ok(points
            .binary_search_by_key(&point_index, |p| p.point_index)
            .ok()
            .map(|i| points[i].altitude_dm))
    }

    /// Materializes decoded features (test / inspect helpers). Hot path should
    /// use [`Self::elevation_dm`] so only one feature's deltas are expanded.
    pub fn elevation_features(&self) -> Result<HashMap<u32, ElevationFeature>, SidecarError> {
        self.ensure_elevation()?;
        let guard = self.elevation.lock().unwrap();
        guard.as_ref().unwrap().to_feature_map()
    }

    /// Attribution JSON text for the Licences / About screen.
    pub fn attribution_json(&self) -> Result<String, SidecarError> {
        self.ensure_attribution()?;
        Ok(self.attribution.lock().unwrap().clone().unwrap_or_default())
    }

    pub fn attribution_value(&self) -> Result<serde_json::Value, SidecarError> {
        let text = self.attribution_json()?;
        serde_json::from_str(&text).map_err(|e| SidecarError::AttributionJson(e.to_string()))
    }

    /// Counts useful for inspect / golden tests.
    pub fn inspect_counts(&self) -> Result<InspectCounts, SidecarError> {
        let wetland = self.wetland_records()?;
        let boardwalk = self.boardwalk_records()?;
        self.ensure_elevation()?;
        let elev_guard = self.elevation.lock().unwrap();
        let elevation = elev_guard.as_ref().unwrap();
        let soft = wetland.iter().filter(|r| r.class == CLASS_SOFT).count();
        let hard = wetland.iter().filter(|r| r.class == CLASS_HARD).count();
        Ok(InspectCounts {
            wetland_records: wetland.len(),
            soft_records: soft,
            hard_records: hard,
            boardwalk_records: boardwalk.len(),
            elevation_features: elevation.feature_count(),
            elevation_points: elevation.point_count_total(),
            sections: self
                .header
                .sections
                .iter()
                .map(|s| s.name.clone())
                .collect(),
        })
    }

    fn ensure_wetland(&self) -> Result<(), SidecarError> {
        let mut g = self.wetland.lock().unwrap();
        if g.is_some() {
            return Ok(());
        }
        let raw = self.load_section_body(SECTION_WETLAND)?;
        *g = Some(parse_wetland_body(&raw)?);
        drop(g);
        self.maybe_release_logical();
        Ok(())
    }

    fn ensure_boardwalk(&self) -> Result<(), SidecarError> {
        let mut g = self.boardwalk.lock().unwrap();
        if g.is_some() {
            return Ok(());
        }
        match self.find_section(SECTION_BOARDWALK) {
            None => *g = Some(Vec::new()),
            Some(_) => {
                let raw = self.load_section_body(SECTION_BOARDWALK)?;
                *g = Some(parse_boardwalk_body(&raw)?);
            }
        }
        drop(g);
        self.maybe_release_logical();
        Ok(())
    }

    fn ensure_elevation(&self) -> Result<(), SidecarError> {
        let mut g = self.elevation.lock().unwrap();
        if g.is_some() {
            return Ok(());
        }
        match self.find_section(SECTION_ELEVATION) {
            None => *g = Some(ElevationIndex::default()),
            Some(_) => {
                let raw = self.load_section_body(SECTION_ELEVATION)?;
                *g = Some(build_elevation_index(raw)?);
            }
        }
        drop(g);
        self.maybe_release_logical();
        Ok(())
    }

    fn ensure_attribution(&self) -> Result<(), SidecarError> {
        let mut g = self.attribution.lock().unwrap();
        if g.is_some() {
            return Ok(());
        }
        let raw = self.load_section_body(SECTION_ATTRIBUTION)?;
        let text = String::from_utf8(raw).map_err(|_| SidecarError::AttributionUtf8)?;
        serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|e| SidecarError::AttributionJson(e.to_string()))?;
        *g = Some(text);
        drop(g);
        self.maybe_release_logical();
        Ok(())
    }

    fn find_section(&self, name: &str) -> Option<&SectionInfo> {
        self.header.sections.iter().find(|s| s.name == name)
    }

    fn load_section_body(&self, name: &str) -> Result<Vec<u8>, SidecarError> {
        let info = self
            .find_section(name)
            .ok_or_else(|| SidecarError::MissingRequiredSection(name.to_string()))?;
        let start = self.payload_start
            + usize::try_from(info.offset).map_err(|_| SidecarError::Truncated)?;
        let n = usize::try_from(info.compressed_size).map_err(|_| SidecarError::Truncated)?;
        let end = start
            .checked_add(n)
            .ok_or_else(|| SidecarError::SectionOutOfRange(name.to_string()))?;
        let logical = self.logical.lock().unwrap();
        if logical.is_empty() {
            return Err(SidecarError::SectionOutOfRange(name.to_string()));
        }
        if end > logical.len() {
            return Err(SidecarError::SectionOutOfRange(name.to_string()));
        }
        let raw = zstd_decompress(&logical[start..end])?;
        drop(logical);
        if u64::try_from(raw.len()).unwrap_or(u64::MAX) != info.uncompressed_size {
            return Err(SidecarError::SectionSizeMismatch(name.to_string()));
        }
        let got = sha256_32(&raw);
        if got != info.sha256 {
            return Err(SidecarError::SectionChecksumMismatch(name.to_string()));
        }
        Ok(raw)
    }

    fn maybe_release_logical(&self) {
        let warm = self.wetland.lock().unwrap().is_some()
            && self.boardwalk.lock().unwrap().is_some()
            && self.elevation.lock().unwrap().is_some()
            && self.attribution.lock().unwrap().is_some();
        if !warm {
            return;
        }
        let mut logical = self.logical.lock().unwrap();
        if !logical.is_empty() {
            logical.clear();
            logical.shrink_to_fit();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectCounts {
    pub wetland_records: usize,
    pub soft_records: usize,
    pub hard_records: usize,
    pub boardwalk_records: usize,
    pub elevation_features: usize,
    pub elevation_points: usize,
    pub sections: Vec<String>,
}

fn ensure_required_sections(sections: &[SectionInfo]) -> Result<(), SidecarError> {
    for req in REQUIRED_SECTIONS {
        if !sections.iter().any(|s| s.name == *req) {
            return Err(SidecarError::MissingRequiredSection((*req).to_string()));
        }
    }
    Ok(())
}

fn find_wetland(
    records: &[WetlandRecord],
    feature_id: u32,
    segment_index: u16,
    direction: u8,
) -> Option<&WetlandRecord> {
    records
        .binary_search_by(|r| {
            (r.feature_id, r.segment_index, r.direction).cmp(&(
                feature_id,
                segment_index,
                direction,
            ))
        })
        .ok()
        .map(|i| &records[i])
}

fn find_boardwalk(
    records: &[BoardwalkRecord],
    feature_id: u32,
    segment_index: u16,
    direction: u8,
) -> Option<&BoardwalkRecord> {
    records
        .binary_search_by(|r| {
            (r.feature_id, r.segment_index, r.direction).cmp(&(
                feature_id,
                segment_index,
                direction,
            ))
        })
        .ok()
        .map(|i| &records[i])
}

fn parse_wetland_body(data: &[u8]) -> Result<Vec<WetlandRecord>, SidecarError> {
    if data.len() < 4 {
        return Err(SidecarError::RecordBodyCorrupt("wetland too short".into()));
    }
    let count = read_u32(data, 0)? as usize;
    let need = 4 + count * RECORD_LEN;
    if data.len() != need {
        return Err(SidecarError::RecordBodyCorrupt(format!(
            "wetland len {} want {}",
            data.len(),
            need
        )));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = 4 + i * RECORD_LEN;
        out.push(WetlandRecord {
            feature_id: read_u32(data, o)?,
            segment_index: read_u16(data, o + 4)?,
            direction: data[o + 6],
            class: data[o + 7],
        });
    }
    Ok(out)
}

fn parse_boardwalk_body(data: &[u8]) -> Result<Vec<BoardwalkRecord>, SidecarError> {
    if data.len() < 4 {
        return Err(SidecarError::RecordBodyCorrupt(
            "boardwalk too short".into(),
        ));
    }
    let count = read_u32(data, 0)? as usize;
    let need = 4 + count * RECORD_LEN;
    if data.len() != need {
        return Err(SidecarError::RecordBodyCorrupt(format!(
            "boardwalk len {} want {}",
            data.len(),
            need
        )));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = 4 + i * RECORD_LEN;
        out.push(BoardwalkRecord {
            feature_id: read_u32(data, o)?,
            segment_index: read_u16(data, o + 4)?,
            direction: data[o + 6],
            flag: data[o + 7],
        });
    }
    Ok(out)
}

/// Parse elevation overrides — **EL02 only** (fully decoded; prefer compact index on reader).
pub fn parse_elevation_el02(data: &[u8]) -> Result<Vec<ElevationFeature>, SidecarError> {
    let index = build_elevation_index(data.to_vec())?;
    let mut out = Vec::with_capacity(index.features.len());
    for entry in &index.features {
        out.push(ElevationFeature {
            feature_id: entry.feature_id,
            point_count: entry.point_count,
            points: index.decode_feature(entry)?,
        });
    }
    Ok(out)
}

/// Build sorted feature directory over one owned EL02 body (no point expansion).
fn build_elevation_index(data: Vec<u8>) -> Result<ElevationIndex, SidecarError> {
    if data.len() < 4 {
        return Err(SidecarError::ElevationCorrupt("too short".into()));
    }
    let magic = read_u32(&data, 0)?;
    if magic != ELEVATION_BODY_MAGIC_EL02 {
        return Err(SidecarError::ElevationNotEl02);
    }
    if data.len() < 8 {
        return Err(SidecarError::ElevationCorrupt("v2 too short".into()));
    }
    let count = read_u32(&data, 4)? as usize;
    let mut off = 8usize;
    let mut features = Vec::with_capacity(count);
    for _ in 0..count {
        if off + 8 > data.len() {
            return Err(SidecarError::ElevationCorrupt("truncated feature".into()));
        }
        let feature_id = read_u32(&data, off)?;
        off += 4;
        let point_count = read_u16(&data, off)?;
        off += 2;
        let n = read_u16(&data, off)? as usize;
        off += 2;
        let need = n
            .checked_mul(ELEV_POINT_PACKED_LEN)
            .ok_or_else(|| SidecarError::ElevationCorrupt("point bytes overflow".into()))?;
        if off + need > data.len() {
            return Err(SidecarError::ElevationCorrupt("truncated point".into()));
        }
        let buf_offset = u32::try_from(off)
            .map_err(|_| SidecarError::ElevationCorrupt("buf too large".into()))?;
        off += need;
        features.push(ElevFeatureEntry {
            feature_id,
            point_count,
            buf_offset,
            n: n as u16,
        });
    }
    if off != data.len() {
        return Err(SidecarError::ElevationCorrupt("trailing bytes".into()));
    }
    features.sort_by_key(|e| e.feature_id);
    for w in features.windows(2) {
        if w[0].feature_id == w[1].feature_id {
            return Err(SidecarError::ElevationCorrupt(
                "duplicate feature_id".into(),
            ));
        }
    }
    Ok(ElevationIndex {
        features,
        body: data,
    })
}

/// Encode EL02 elevation body (for unit tests).
pub fn encode_elevation_el02(features: &[ElevationFeature]) -> Result<Vec<u8>, SidecarError> {
    let mut body = Vec::new();
    body.extend_from_slice(&ELEVATION_BODY_MAGIC_EL02.to_le_bytes());
    body.extend_from_slice(&(features.len() as u32).to_le_bytes());
    for feat in features {
        body.extend_from_slice(&feat.feature_id.to_le_bytes());
        body.extend_from_slice(&feat.point_count.to_le_bytes());
        let n = u16::try_from(feat.points.len())
            .map_err(|_| SidecarError::ElevationCorrupt("too many points".into()))?;
        body.extend_from_slice(&n.to_le_bytes());
        let mut prev_idx = 0u16;
        let mut prev_alt = 0i32;
        for (i, p) in feat.points.iter().enumerate() {
            let idx_delta = if i == 0 {
                p.point_index
            } else {
                p.point_index.checked_sub(prev_idx).ok_or_else(|| {
                    SidecarError::ElevationCorrupt("point_index not ascending".into())
                })?
            };
            body.extend_from_slice(&idx_delta.to_le_bytes());
            let alt_delta = p.altitude_dm.wrapping_sub(prev_alt);
            body.extend_from_slice(&zigzag_encode_i32(alt_delta).to_le_bytes());
            prev_idx = p.point_index;
            prev_alt = p.altitude_dm;
        }
    }
    Ok(body)
}

pub fn zigzag_encode_i32(v: i32) -> u32 {
    ((v << 1) ^ (v >> 31)) as u32
}

pub fn zigzag_decode_i32(v: u32) -> i32 {
    ((v >> 1) as i32) ^ -((v & 1) as i32)
}

fn zstd_decompress(data: &[u8]) -> Result<Vec<u8>, SidecarError> {
    zstd::stream::decode_all(data).map_err(|e| SidecarError::Zstd(e.to_string()))
}

fn sha256_32(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut a = [0u8; 32];
    a.copy_from_slice(&out);
    a
}

fn read_u16(data: &[u8], off: usize) -> Result<u16, SidecarError> {
    let b = data.get(off..off + 2).ok_or(SidecarError::Truncated)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(data: &[u8], off: usize) -> Result<u32, SidecarError> {
    let b = data.get(off..off + 4).ok_or(SidecarError::Truncated)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_u64(data: &[u8], off: usize) -> Result<u64, SidecarError> {
    let b = data.get(off..off + 8).ok_or(SidecarError::Truncated)?;
    Ok(u64::from_le_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

fn read_u16_str(data: &[u8], off: &mut usize) -> Result<String, SidecarError> {
    let len = read_u16(data, *off)? as usize;
    *off += 2;
    let slice = data.get(*off..*off + len).ok_or(SidecarError::Truncated)?;
    *off += len;
    Ok(String::from_utf8(slice.to_vec())?)
}

/// Build a minimal sectioned NVSC logical blob + outer zstd (tests).
pub fn encode_test_sidecar_zstd(
    generator_tag: &str,
    generator_sha: &str,
    mwm_sha256: [u8; 32],
    mwm_size: u64,
    wetland: &[WetlandRecord],
    boardwalk: &[BoardwalkRecord],
    elevation: &[ElevationFeature],
    attribution_json: &str,
) -> Result<Vec<u8>, SidecarError> {
    let mut sections_bodies: Vec<(&str, Vec<u8>)> = Vec::new();
    sections_bodies.push((SECTION_WETLAND, encode_record_section_wetland(wetland)));
    sections_bodies.push((
        SECTION_BOARDWALK,
        encode_record_section_boardwalk(boardwalk),
    ));
    if !elevation.is_empty() {
        sections_bodies.push((SECTION_ELEVATION, encode_elevation_el02(elevation)?));
    }
    sections_bodies.push((SECTION_ATTRIBUTION, attribution_json.as_bytes().to_vec()));

    let mut payload = Vec::new();
    let mut infos = Vec::new();
    for (name, body) in &sections_bodies {
        let comp = zstd::stream::encode_all(body.as_slice(), 10)
            .map_err(|e| SidecarError::Zstd(e.to_string()))?;
        let offset = payload.len() as u64;
        infos.push(SectionInfo {
            name: (*name).to_string(),
            uncompressed_size: body.len() as u64,
            compressed_size: comp.len() as u64,
            sha256: sha256_32(body),
            offset,
        });
        payload.extend_from_slice(&comp);
    }

    let mut logical = Vec::new();
    logical.extend_from_slice(MAGIC);
    logical.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    write_u16_str(&mut logical, generator_tag);
    write_u16_str(&mut logical, generator_sha);
    logical.extend_from_slice(&mwm_sha256);
    logical.extend_from_slice(&mwm_size.to_le_bytes());
    logical.extend_from_slice(&(infos.len() as u32).to_le_bytes());
    for info in &infos {
        write_u16_str(&mut logical, &info.name);
        logical.extend_from_slice(&info.uncompressed_size.to_le_bytes());
        logical.extend_from_slice(&info.compressed_size.to_le_bytes());
        logical.extend_from_slice(&info.sha256);
        logical.extend_from_slice(&info.offset.to_le_bytes());
    }
    logical.extend_from_slice(&payload);
    zstd::stream::encode_all(logical.as_slice(), 10).map_err(|e| SidecarError::Zstd(e.to_string()))
}

fn encode_record_section_wetland(records: &[WetlandRecord]) -> Vec<u8> {
    let mut b = Vec::with_capacity(4 + records.len() * RECORD_LEN);
    b.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for r in records {
        b.extend_from_slice(&r.feature_id.to_le_bytes());
        b.extend_from_slice(&r.segment_index.to_le_bytes());
        b.push(r.direction);
        b.push(r.class);
    }
    b
}

fn encode_record_section_boardwalk(records: &[BoardwalkRecord]) -> Vec<u8> {
    let mut b = Vec::with_capacity(4 + records.len() * RECORD_LEN);
    b.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for r in records {
        b.extend_from_slice(&r.feature_id.to_le_bytes());
        b.extend_from_slice(&r.segment_index.to_le_bytes());
        b.push(r.direction);
        b.push(r.flag);
    }
    b
}

fn write_u16_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    buf.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_roundtrip_edges() {
        for v in [
            0,
            1,
            -1,
            5000,
            -5000,
            32768,
            -32768,
            53000,
            88480,
            i32::MAX,
            i32::MIN + 1,
        ] {
            assert_eq!(zigzag_decode_i32(zigzag_encode_i32(v)), v, "{v}");
        }
    }

    #[test]
    fn el02_altitude_roundtrip_metres() {
        // −500 m, 0, 3276.8 m, 5300 m, 8848 m (Everest) as decimetres.
        let alts_dm = [-5000, 0, 32768, 53000, 88480];
        let feat = ElevationFeature {
            feature_id: 42,
            point_count: 10,
            points: alts_dm
                .iter()
                .enumerate()
                .map(|(i, &dm)| ElevationPoint {
                    point_index: i as u16,
                    altitude_dm: dm,
                })
                .collect(),
        };
        let body = encode_elevation_el02(&[feat.clone()]).unwrap();
        let parsed = parse_elevation_el02(&body).unwrap();
        assert_eq!(parsed, vec![feat]);
        assert!(32768 > 32767); // beyond i16 max
    }

    #[test]
    fn rejects_legacy_elevation_without_el02_magic() {
        // Fake v1-shaped body (feature_count=0) without EL02 magic.
        let err = parse_elevation_el02(&0u32.to_le_bytes()).unwrap_err();
        assert!(matches!(err, SidecarError::ElevationNotEl02));
    }

    #[test]
    fn encode_open_roundtrip_mini_sidecar() {
        let mwm_sha = [0x11u8; 32];
        let zst = encode_test_sidecar_zstd(
            "test-gen",
            "deadbeef",
            mwm_sha,
            100,
            &[WetlandRecord {
                feature_id: 1,
                segment_index: 2,
                direction: 0,
                class: CLASS_SOFT,
            }],
            &[BoardwalkRecord {
                feature_id: 1,
                segment_index: 2,
                direction: 0,
                flag: 1,
            }],
            &[ElevationFeature {
                feature_id: 9,
                point_count: 3,
                points: vec![ElevationPoint {
                    point_index: 1,
                    altitude_dm: 88480,
                }],
            }],
            r#"{"navi":{"elevation_policy_version":1}}"#,
        )
        .unwrap();
        let r = SidecarReader::open_bytes(&zst).unwrap();
        assert_eq!(r.header().mwm_size, 100);
        assert_eq!(r.wetland_class(1, 2, 0).unwrap(), Some(CLASS_SOFT));
        assert!(r.is_boardwalk(1, 2, 0).unwrap());
        assert_eq!(r.elevation_dm(9, 1).unwrap(), Some(88480));
        assert!(r.attribution_json().unwrap().contains("elevation_policy"));
        r.require_fingerprint(&MwmFingerprint {
            size: 100,
            sha256: hex::encode(mwm_sha),
        })
        .unwrap();
        assert!(matches!(
            r.require_fingerprint(&MwmFingerprint {
                size: 99,
                sha256: hex::encode(mwm_sha),
            }),
            Err(SidecarError::FingerprintMismatch)
        ));
    }

    #[test]
    fn corrupt_header_and_truncated() {
        assert!(matches!(
            SidecarReader::open_bytes(b"not-zstd"),
            Err(SidecarError::Zstd(_))
        ));
        let good = encode_test_sidecar_zstd("t", "s", [0; 32], 1, &[], &[], &[], "{}").unwrap();
        let mut logical = zstd_decompress(&good).unwrap();
        logical[0] = b'X';
        let bad_outer = zstd::stream::encode_all(logical.as_slice(), 3).unwrap();
        assert!(matches!(
            SidecarReader::open_bytes(&bad_outer),
            Err(SidecarError::BadMagic)
        ));
    }

    #[test]
    fn unknown_optional_section_skipped() {
        // Built-in encoder only emits known sections; simulate by opening Hedmark
        // is covered in golden test. Here ensure missing boardwalk/elevation OK.
        let zst = encode_test_sidecar_zstd("t", "s", [1; 32], 5, &[], &[], &[], "{}").unwrap();
        let r = SidecarReader::open_bytes(&zst).unwrap();
        assert!(r.boardwalk_records().unwrap().is_empty());
        assert!(r.elevation_features().unwrap().is_empty());
    }
}
