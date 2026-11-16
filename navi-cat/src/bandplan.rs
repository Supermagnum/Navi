//! Amateur band plan checks for repeater shift (ITU Region 1, Norway).
//!
//! Source data can carry absurd shifts (one mirrored OSM row has 287.9375 MHz).
//! A bad value must never reach the rig: the row is rejected, never clamped and
//! never replaced by a guessed standard offset.

use std::fmt;

/// Any shift above this is rejected outright, whatever the band.
pub const MAX_SHIFT_MHZ: f64 = 10.0;

/// Float tolerance when matching a shift against the standard offsets (0.5 kHz).
const OFFSET_TOLERANCE_MHZ: f64 = 0.0005;

struct Band {
    name: &'static str,
    lo_mhz: f64,
    hi_mhz: f64,
    /// Standard offsets (absolute, MHz). `None` means only the generic checks apply.
    offsets_mhz: Option<&'static [f64]>,
}

/// 2 m accepts 0.6 MHz. 70 cm accepts 1.6 and 2.0 MHz (Norway uses -2.0 MHz) and
/// the 7.6 MHz legacy split. Simplex (0) is always accepted.
const BANDS: &[Band] = &[
    Band {
        name: "10m",
        lo_mhz: 28.0,
        hi_mhz: 29.7,
        offsets_mhz: None,
    },
    Band {
        name: "6m",
        lo_mhz: 50.0,
        hi_mhz: 52.0,
        offsets_mhz: None,
    },
    Band {
        name: "4m",
        lo_mhz: 70.0,
        hi_mhz: 70.5,
        offsets_mhz: None,
    },
    Band {
        name: "2m",
        lo_mhz: 144.0,
        hi_mhz: 146.0,
        offsets_mhz: Some(&[0.6]),
    },
    Band {
        name: "70cm",
        lo_mhz: 430.0,
        hi_mhz: 440.0,
        offsets_mhz: Some(&[1.6, 2.0, 7.6]),
    },
];

impl Band {
    fn contains(&self, mhz: f64) -> bool {
        mhz >= self.lo_mhz && mhz <= self.hi_mhz
    }
}

/// Why a shift was refused.
#[derive(Debug, Clone, PartialEq)]
pub enum ShiftReject {
    NotFinite,
    /// `|shift|` is above [`MAX_SHIFT_MHZ`].
    TooLarge,
    /// The RX frequency is not in a supported amateur band.
    RxOutOfBand,
    /// The band has a standard offset set and the shift is not in it.
    NonStandard {
        band: &'static str,
    },
    /// RX plus shift leaves the amateur band of the RX frequency.
    TxOutOfBand {
        band: &'static str,
    },
}

impl fmt::Display for ShiftReject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite => f.write_str("shift is not a finite number"),
            Self::TooLarge => write!(f, "shift exceeds {MAX_SHIFT_MHZ} MHz"),
            Self::RxOutOfBand => f.write_str("rx frequency is outside supported amateur bands"),
            Self::NonStandard { band } => write!(f, "non-standard {band} offset"),
            Self::TxOutOfBand { band } => write!(f, "tx frequency leaves the {band} band"),
        }
    }
}

/// Check a repeater shift against the band plan. `shift_mhz` is signed:
/// TX = `freq_out_mhz` + `shift_mhz`. Zero (simplex) is always accepted.
///
/// A shift that is unknown (`None` / NaN upstream) is not handled here: callers
/// leave the rig shift untouched instead of validating a guess.
pub fn validate_shift(freq_out_mhz: f64, shift_mhz: f64) -> Result<(), ShiftReject> {
    if !shift_mhz.is_finite() || !freq_out_mhz.is_finite() {
        return Err(ShiftReject::NotFinite);
    }
    let abs = shift_mhz.abs();
    if abs > MAX_SHIFT_MHZ {
        return Err(ShiftReject::TooLarge);
    }
    if abs < OFFSET_TOLERANCE_MHZ {
        return Ok(());
    }
    let Some(band) = BANDS.iter().find(|b| b.contains(freq_out_mhz)) else {
        return Err(ShiftReject::RxOutOfBand);
    };
    if let Some(offsets) = band.offsets_mhz {
        if !offsets
            .iter()
            .any(|o| (abs - o).abs() <= OFFSET_TOLERANCE_MHZ)
        {
            return Err(ShiftReject::NonStandard { band: band.name });
        }
    }
    if !band.contains(freq_out_mhz + shift_mhz) {
        return Err(ShiftReject::TxOutOfBand { band: band.name });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_metre_standard_offset_is_valid() {
        assert!(validate_shift(145.725, -0.6).is_ok());
        assert!(validate_shift(145.0, 0.6).is_ok());
        assert!(validate_shift(145.6625, -0.600_000_000_1).is_ok());
    }

    #[test]
    fn seventy_cm_standard_offsets_are_valid() {
        assert!(validate_shift(434.775, -2.0).is_ok());
        assert!(validate_shift(434.65, -2.0).is_ok());
        assert!(validate_shift(433.0, -1.6).is_ok());
        assert!(validate_shift(439.0, -7.6).is_ok());
        assert_eq!(
            validate_shift(434.0, -7.6),
            Err(ShiftReject::TxOutOfBand { band: "70cm" })
        );
    }

    #[test]
    fn simplex_is_valid() {
        assert!(validate_shift(145.5, 0.0).is_ok());
        assert!(validate_shift(433.5, 0.0).is_ok());
    }

    #[test]
    fn the_la2trr_287_mhz_shift_is_rejected() {
        assert_eq!(
            validate_shift(145.2375, 287.9375),
            Err(ShiftReject::TooLarge)
        );
        assert_eq!(
            validate_shift(145.2375, -287.9375),
            Err(ShiftReject::TooLarge)
        );
    }

    #[test]
    fn non_standard_offsets_are_rejected_not_clamped() {
        assert_eq!(
            validate_shift(145.6, -0.5),
            Err(ShiftReject::NonStandard { band: "2m" })
        );
        assert_eq!(
            validate_shift(145.6, -2.0),
            Err(ShiftReject::NonStandard { band: "2m" })
        );
        assert_eq!(
            validate_shift(434.6, -0.6),
            Err(ShiftReject::NonStandard { band: "70cm" })
        );
    }

    #[test]
    fn tx_must_stay_in_the_rx_band() {
        assert_eq!(
            validate_shift(144.2, -0.6),
            Err(ShiftReject::TxOutOfBand { band: "2m" })
        );
        assert_eq!(
            validate_shift(145.8, 0.6),
            Err(ShiftReject::TxOutOfBand { band: "2m" })
        );
        assert_eq!(
            validate_shift(430.5, -2.0),
            Err(ShiftReject::TxOutOfBand { band: "70cm" })
        );
    }

    #[test]
    fn unsupported_band_and_non_finite_are_rejected() {
        assert_eq!(validate_shift(1296.0, -28.0), Err(ShiftReject::TooLarge));
        assert_eq!(validate_shift(1296.0, -1.0), Err(ShiftReject::RxOutOfBand));
        assert_eq!(validate_shift(145.0, f64::NAN), Err(ShiftReject::NotFinite));
    }

    #[test]
    fn six_metre_fixture_row_is_accepted() {
        // LA2RRR 51.83 MHz / -0.6 MHz in testdata/cat/non_networked.json.
        assert!(validate_shift(51.83, -0.6).is_ok());
    }
}
