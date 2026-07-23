//! gpsd client via [`gpsd_proto`] (TCP JSON, no libgps).
//!
//! Chosen over `gpsd_client` because `gpsd_proto` is the lower-level, widely
//! used C-library-free protocol crate (`handshake` + `get_data`) and matches
//! this project's preference for pure-Rust cross-compilation.
//!
//! Handshake uses `ENABLE_WATCH_CMD` (`?WATCH={"enable":true,"json":true};`).
//! Publishes TPV (position/speed/track) and uses SKY for satellite counts.

use std::io::{BufReader, BufWriter};
use std::net::TcpStream;
use std::time::{SystemTime, UNIX_EPOCH};

use gpsd_proto::{get_data, handshake, Mode, ResponseData};

use super::{PositionSample, SensorBus};

/// Connect to gpsd, enable JSON watch, and publish TPV/SKY-derived samples.
///
/// Blocks the calling thread; run on the sensor tier. Returns when the socket
/// closes or an I/O error occurs.
pub fn run_gpsd_loop(addr: &str, bus: &SensorBus) -> anyhow::Result<()> {
    let stream = TcpStream::connect(addr)?;
    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);
    handshake(&mut reader, &mut writer).map_err(|e| anyhow::anyhow!("gpsd handshake: {e}"))?;

    let mut sats: Option<u32> = None;
    loop {
        let msg = get_data(&mut reader).map_err(|e| anyhow::anyhow!("gpsd get_data: {e}"))?;
        match msg {
            ResponseData::Sky(sky) => {
                sats = sky.satellites.as_ref().map(|list| {
                    list.iter().filter(|s| s.used).count() as u32
                });
            }
            ResponseData::Tpv(t) => {
                if matches!(t.mode, Mode::NoFix) {
                    continue;
                }
                let (Some(lat), Some(lon)) = (t.lat, t.lon) else {
                    continue;
                };
                let sample = PositionSample {
                    lat: lat as f64,
                    lon: lon as f64,
                    altitude_m: t.alt.map(|a| a as f64),
                    speed_m_s: t.speed.unwrap_or(0.0) as f64,
                    course_deg: t.track.unwrap_or(0.0) as f64,
                    timestamp_unix: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    horizontal_accuracy_m: t.eph.map(|e| e as f64),
                    satellites_used: sats,
                };
                bus.publish_position(sample);
            }
            _ => {}
        }
    }
}

/// Map a deserialized TPV into a [`PositionSample`] (tests / injection).
pub fn sample_from_tpv(
    lat: f64,
    lon: f64,
    alt: Option<f64>,
    track: Option<f64>,
    speed: Option<f64>,
    eph: Option<f64>,
) -> PositionSample {
    PositionSample {
        lat,
        lon,
        altitude_m: alt,
        speed_m_s: speed.unwrap_or(0.0),
        course_deg: track.unwrap_or(0.0),
        timestamp_unix: 0,
        horizontal_accuracy_m: eph,
        satellites_used: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_from_tpv_fields() {
        let s = sample_from_tpv(60.3913, 5.3221, Some(12.5), Some(45.0), Some(8.2), Some(4.0));
        assert!((s.lat - 60.3913).abs() < 1e-6);
        assert!((s.course_deg - 45.0).abs() < 1e-6);
        assert_eq!(s.horizontal_accuracy_m, Some(4.0));
    }
}
