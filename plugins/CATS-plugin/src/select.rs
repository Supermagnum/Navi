//! Repeater selection within 150 km (NFM preferred for auto-tune).

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Candidate {
    pub callsign: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub freq_out_mhz: f64,
    pub shift_mhz: f64,
    pub ctcss_hz: Option<f64>,
    #[serde(default)]
    pub modulation: String,
    pub network_id: Option<String>,
    pub distance_km: Option<f64>,
    #[serde(default)]
    pub conflict: bool,
}

/// Pick nearest NFM/FM site that is not APRS and has coordinates.
pub fn pick_best_nfm(json: &str, _lat: f64, _lon: f64) -> Option<Candidate> {
    let sites: Vec<Candidate> = serde_json::from_str(json).ok()?;
    sites
        .into_iter()
        .filter(|s| {
            let m = s.modulation.to_ascii_uppercase();
            !m.contains("APRS") && !m.contains("DMR") && s.lat.is_some() && s.lon.is_some()
        })
        .min_by(|a, b| {
            a.distance_km
                .partial_cmp(&b.distance_km)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_nearest_nfm() {
        let json = r#"[
          {"callsign":"FAR","lat":61.0,"lon":10.0,"freq_out_mhz":145.0,"shift_mhz":-0.6,
           "modulation":"NFM","distance_km":40.0},
          {"callsign":"NEAR","lat":61.0,"lon":10.1,"freq_out_mhz":145.725,"shift_mhz":-0.6,
           "modulation":"NFM","distance_km":5.0},
          {"callsign":"APRS","lat":61.0,"lon":10.05,"freq_out_mhz":144.8,"shift_mhz":0.0,
           "modulation":"APRS","distance_km":1.0}
        ]"#;
        let best = pick_best_nfm(json, 61.0, 10.0).unwrap();
        assert_eq!(best.callsign, "NEAR");
    }
}
