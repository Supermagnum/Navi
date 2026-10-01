//! Vehicle overnight profile fields for plugin HostApi `vehicle_profile_read`.
//!
//! Distinct from clearance [`super::VehicleLimits`]: this carries the
//! right-to-roam §3.5 class snapshot and the user-set professional-driver flag.
//! `caravan_combo` is intentionally unsupported.

use serde::{Deserialize, Serialize};

use super::Profile;

/// Overnight vehicle class exposed to plugins (subset of travel profiles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VehicleOvernightClass {
    Car,
    CampervanMotorhome,
    Hgv,
    /// Unsupported (including caravan_combo) or non-vehicle travel profile.
    #[default]
    Unknown,
}

impl VehicleOvernightClass {
    /// Map the active travel profile. Never invents `caravan_combo`.
    pub fn from_travel_profile(profile: Profile) -> Self {
        match profile {
            Profile::Car
            | Profile::CarElectric
            | Profile::Motorcycle
            | Profile::MotorcycleElectric => Self::Car,
            Profile::MobileHome => Self::CampervanMotorhome,
            Profile::Truck | Profile::TruckElectric => Self::Hgv,
            Profile::Hiking | Profile::Cycling | Profile::CyclingElectric => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Car => "car",
            Self::CampervanMotorhome => "campervan_motorhome",
            Self::Hgv => "hgv",
            Self::Unknown => "unknown",
        }
    }
}

/// Persisted extras for vehicle overnight (not clearance dimensions).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VehicleOvernightConfig {
    /// User-set. Default **false**. Never infer from weight or size.
    #[serde(default)]
    pub is_professional_driver_under_rest_rules: bool,
}

/// Snapshot returned by HostApi `vehicle_profile_read`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleProfileView {
    pub class: VehicleOvernightClass,
    pub gross_weight_kg: Option<f64>,
    pub is_professional_driver_under_rest_rules: bool,
}

impl VehicleProfileView {
    pub fn from_parts(
        profile: Profile,
        limits: &super::VehicleLimits,
        overnight: &VehicleOvernightConfig,
    ) -> Self {
        Self {
            class: VehicleOvernightClass::from_travel_profile(profile),
            gross_weight_kg: limits.total_weight_kg,
            is_professional_driver_under_rest_rules: overnight
                .is_professional_driver_under_rest_rules,
        }
    }

    pub fn unknown() -> Self {
        Self {
            class: VehicleOvernightClass::Unknown,
            gross_weight_kg: None,
            is_professional_driver_under_rest_rules: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VehicleLimits;

    #[test]
    fn professional_driver_defaults_false_never_inferred_from_weight() {
        let overnight = VehicleOvernightConfig::default();
        assert!(!overnight.is_professional_driver_under_rest_rules);
        let heavy = VehicleLimits {
            total_weight_kg: Some(40_000.0),
            ..VehicleLimits::default()
        };
        let view = VehicleProfileView::from_parts(Profile::Truck, &heavy, &overnight);
        assert_eq!(view.class, VehicleOvernightClass::Hgv);
        assert!(!view.is_professional_driver_under_rest_rules);
    }

    #[test]
    fn mobile_home_maps_to_campervan_not_hgv() {
        assert_eq!(
            VehicleOvernightClass::from_travel_profile(Profile::MobileHome),
            VehicleOvernightClass::CampervanMotorhome
        );
    }
}
