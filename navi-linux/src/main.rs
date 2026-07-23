//! Linux sensor bring-up: gpsd position + optional IMU heading → SensorBus.
//!
//! There is no desktop MapLibre UI; this binary proves the highest-priority
//! sensor tier can ingest real hardware data for Compass / Direction-of-travel
//! rotation modes (see docs/build-linux.md, architecture.md).

use std::env;
use std::thread;
use std::time::Duration;

use driver_break_core::sensors::SensorBus;

fn main() -> anyhow::Result<()> {
    let mut gpsd_addr = "127.0.0.1:2947".to_string();
    let mut demo_imu = false;
    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--gpsd" => {
                i += 1;
                gpsd_addr = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--gpsd needs host:port"))?;
            }
            "--demo-imu" => demo_imu = true,
            other => anyhow::bail!("unknown arg: {other}"),
        }
        i += 1;
    }

    let bus = SensorBus::new();

    #[cfg(feature = "linux-imu")]
    if demo_imu {
        let bus_imu = bus.clone();
        thread::spawn(move || {
            let mut heading = 0.0_f64;
            loop {
                driver_break_core::sensors::linux_imu::publish_imu(&bus_imu, heading, 0.0, 0.0);
                heading = (heading + 5.0).rem_euclid(360.0);
                thread::sleep(Duration::from_millis(200));
            }
        });
    }

    #[cfg(feature = "gpsd")]
    {
        let bus_gps = bus.clone();
        let addr = gpsd_addr.clone();
        println!("Connecting to gpsd at {addr} (WATCH json)…");
        thread::spawn(move || {
            if let Err(e) = driver_break_core::sensors::gpsd::run_gpsd_loop(&addr, &bus_gps) {
                eprintln!("gpsd loop ended: {e:#}");
            }
        });
    }

    #[cfg(not(feature = "gpsd"))]
    {
        let _ = gpsd_addr;
        eprintln!("Rebuild with --features gpsd to enable gpsd");
    }

    println!("Polling SensorBus (Ctrl-C to stop). Compass uses IMU heading; Travel uses GPS course.");
    loop {
        if let Some(p) = bus.latest_position() {
            println!(
                "POS lat={:.5} lon={:.5} alt={:?} course={:.1}° speed={:.2} m/s sats={:?} eph={:?}",
                p.lat, p.lon, p.altitude_m, p.course_deg, p.speed_m_s, p.satellites_used, p.horizontal_accuracy_m
            );
        }
        if let Some(imu) = bus.latest_imu() {
            println!(
                "IMU heading={:.1}° pitch={:.1}° roll={:.1}°",
                imu.heading_deg, imu.pitch_deg, imu.roll_deg
            );
        }
        thread::sleep(Duration::from_secs(1));
    }
}
