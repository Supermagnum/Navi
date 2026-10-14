//! Step 6: 5-second VFO A demo on real VR-N76 (never transmit; VFO B untouched).
//!
//! One RFCOMM session for the whole invocation — do not wrap with bluetoothctl
//! connect/disconnect churn.
//!
//! Usage:
//!   cargo run -p navi-cat --bin benshi-hw-demo -- <MAC> [--select-vfo-a] \
//!       espa-dombas osterdalen [--restore-step5]
//!   cargo run -p navi-cat --bin benshi-hw-demo -- <MAC> select-vfo-a
//!   cargo run -p navi-cat --bin benshi-hw-demo -- <MAC> restore-step5
//!
//! `select-vfo-a` is operator-requested display prep only (not product auto-tune).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
use navi_cat::benshi::{BenshiBackend, BluezRfcommTransport, TuningMethod};
#[cfg(target_os = "linux")]
use navi_cat::program::{program_vfo1_verified, ProgramRequest};

#[derive(Clone)]
struct Step {
    callsign: &'static str,
    site_id: &'static str,
    freq_out_mhz: f64,
    shift_mhz: f64,
    ctcss_hz: f64,
}

fn route_steps(name: &str) -> Option<&'static [Step]> {
    const ESPA_DOMBAS: &[Step] = &[
        Step {
            callsign: "LA5MR",
            site_id: "osm-node-1473491257",
            freq_out_mhz: 145.6250,
            shift_mhz: -0.6,
            ctcss_hz: 88.5,
        },
        Step {
            callsign: "LA6JRR",
            site_id: "osm-way-1194875539",
            freq_out_mhz: 145.6625,
            shift_mhz: -0.6,
            ctcss_hz: 88.5,
        },
    ];
    const OSTERDALEN: &[Step] = &[
        Step {
            callsign: "LA5MR",
            site_id: "osm-node-1473491257",
            freq_out_mhz: 145.6250,
            shift_mhz: -0.6,
            ctcss_hz: 88.5,
        },
        Step {
            callsign: "LA5TRR",
            site_id: "osm-node-5576656416",
            freq_out_mhz: 145.7250,
            shift_mhz: -0.6,
            ctcss_hz: 88.5,
        },
        Step {
            callsign: "LA9AR",
            site_id: "osm-way-1005486272",
            freq_out_mhz: 145.6000,
            shift_mhz: -0.6,
            ctcss_hz: 88.5,
        },
    ];
    match name {
        "espa-dombas" | "espa_dombas" | "h" => Some(ESPA_DOMBAS),
        "osterdalen" | "litjfossen" | "i" => Some(OSTERDALEN),
        _ => None,
    }
}

/// Step 5 channel-A face before demos (RX 434.6500 / TX 432.6500).
fn restore_step5_req() -> ProgramRequest {
    ProgramRequest {
        freq_out_mhz: 434.6500,
        shift_mhz: -2.0,
        ctcss_hz: None,
        dcs_code: None,
        mode: "FM".into(),
        passband_hz: 12_500,
        vfo: "VFOA".into(),
        leaves_follow: false,
    }
}

/// Operator leave: LA5MR 145.625000 (−0.6 / 88.5).
fn leave_la5mr_req() -> ProgramRequest {
    ProgramRequest {
        freq_out_mhz: 145.6250,
        shift_mhz: -0.6,
        ctcss_hz: Some(88.5),
        dcs_code: None,
        mode: "FM".into(),
        passband_hz: 12_500,
        vfo: "VFOA".into(),
        leaves_follow: false,
    }
}

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/cat/hardware/vr-n76")
}

fn now_stamp() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{ms}")
}

