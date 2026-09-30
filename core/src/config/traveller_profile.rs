//! Traveller residency for jurisdiction packs that differ by residency
//! (Ontario Crown land — right-to-roam camping spec §3.2 Canada).
//!
//! **Never** infer from GPS position or device locale. Default is unknown;
//! Ontario non-resident rules treat unknown as the stricter (non-resident) path.

use serde::{Deserialize, Serialize};

/// User-set traveller profile.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TravellerProfile {
    /// ISO 3166-1 alpha-2 lowercase when set. `None` = unknown.
    #[serde(default)]
    pub residency_country: Option<String>,
}

impl TravellerProfile {
    pub fn unknown() -> Self {
        Self {
            residency_country: None,
        }
    }

    /// Normalize and store a residency code; empty / invalid → unknown.
    pub fn with_residency_country(raw: Option<&str>) -> Self {
        let residency_country = raw.and_then(|s| {
            let t = s.trim().to_ascii_lowercase();
            if t.len() == 2 && t.chars().all(|c| c.is_ascii_alphabetic()) {
                Some(t)
            } else {
                None
            }
        });
        Self { residency_country }
    }

    /// Ontario non-resident path: unknown residency counts as non-resident.
    pub fn treat_as_non_resident_of_canada(&self) -> bool {
        match self.residency_country.as_deref() {
            Some("ca") => false,
            Some(_) | None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_residency_is_stricter_ontario_path() {
        assert!(TravellerProfile::unknown().treat_as_non_resident_of_canada());
        assert!(TravellerProfile::with_residency_country(Some("us")).treat_as_non_resident_of_canada());
        assert!(!TravellerProfile::with_residency_country(Some("CA")).treat_as_non_resident_of_canada());
    }
}
