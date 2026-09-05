//! Approximate Nordic / Arctic coverage for MET Norway Locationforecast priority.

/// True when the fix is inside the Nordic/Arctic domain where MET Norway
/// short-range products are strongest. Outside this box, Open-Meteo is tried
/// first.
///
/// Bounds cover Scandinavia, Finland, Iceland, Svalbard, and adjacent Arctic
/// seas (not a legal jurisdiction polygon).
pub fn in_nordic_arctic(lat: f64, lon: f64) -> bool {
    (54.0..=82.0).contains(&lat) && (-30.0..=45.0).contains(&lon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oslo_is_nordic() {
        assert!(in_nordic_arctic(59.9139, 10.7522));
    }

    #[test]
    fn tokyo_is_not() {
        assert!(!in_nordic_arctic(35.6762, 139.6503));
    }

    #[test]
    fn svalbard_is_nordic() {
        assert!(in_nordic_arctic(78.2232, 15.6267));
    }
}