fn log_line(log: &mut impl Write, line: &str) {
    eprintln!("{line}");
    let _ = writeln!(log, "{line}");
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("benshi-hw-demo: Linux BlueZ only");
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
fn program_with_deferral(
    backend: &mut BenshiBackend<BluezRfcommTransport>,
    req: &ProgramRequest,
    log: &mut impl Write,
) -> Result<(String, String), String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match program_vfo1_verified(backend, req) {
            Ok(reported) => {
                let method = match backend.last_tuning_method() {
                    Some(TuningMethod::ScratchChannel) => "scratch_channel",
                    Some(TuningMethod::FrequencyMode) => "frequency_mode",
                    None => "unknown",
                };
                let readback = format!(
                    "RX={:.4} MHz shift={:?} offset_hz={} ctcss_tenths={:?}",
                    reported.freq_hz as f64 / 1e6,
                    reported.shift,
                    reported.offset_hz,
                    reported.ctcss_tenths_hz
                );
                return Ok((method.into(), readback));
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("deferred") || msg.contains("squelch") || msg.contains("PTT") {
                    log_line(log, &format!("  DEFER (wait, not skip): {msg}"));
                    if Instant::now() >= deadline {
                        return Err(format!("deferral timeout: {msg}"));
                    }
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }
                return Err(msg);
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run_route(
    backend: &mut BenshiBackend<BluezRfcommTransport>,
    route: &str,
    steps: &[Step],
    vfo2_before: &[u8],
    log: &mut impl Write,
) -> Result<(), String> {
    log_line(
        log,
        &format!(
            "--- route={route} steps={} duration≈{}s (same RFCOMM session) ---",
            steps.len(),
            steps.len() * 5
        ),
    );
    let run_start = Instant::now();
    for (i, step) in steps.iter().enumerate() {
        let target = run_start + Duration::from_secs((i as u64) * 5);
        let now = Instant::now();
        if now < target {
            thread::sleep(target - now);
        }
        let req = ProgramRequest {
            freq_out_mhz: step.freq_out_mhz,
            shift_mhz: step.shift_mhz,
            ctcss_hz: Some(step.ctcss_hz),
            dcs_code: None,
            mode: "FM".into(),
            passband_hz: 12_500,
            vfo: "VFOA".into(),
            leaves_follow: false,
        };
        log_line(
            log,
            &format!(
                "t≈{}s STEP {} {} ({}) request RX={:.4} shift={:+.4} CTCSS={}",
                run_start.elapsed().as_secs(),
                i + 1,
                step.callsign,
                step.site_id,
                step.freq_out_mhz,
                step.shift_mhz,
                step.ctcss_hz
            ),
        );
        let (method, readback) = program_with_deferral(backend, &req, log)?;
        log_line(log, &format!("  OK method={method} read-back: {readback}"));
        let fp = backend
            .refresh_vfo2_fingerprint()
            .map_err(|e| format!("fingerprint: {e}"))?;
        if fp != vfo2_before {
            return Err("VFO B fingerprint changed mid-run; leaving radio as-is".into());
        }
    }
    log_line(
        log,
        &format!(
            "route {route} done wall={:.1}s",
            run_start.elapsed().as_secs_f64()
        ),
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: benshi-hw-demo <MAC> [--select-vfo-a] <route...> [--restore-step5]\n\
             routes: espa-dombas osterdalen | select-vfo-a | restore-step5"
        );
        std::process::exit(2);
    }
    let mac = args.remove(0);
    let mut select_vfo_a = false;
    let mut restore = false;
    let mut leave_la5mr = false;
    let mut routes: Vec<String> = Vec::new();
    for a in args {
        match a.as_str() {
            "--select-vfo-a" | "select-vfo-a" | "select_vfo_a" => select_vfo_a = true,
            "--restore-step5" | "restore-step5" | "restore_step5" => restore = true,
            "--leave-la5mr" | "leave-la5mr" | "leave-145.625" | "leave_145_625" => {
                leave_la5mr = true
            }
            other => {
                if route_steps(other).is_some() {
                    routes.push(other.to_string());
                } else {
                    eprintln!("unknown arg: {other}");
                    std::process::exit(2);
                }
            }
        }
    }
    if routes.is_empty() && !select_vfo_a && !restore && !leave_la5mr {
        eprintln!("need at least one route, select-vfo-a, leave-la5mr, or restore-step5");
        std::process::exit(2);
    }

    let dir = out_dir();
    let _ = fs::create_dir_all(&dir);
    let label = if !routes.is_empty() {
        routes.join("+")
    } else if leave_la5mr {
        "leave-la5mr".into()
    } else if restore {
        "restore-step5".into()
    } else {
        "select-vfo-a".into()
    };
    let log_path = dir.join(format!("STEP6_{label}_{}.log", now_stamp()));
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("log");
    log_line(
        &mut log,
        &format!(
            "Step 6 single-session mac={mac} select_vfo_a={select_vfo_a} routes={routes:?} leave_la5mr={leave_la5mr} restore={restore}"
        ),
    );
    log_line(
        &mut log,
        "note: one RFCOMM connect for entire run; no bluetoothctl churn",
    );

    eprintln!("connecting once…");
    let transport = match BluezRfcommTransport::connect(&mac) {
        Ok(t) => {
            log_line(
                &mut log,
                &format!(
                    "connect ok RFCOMM channel {} (held for session)",
                    t.channel()
                ),
            );
            t
        }
        Err(e) => {
            log_line(&mut log, &format!("FAIL connect: {e}"));
            std::process::exit(1);
        }
    };
    let mut backend = BenshiBackend::new(transport, 0);
    backend.set_reply_timeout(Duration::from_secs(3));
    thread::sleep(Duration::from_millis(200));
    if let Err(e) = backend.handshake() {
        log_line(&mut log, &format!("FAIL handshake: {e}"));
        std::process::exit(1);
    }

    if select_vfo_a {
        match backend.select_vfo_a_for_display() {
            Ok(()) => log_line(
                &mut log,
                "selected VFO A (doubleChannel=1); channel B content unchanged",
            ),
            Err(e) => {
                log_line(&mut log, &format!("FAIL select VFO A: {e}"));
                std::process::exit(1);
            }
        }
    }

    let vfo2_before = match backend.refresh_vfo2_fingerprint() {
        Ok(fp) => fp,
        Err(e) => {
            log_line(&mut log, &format!("FAIL fingerprint: {e}"));
            std::process::exit(1);
        }
    };
    log_line(
        &mut log,
        &format!(
            "VFO B fingerprint session start: {}",
            vfo2_before
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    );

    for route in &routes {
        let steps = route_steps(route).unwrap();
        if let Err(e) = run_route(&mut backend, route, steps, &vfo2_before, &mut log) {
            log_line(&mut log, &format!("ABORT: {e}"));
            std::process::exit(1);
        }
    }

    if leave_la5mr {
        log_line(
            &mut log,
            "--- leave channel A on LA5MR 145.625000 (−0.6 / 88.5) ---",
        );
        match program_with_deferral(&mut backend, &leave_la5mr_req(), &mut log) {
            Ok((method, readback)) => {
                log_line(
                    &mut log,
                    &format!("  leave OK method={method} read-back: {readback}"),
                );
            }
            Err(e) => {
                log_line(&mut log, &format!("ABORT leave: {e}"));
                std::process::exit(1);
            }
        }
        let fp = backend.refresh_vfo2_fingerprint().unwrap_or_default();
        if fp != vfo2_before {
            log_line(&mut log, "ABORT: VFO B fingerprint changed during leave");
            std::process::exit(1);
        }
    }

    if restore {
        log_line(
            &mut log,
            "--- restore channel A to Step 5 (434.6500 / −2.0) ---",
        );
        match program_with_deferral(&mut backend, &restore_step5_req(), &mut log) {
            Ok((method, readback)) => {
                log_line(
                    &mut log,
                    &format!("  restore OK method={method} read-back: {readback}"),
                );
            }
            Err(e) => {
                log_line(&mut log, &format!("ABORT restore: {e}"));
                std::process::exit(1);
            }
        }
        let fp = backend.refresh_vfo2_fingerprint().unwrap_or_default();
        if fp != vfo2_before {
            log_line(&mut log, "ABORT: VFO B fingerprint changed during restore");
            std::process::exit(1);
        }
    }

    let vfo2_after = backend.refresh_vfo2_fingerprint().unwrap_or_default();
    let identical = vfo2_after == vfo2_before;
    log_line(
        &mut log,
        &format!(
            "VFO B fingerprint session end identical={identical}: {}",
            vfo2_after
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    );
    if !identical {
        log_line(&mut log, "ABORT: VFO B state not byte-identical");
        std::process::exit(1);
    }
    log_line(
        &mut log,
        &format!("DONE single-session log={}", log_path.display()),
    );
}
