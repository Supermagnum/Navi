//! Read-only storage volume descriptors for long-trip free-space checks.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VolumeId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageVolume {
    pub id: VolumeId,
    pub removable: bool,
    pub mounted: bool,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

impl StorageVolume {
    pub fn primary(free_bytes: u64, total_bytes: u64) -> Self {
        Self {
            id: VolumeId("primary".into()),
            removable: false,
            mounted: true,
            total_bytes,
            free_bytes,
        }
    }
}
