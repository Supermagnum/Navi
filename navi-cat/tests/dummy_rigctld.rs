//! Integration test against Hamlib dummy `rigctld -m 1` when available.

use navi_cat::program::{program_vfo1_verified, ProgramRequest};
use navi_cat::tcp::TcpRigBackend;
use navi_cat::types::{RigBackend, ShiftDir};
use std::net::TcpListener;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

fn which_rigctld() -> bool {
    Command::new("which")
        .arg("rigctld")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct RigctldGuard(Child);

impl Drop for RigctldGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_dummy(port: u16) -> Option<RigctldGuard> {
    let child = Command::new("rigctld")
        .args(["-m", "1", "-t", &port.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Some(RigctldGuard(child));
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = RigctldGuard(child);
    None
}

#[test]
fn dummy_rigctld_program_readback_and_never_transmit() {
    if !which_rigctld() {
        eprintln!("skipping: rigctld not found on PATH");
        return;
    }
    let port = free_port();
    let Some(_guard) = spawn_dummy(port) else {
        eprintln!("skipping: could not start rigctld");
        return;
    };

    let mut backend =
        TcpRigBackend::connect_with_options("127.0.0.1", port, false, true).expect("connect primary");
    assert!(
        backend.gate().allowed,
        "dummy should be allowed when allow_dummy=true: {}",
        backend.gate().reason
    );

    let req = ProgramRequest {
        freq_out_mhz: 145.725,
        shift_mhz: -0.6,
        ctcss_hz: Some(88.5),
        dcs_code: None,
        mode: "FM".into(),
        passband_hz: 12_500,
        vfo: "VFOA".into(),
        leaves_follow: false,
    };
    let reported = program_vfo1_verified(&mut backend, &req).expect("program");
    assert_eq!(reported.freq_hz, 145_725_000);
    assert_eq!(reported.shift, ShiftDir::Minus);
    assert_eq!(reported.offset_hz, 600_000);
    assert_eq!(reported.ctcss_tenths_hz, Some(885));

    // Independent second connection asserts the same state.
    let mut other =
        TcpRigBackend::connect_with_options("127.0.0.1", port, false, true).expect("connect secondary");
    let state = other.read_vfo_state().expect("read secondary");
    assert_eq!(state.freq_hz, 145_725_000);
    assert_eq!(state.shift, ShiftDir::Minus);
    assert_eq!(state.offset_hz, 600_000);
    assert_eq!(state.ctcss_tenths_hz, Some(885));

    for cmd in backend.sent_commands() {
        let t = cmd.trim_start_matches('+').trim();
        assert!(
            !(t == "T" || t.starts_with("T ") || t.eq_ignore_ascii_case("set_ptt")),
            "must never send transmit command, got {cmd}"
        );
    }

    let err = backend.transact_raw("T 1");
    assert!(
        matches!(err, Err(navi_cat::RigError::Unsupported(_))),
        "T must be refused: {err:?}"
    );
}
