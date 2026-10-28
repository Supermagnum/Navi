//! Stable UI message codes for host-side string resources.
//!
//! Progress APIs still carry a display string; prefer emitting [MSG_PREFIX] codes
//! so Kotlin can map to `R.string.*` without baking English in the core.

/// Prefix for codes in progress / status channels (`navi.msg.download_region`).
pub const MSG_PREFIX: &str = "navi.msg.";

pub mod codes {
    pub const DOWNLOAD_REGION: &str = "download_region";
    pub const DOWNLOAD_DEM: &str = "download_dem";
    pub const DOWNLOAD_TILES: &str = "download_tiles";
    pub const DOWNLOAD_ELEVATION: &str = "download_elevation";
    pub const DOWNLOAD_PLACE_SOURCE: &str = "download_place_source";
    pub const DOWNLOAD_PACK_UPDATE: &str = "download_pack_update";
    pub const BUILD_INDEXED_BOUNDS: &str = "build_indexed_bounds";
    pub const BUILD_INDEXED_GRAPHS: &str = "build_indexed_graphs";
    pub const BUILD_INDEXED_POI: &str = "build_indexed_poi";
    pub const BUILD_INDEXED_WETLANDS: &str = "build_indexed_wetlands";
    pub const BUILD_GRAPH: &str = "build_graph";
    pub const REBUILD_LOCAL: &str = "rebuild_local";
    pub const RESUMING_DOWNLOAD: &str = "resuming_download";
}

/// Encode a stable message code for the UI host.
pub fn encode(code: &str) -> String {
    format!("{MSG_PREFIX}{code}")
}

/// Encode with a single string parameter (appended after `|`).
pub fn encode1(code: &str, arg: &str) -> String {
    format!("{MSG_PREFIX}{code}|{arg}")
}

/// True when `s` is a host message code (optionally with params).
pub fn is_encoded(s: &str) -> bool {
    s.starts_with(MSG_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_round_trip_prefix() {
        let s = encode(codes::DOWNLOAD_REGION);
        assert!(is_encoded(&s));
        assert_eq!(s, "navi.msg.download_region");
        assert_eq!(
            encode1(codes::BUILD_GRAPH, "foo.navi-graph"),
            "navi.msg.build_graph|foo.navi-graph"
        );
    }
}
