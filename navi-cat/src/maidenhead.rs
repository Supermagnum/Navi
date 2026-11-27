//! Maidenhead grid locator → approximate lat/lon (square centre).
//!
//! Used as a fallback position signal for repeater rows that only carry a
//! locator string. Invalid input returns `None` (never panics).

/// Decode a Maidenhead locator (2–8 characters, even length) to the centre of
/// the square as `(lat_deg, lon_deg)`.
///
/// Accepts case-insensitive letters. Field letters outside A–R, square digits
/// outside 0–9, and subsquare letters outside A–X are rejected.
pub fn maidenhead_to_latlon(locator: &str) -> Option<(f64, f64)> {
    let s: String = locator
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    let bytes = s.as_bytes();
    let n = bytes.len();
    if n < 2 || n > 8 || n % 2 != 0 {
        return None;
    }

    let mut lon = -180.0_f64;
    let mut lat = -90.0_f64;

    // Field: 20° lon × 10° lat (A–R)
    let lon_field = field_letter(bytes[0])?;
    let lat_field = field_letter(bytes[1])?;
    lon += f64::from(lon_field) * 20.0;
    lat += f64::from(lat_field) * 10.0;
    let mut lon_span = 20.0;
    let mut lat_span = 10.0;

    if n >= 4 {
        let lon_sq = digit(bytes[2])?;
        let lat_sq = digit(bytes[3])?;
        lon_span = 2.0;
        lat_span = 1.0;
        lon += f64::from(lon_sq) * lon_span;
        lat += f64::from(lat_sq) * lat_span;
    }

    if n >= 6 {
        let lon_sub = subsquare_letter(bytes[4])?;
        let lat_sub = subsquare_letter(bytes[5])?;
        lon_span = 2.0 / 24.0;
        lat_span = 1.0 / 24.0;
        lon += f64::from(lon_sub) * lon_span;
        lat += f64::from(lat_sub) * lat_span;
    }

    if n >= 8 {
        let lon_ext = digit(bytes[6])?;
        let lat_ext = digit(bytes[7])?;
        lon_span /= 10.0;
        lat_span /= 10.0;
        lon += f64::from(lon_ext) * lon_span;
        lat += f64::from(lat_ext) * lat_span;
    }

    // Centre of the resolved square.
    lon += lon_span / 2.0;
    lat += lat_span / 2.0;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    Some((lat, lon))
}

fn field_letter(b: u8) -> Option<u8> {
    if (b'A'..=b'R').contains(&b) {
        Some(b - b'A')
    } else {
        None
    }
}

fn digit(b: u8) -> Option<u8> {
    if (b'0'..=b'9').contains(&b) {
        Some(b - b'0')
    } else {
        None
    }
}

fn subsquare_letter(b: u8) -> Option<u8> {
    if (b'A'..=b'X').contains(&b) {
        Some(b - b'A')
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jo59_is_southern_norway_ish() {
        let (lat, lon) = maidenhead_to_latlon("JO59").unwrap();
        assert!((59.0..60.0).contains(&lat), "lat={lat}");
        assert!((10.0..12.0).contains(&lon), "lon={lon}");
    }

    #[test]
    fn rejects_garbage() {
        assert!(maidenhead_to_latlon("").is_none());
        assert!(maidenhead_to_latlon("J").is_none());
        assert!(maidenhead_to_latlon("ZZ99").is_none());
        assert!(maidenhead_to_latlon("JO5").is_none());
    }
}
