//! Jurisdiction-keyed commercial driving-hours pack selector.

use serde::{Deserialize, Serialize};

/// Which commercial HOS / driving-hours rule pack applies for a truck plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JurisdictionDrivingHoursPack {
    /// EU Regulation EC 561/2006 (and closely aligned EEA / AETR-style use).
    #[default]
    Ec561,
    /// US FMCSA Hours of Service (property-carrying), 49 CFR 395.3.
    Fmcsa,
    /// No recognized pack — decline legal tracking rather than guess.
    Unknown,
}

impl JurisdictionDrivingHoursPack {
    pub fn as_report_key(self) -> &'static str {
        match self {
            Self::Ec561 => "ec561",
            Self::Fmcsa => "fmcsa",
            Self::Unknown => "unknown",
        }
    }
}
