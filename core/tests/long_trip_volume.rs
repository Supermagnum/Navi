//! Phase B1: storage volume descriptors used by long-trip free-space checks.

use driver_break_core::long_trip::{StorageVolume, VolumeId};

#[test]
fn primary_and_removable_descriptors() {
    let p = StorageVolume::primary(100, 200);
    assert_eq!(p.id, VolumeId("internal".into()));
    assert!(!p.removable);
    assert!(p.mounted);
    assert_eq!(p.label, "Internal storage");

    let sd = StorageVolume::removable("uuid:abc", "SD card", 50, 100, true, "/mnt/sd/Android/data");
    assert!(sd.removable);
    assert_eq!(sd.id.0, "uuid:abc");
    assert!(sd.path.contains("Android/data"));
}

#[test]
fn unavailable_state_exists_for_card_removal() {
    use driver_break_core::long_trip::RegionTripState;
    let u = RegionTripState::Unavailable;
    assert!(matches!(u, RegionTripState::Unavailable));
}
