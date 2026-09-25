//! Read-only storage volume descriptors for long-trip free-space checks.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VolumeId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageVolume {
    pub id: VolumeId,
    /// User-visible label (e.g. "Internal storage", "SD card").
    pub label: String,
    pub removable: bool,
    pub mounted: bool,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// Absolute app-files path when mounted; empty when unknown/unmounted.
    pub path: String,
}

impl StorageVolume {
    pub fn primary(free_bytes: u64, total_bytes: u64) -> Self {
        Self {
            id: VolumeId("internal".into()),
            label: "Internal storage".into(),
            removable: false,
            mounted: true,
            total_bytes,
            free_bytes,
            path: String::new(),
        }
    }

    pub fn removable(
        id: impl Into<String>,
        label: impl Into<String>,
        free_bytes: u64,
        total_bytes: u64,
        mounted: bool,
        path: impl Into<String>,
    ) -> Self {
        Self {
            id: VolumeId(id.into()),
            label: label.into(),
            removable: true,
            mounted,
            total_bytes,
            free_bytes,
            path: path.into(),
        }
    }
}
