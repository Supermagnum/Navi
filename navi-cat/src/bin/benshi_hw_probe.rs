//! Read-only VR-N76 reachability probe over host BlueZ RFCOMM (Step 5).
//!
//! Usage:
//!   cargo run -p navi-cat --bin benshi-hw-probe -- AA:BB:CC:DD:EE:FF [--channel N]
//!
//! Writes raw frames under testdata/cat/hardware/vr-n76/. Never transmits;
//! allowlist refuses non-read commands. Does not change VFO B or selection.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use navi_cat::benshi::allowlist::{is_allowed, GROUP_BASIC};
use navi_cat::benshi::codec::*;
use navi_cat::benshi::gate::gate_native_identity;
#[cfg(target_os = "linux")]
use navi_cat::benshi::{BenshiBackend, BluezRfcommTransport};

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/cat/hardware/vr-n76")
}

fn write_hex(name: &str, bytes: &[u8]) {
    let dir = out_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(name);
    let mut s = format!("# {} bytes\n", bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && i % 16 == 0 {
            s.push('\n');
        }
        s.push_str(&format!("{b:02x} "));
    }
    s.push('\n');
    let _ = fs::write(&path, s);
    eprintln!("wrote {}", path.display());
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("benshi-hw-probe: Linux BlueZ only");
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
fn main() {
    let mut args = std::env::args().skip(1);
    let mac = args.next().unwrap_or_else(|| {
        eprintln!("usage: benshi-hw-probe <MAC> [--channel N]");
        std::process::exit(2);
    });
    let mut force_channel: Option<u8> = None;
    while let Some(a) = args.next() {
        if a == "--channel" {
            force_channel = Some(args.next().and_then(|s| s.parse().ok()).unwrap_or_else(|| {
                eprintln!("--channel needs 0-30");
                std::process::exit(2);
            }));
        }
    }
    eprintln!(
        "connecting BlueZ RFCOMM control to {mac} ({} never audio UUID)...",
        force_channel
            .map(|c| format!("channel {c};"))
            .unwrap_or_else(|| "channels 4,1;".into())
    );
    let t0 = Instant::now();
    let transport = match force_channel {
        Some(ch) => BluezRfcommTransport::connect_channel(&mac, ch),
        None => BluezRfcommTransport::connect(&mac),
    };
    let transport = match transport {
        Ok(t) => {
            eprintln!(
                "connect ok in {:.0} ms (RFCOMM channel {})",
                t0.elapsed().as_secs_f64() * 1000.0,
                t.channel()
            );
            t
        }
        Err(e) => {
            eprintln!("FAIL at connect: {e}");
            std::process::exit(1);
        }
    };

    std::thread::sleep(Duration::from_millis(200));
    let mut backend = BenshiBackend::new(transport, 0);
    backend.set_reply_timeout(Duration::from_secs(3));

    let t_hs = Instant::now();
    match backend.handshake() {
        Ok(()) => eprintln!(
            "handshake (GET_DEV_INFO + REGISTER_NOTIFICATION) ok in {:.0} ms",
            t_hs.elapsed().as_secs_f64() * 1000.0
        ),
        Err(e) => {
            let leftover = backend.take_rx_buf();
            if !leftover.is_empty() {
                eprintln!(
                    "leftover rx_buf ({}): {}",
                    leftover.len(),
                    hex_preview(&leftover)
                );
                write_hex("handshake_fail_rx_buf.hex", &leftover);
            }
            eprintln!(
                "FAIL at handshake/gating after {:.0} ms: {e}",
                t_hs.elapsed().as_secs_f64() * 1000.0
            );
            std::process::exit(1);
        }
    }

    let cmds: &[(&str, u16, Vec<u8>)] = &[
        (
            "GET_DEV_INFO",
            CMD_GET_DEV_INFO,
            GET_DEV_INFO_REQ_PAYLOAD.to_vec(),
        ),
        ("GET_HT_STATUS", CMD_GET_HT_STATUS, vec![]),
        ("READ_SETTINGS", CMD_READ_SETTINGS, vec![]),
        ("FREQ_MODE_GET_STATUS", CMD_FREQ_MODE_GET_STATUS, vec![]),
    ];
    for (name, cmd, data) in cmds {
        assert!(
            is_allowed(GROUP_BASIC, *cmd),
            "{name} must be on never-transmit allowlist"
        );
        let t = Instant::now();
        match backend.send_command_checked(GROUP_BASIC, *cmd, data.clone()) {
            Ok(body) => {
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                eprintln!("{name}: ok round-trip {:.0} ms ({} bytes)", ms, body.len());
                write_hex(&format!("{name}.hex"), &body);
                match *cmd {
                    CMD_GET_DEV_INFO => {
                        if let Ok(info) = DevInfo::from_reply(&body) {
                            let gate =
                                gate_native_identity(Some(info.vendor_id), Some(info.product_id));
                            eprintln!(
                                "  identity vendor=0x{:02x} product=0x{:04x} support_vfo={} support_dmr={} channels={} gate_ok={}",
                                info.vendor_id,
                                info.product_id,
                                info.support_vfo,
                                info.support_dmr,
                                info.channel_count,
                                gate.allowed
                            );
                        }
                    }
                    CMD_GET_HT_STATUS => {
                        if let Ok(st) = HtStatus::from_reply(&body) {
                            eprintln!(
                                "  status tx={} sq_open={} rx={} selected_vfo(doubleChannel)={} scan={} curr_ch={}",
                                st.is_in_tx,
                                st.is_sq,
                                st.is_in_rx,
                                st.double_channel,
                                st.is_scan,
                                st.curr_ch_id
                            );
                        }
                    }
                    CMD_READ_SETTINGS => {
                        if let Ok(s) = SettingsBlock::from_reply(&body) {
                            let fp = s.vfo2_fingerprint();
                            write_hex("VFO_B_fingerprint.hex", &fp);
                            write_hex("READ_SETTINGS_raw.hex", &s.raw_reply);
                            eprintln!(
                                "  channel_a={} channel_b={} double_channel={} vfo2_fp={}",
                                s.channel_a,
                                s.channel_b,
                                s.double_channel,
                                hex_preview(&fp)
                            );
                            for (label, ch) in [("A", s.channel_a), ("B", s.channel_b)] {
                                let tch = Instant::now();
                                match backend.send_command_checked(
                                    GROUP_BASIC,
                                    CMD_READ_RF_CH,
                                    vec![ch],
                                ) {
                                    Ok(ch_body) => {
                                        eprintln!(
                                            "  READ_RF_CH {label} (id={ch}): {:.0} ms",
                                            tch.elapsed().as_secs_f64() * 1000.0
                                        );
                                        write_hex(&format!("READ_RF_CH_{label}.hex"), &ch_body);
                                        if let Ok(rec) = ChannelRecord::from_reply(&ch_body) {
                                            eprintln!(
                                                "    VFO {label}: RX={:.4} MHz  TX={:.4} MHz",
                                                rec.rx_freq_hz as f64 / 1e6,
                                                rec.tx_freq_hz as f64 / 1e6
                                            );
                                        }
                                    }
                                    Err(e) => eprintln!("  READ_RF_CH {label}: {e}"),
                                }
                            }
                        }
                    }
                    CMD_FREQ_MODE_GET_STATUS => {
                        eprintln!("  FREQ_MODE_GET_STATUS raw saved (layout = Q3)");
                    }
                    _ => {}
                }
            }
            Err(e) => {
                eprintln!(
                    "{name}: FAIL ({:.0} ms): {e}",
                    t.elapsed().as_secs_f64() * 1000.0
                );
                if *cmd == CMD_FREQ_MODE_GET_STATUS {
                    eprintln!("  (optional; continue)");
                    continue;
                }
                std::process::exit(1);
            }
        }
    }

    eprintln!(
        "VFO 2 fingerprint bytes saved; selected VFO left untouched. Probe complete in {:.0} ms total.",
        t0.elapsed().as_secs_f64() * 1000.0
    );
}
