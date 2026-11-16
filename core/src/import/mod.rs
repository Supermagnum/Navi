//! Import helpers (GPX and related parsers). Counterpart of [`crate::export`].

pub mod gpx_import;

pub use gpx_import::{
    ascent_m, parse_gpx, parse_gpx_bytes, path_length_m, sample_via_points, simplify,
    simplify_to_max_points, GpxBounds, GpxDocument, GpxImportError, GpxPoint, GpxRoute, GpxTrack,
    GpxWaypoint,
};
