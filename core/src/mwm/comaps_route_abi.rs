//! ABI layout for `NaviComapsRouteResult` — kept in sync with
//! `native/comaps_shim/navi_comaps.h` and liborganicmaps.so `navi_comaps_free_route`.
//!
//! Regression: omitting `vehicle_segments_json` made Rust's stack buffer 104 bytes
//! while C++ memset/free used 112 → SIGSEGV at free_route+74 (7th free at 0x68).

/// Documented x86_64/aarch64 layout size (enum int + pad + fields through vehicle_segments_json).
pub const NAVI_COMAPS_ROUTE_RESULT_SIZE: usize = 112;
pub const NAVI_COMAPS_ROUTE_RESULT_POINTS_OFFSET: usize = 0x20;
pub const NAVI_COMAPS_ROUTE_RESULT_VEHICLE_SEGMENTS_OFFSET: usize = 0x68;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_route_seventh_pointer_is_vehicle_segments_json() {
        // free_route in liborganicmaps frees pointers at:
        // 0x20 points, 0x30 error, 0x38 turns, 0x50 elev, 0x58 mode,
        // 0x60 speed_limits, 0x68 vehicle_segments.
        assert_eq!(NAVI_COMAPS_ROUTE_RESULT_VEHICLE_SEGMENTS_OFFSET, 0x68);
        assert_eq!(NAVI_COMAPS_ROUTE_RESULT_SIZE, 0x70);
        assert_eq!(NAVI_COMAPS_ROUTE_RESULT_POINTS_OFFSET, 0x20);
    }
}
