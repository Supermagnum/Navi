//! Linux desktop map shell (Option B: WebKitGTK + MapLibre GL JS).
//!
//! Reuses `driver-break-core` / `navi-ffi` for routing and search, and the same
//! `SensorBus` gpsd / demo-IMU path as `navi-linux`.

mod basemap;
mod server;

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use driver_break_core::sensors::SensorBus;

use server::{default_pmtiles_dirs, AppState};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().skip(1).collect();
    let mut data_dir = default_data_dir();
    let mut pbf_path: Option<PathBuf> = None;
    let mut elev_dir: Option<PathBuf> = None;
    let mut cache_dir: Option<PathBuf> = None;
    let mut place_index: Option<PathBuf> = None;
    let mut explicit_pmtiles: Option<PathBuf> = None;
    let mut force_online = false;
    let mut gpsd_addr = "127.0.0.1:2947".to_string();
    let mut demo_imu = false;
    let mut listen = "127.0.0.1:0".to_string();
    let mut open_browser = false;
    let mut no_webview = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--data-dir" => {
                i += 1;
                data_dir = PathBuf::from(args.get(i).context("--data-dir needs path")?);
            }
            "--pbf" => {
                i += 1;
                pbf_path = Some(PathBuf::from(args.get(i).context("--pbf needs path")?));
            }
            "--elev-dir" => {
                i += 1;
                elev_dir = Some(PathBuf::from(args.get(i).context("--elev-dir needs path")?));
            }
            "--cache-dir" => {
                i += 1;
                cache_dir = Some(PathBuf::from(
                    args.get(i).context("--cache-dir needs path")?,
                ));
            }
            "--place-index" => {
                i += 1;
                place_index = Some(PathBuf::from(
                    args.get(i).context("--place-index needs path")?,
                ));
            }
            "--pmtiles" => {
                i += 1;
                explicit_pmtiles =
                    Some(PathBuf::from(args.get(i).context("--pmtiles needs path")?));
            }
            "--force-online" => force_online = true,
            "--gpsd" => {
                i += 1;
                gpsd_addr = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--gpsd needs host:port"))?;
            }
            "--demo-imu" => demo_imu = true,
            "--listen" => {
                i += 1;
                listen = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--listen needs host:port"))?;
            }
            "--browser" => open_browser = true,
            "--no-webview" => no_webview = true,
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => anyhow::bail!("unknown arg: {other} (try --help)"),
        }
        i += 1;
    }

    std::fs::create_dir_all(&data_dir)?;
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/target/integration-fixtures");
    let pbf_path = pbf_path.unwrap_or_else(|| {
        let a = data_dir.join("ostlandet-latest.osm.pbf");
        if a.is_file() {
            a
        } else {
            fixtures.join("ostlandet-latest.osm.pbf")
        }
    });
    let elev_dir = elev_dir.unwrap_or_else(|| {
        let a = data_dir.join("elevation");
        if a.is_dir() {
            a
        } else {
            fixtures.join("elevation")
        }
    });
    let cache_dir = cache_dir.unwrap_or_else(|| data_dir.join("graph-cache"));
    std::fs::create_dir_all(&cache_dir)?;

    let icons_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/src/icons");

    let bus = SensorBus::new();
    start_sensors(&bus, &gpsd_addr, demo_imu);

    let rt = tokio::runtime::Runtime::new()?;
    let listen_addr: SocketAddr = listen.parse().context("parse --listen")?;
    let local_origin = Arc::new(Mutex::new(format!("http://{listen_addr}")));
    let state = AppState {
        bus: bus.clone(),
        data_dir: data_dir.clone(),
        pbf_path: pbf_path.clone(),
        elev_dir,
        cache_dir,
        place_index,
        explicit_pmtiles: explicit_pmtiles.clone(),
        force_online,
        icons_dir,
        pmtiles_dirs: Arc::new(default_pmtiles_dirs(&data_dir)),
        route: Arc::new(Mutex::new(None)),
        local_origin: local_origin.clone(),
    };

    let bound = rt.block_on(server::serve(state, listen_addr))?;
    let origin = format!("http://{bound}");
    if let Ok(mut g) = local_origin.lock() {
        *g = origin.clone();
    }

    println!("navi-desktop listening at {origin}");
    println!("  PBF: {}", pbf_path.display());
    if let Some(p) = &explicit_pmtiles {
        println!("  PMTiles: {}", p.display());
    }
    println!("  Sensors: gpsd={gpsd_addr} demo_imu={demo_imu}");

    if open_browser || no_webview || !cfg!(feature = "embedded-webview") {
        let _ = open_system_browser(&origin);
        println!("UI opened in the system browser (or open {origin} manually).");
        println!("Ctrl-C to stop.");
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
    }

    #[cfg(feature = "embedded-webview")]
    {
        run_webview(&origin)?;
        return Ok(());
    }

    #[cfg(not(feature = "embedded-webview"))]
    {
        let _ = open_system_browser(&origin);
        println!("Built without embedded-webview; using system browser.");
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
    }
}

fn default_data_dir() -> PathBuf {
    env::var_os("NAVI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs_next_home()
                .map(|h| h.join(".local/share/navi"))
                .unwrap_or_else(|| PathBuf::from("./navi-data"))
        })
}

fn dirs_next_home() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn start_sensors(bus: &SensorBus, gpsd_addr: &str, demo_imu: bool) {
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
        let addr = gpsd_addr.to_string();
        println!("Connecting to gpsd at {addr} (WATCH json)…");
        thread::spawn(move || {
            if let Err(e) = driver_break_core::sensors::gpsd::run_gpsd_loop(&addr, &bus_gps) {
                eprintln!("gpsd loop ended: {e:#}");
            }
        });
    }

    #[cfg(not(feature = "gpsd"))]
    {
        let _ = (bus, gpsd_addr);
        eprintln!("Rebuild with --features gpsd to enable gpsd");
    }

    #[cfg(not(feature = "linux-imu"))]
    let _ = demo_imu;
}

fn open_system_browser(url: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("xdg-open").arg(url).status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => anyhow::bail!("xdg-open exited {s}"),
        Err(e) => Err(e.into()),
    }
}

#[cfg(feature = "embedded-webview")]
fn run_webview(url: &str) -> anyhow::Result<()> {
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Navi (Linux desktop)")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 800.0))
        .build(&event_loop)?;

    let _webview = WebViewBuilder::new().with_url(url).build(&window)?;

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

fn print_help() {
    eprintln!(
        "\
navi-desktop — Linux map shell (MapLibre GL JS in WebKitGTK / browser)

USAGE:
  cargo run -p navi-desktop -- [OPTIONS]

OPTIONS:
  --data-dir DIR       App data (default: $NAVI_DATA_DIR or ~/.local/share/navi)
  --pbf PATH           OSM extract for routing (default: Ostlandet under data-dir or fixtures)
  --elev-dir DIR       Elevation cache directory
  --cache-dir DIR      Graph cache directory
  --place-index PATH   FTS5 place index DB for search
  --pmtiles PATH       Force offline Protomaps basemap file
  --force-online       Always use OpenFreeMap Liberty
  --gpsd HOST:PORT     gpsd TCP address (default 127.0.0.1:2947)
  --demo-imu           Synthetic IMU heading (no hardware)
  --listen HOST:PORT   Local HTTP bind (default 127.0.0.1:0)
  --browser            Open system browser instead of embedded WebKit
  --no-webview         Same as --browser
  -h, --help           Show this help
"
    );
}
