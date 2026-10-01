//! Own rigctld TCP client (extended `+` protocol, `RPRT n` parsing).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::gating::{gate_from_dump_caps, GateDecision};
use crate::types::{RigBackend, RigError, ShiftDir, VfoState};

pub struct TcpRigBackend {
    stream: Option<TcpStream>,
    #[allow(dead_code)]
    host: String,
    #[allow(dead_code)]
    port: u16,
    dump_caps: String,
    gate: GateDecision,
    model: i32,
    model_name: String,
    allow_beta: bool,
    allow_dummy: bool,
    sent_log: Vec<String>,
}

impl TcpRigBackend {
    pub fn connect(host: &str, port: u16, allow_beta: bool) -> Result<Self, RigError> {
        Self::connect_with_options(host, port, allow_beta, cfg!(test) || cfg!(feature = "allow-dummy-rig"))
    }

    /// Like [`connect`], but control whether Hamlib dummy (model 1) may pass gating.
    /// Integration tests must pass `allow_dummy=true` (`cfg(test)` is false in the lib crate).
    pub fn connect_with_options(
        host: &str,
        port: u16,
        allow_beta: bool,
        allow_dummy: bool,
    ) -> Result<Self, RigError> {
        let addr = format!("{host}:{port}");
        let stream = TcpStream::connect(&addr).map_err(|e| RigError::Io(e.to_string()))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .ok();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .ok();
        let mut s = Self {
            stream: Some(stream),
            host: host.into(),
            port,
            dump_caps: String::new(),
            gate: GateDecision {
                allowed: false,
                status: crate::gating::GateStatus::Unknown,
                can_set_rptr_shift: false,
                can_set_rptr_offs: false,
                can_set_ctcss: false,
                reason: "not gated yet".into(),
                beta_override_warning: None,
            },
            model: 0,
            model_name: "unknown".into(),
            allow_beta,
            allow_dummy,
            sent_log: Vec::new(),
        };
        s.refresh_gate()?;
        Ok(s)
    }

    /// Commands sent (for never-transmit assertions).
    pub fn sent_commands(&self) -> &[String] {
        &self.sent_log
    }

    /// Send a raw rigctld command (extended `+` protocol). Still refuses `T` / set_ptt.
    pub fn transact_raw(&mut self, cmd: &str) -> Result<String, RigError> {
        self.cmd_raw(cmd)
    }

    fn cmd_raw(&mut self, cmd: &str) -> Result<String, RigError> {
        // Never transmit: refuse any set-PTT / T command.
        let trimmed = cmd.trim();
        if trimmed == "T" || trimmed.starts_with("T ") || trimmed.starts_with("+T") {
            return Err(RigError::Unsupported(
                "refusing set-PTT / T command".into(),
            ));
        }
        self.sent_log.push(cmd.to_string());
        let stream = self.stream.as_mut().ok_or(RigError::NotConnected)?;
        // Extended protocol: always prefix `+` so replies end with `RPRT n`.
        let line = if trimmed.starts_with('+') {
            format!("{trimmed}\n")
        } else {
            format!("+{trimmed}\n")
        };
        stream
            .write_all(line.as_bytes())
            .map_err(|e| RigError::Io(e.to_string()))?;
        stream.flush().map_err(|e| RigError::Io(e.to_string()))?;

        let mut reader = BufReader::new(stream.try_clone().map_err(|e| RigError::Io(e.to_string()))?);
        let mut body = String::new();
        let mut rprt = None;
        loop {
            let mut l = String::new();
            let n = reader
                .read_line(&mut l)
                .map_err(|e| RigError::Io(e.to_string()))?;
            if n == 0 {
                break;
            }
            if let Some(rest) = l.trim().strip_prefix("RPRT ") {
                rprt = Some(rest.parse::<i32>().unwrap_or(-999));
                break;
            }
            body.push_str(&l);
        }
        match rprt {
            Some(0) => Ok(body),
            Some(n) => Err(RigError::Rprt(n)),
            None => Err(RigError::Protocol("missing RPRT".into())),
        }
    }

    fn refresh_gate(&mut self) -> Result<(), RigError> {
        let caps = self.cmd_raw("\\dump_caps")?;
        self.dump_caps = caps.clone();
        if let Some(line) = caps.lines().find(|l| {
            let t = l.trim_start();
            t.starts_with("Caps dump for model:") || t.starts_with("Rig model:")
        }) {
            self.model = line
                .rsplit(':')
                .next()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
        }
        if let Some(line) = caps
            .lines()
            .find(|l| l.trim_start().starts_with("Model name:"))
        {
            self.model_name = line
                .split(':')
                .nth(1)
                .unwrap_or("unknown")
                .trim()
                .to_string();
        }
        self.gate = gate_from_dump_caps(&caps, self.allow_beta, self.allow_dummy);
        Ok(())
    }

    fn read_simple(&mut self, cmd: &str) -> Result<String, RigError> {
        let body = self.cmd_raw(cmd)?;
        // Extended replies look like: get_freq:\nFrequency: 145725000\n
        for line in body.lines() {
            if let Some((_, v)) = line.split_once(':') {
                let v = v.trim();
                if !v.is_empty() && !line.trim_start().starts_with("get_") {
                    return Ok(v.to_string());
                }
            }
        }
        Ok(body.trim().to_string())
    }
}

impl RigBackend for TcpRigBackend {
    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }
    fn model_name(&self) -> String {
        self.model_name.clone()
    }
    fn model_number(&self) -> i32 {
        self.model
    }
    fn gate(&self) -> GateDecision {
        self.gate.clone()
    }
    fn get_ptt(&mut self) -> Result<bool, RigError> {
        let v = self.read_simple("t")?;
        let t = v.trim().to_ascii_lowercase();
        Ok(t == "1" || t == "on" || t.ends_with(" 1"))
    }
    fn get_dcd(&mut self) -> Result<bool, RigError> {
        match self.cmd_raw("\\get_dcd") {
            Ok(body) => {
                for line in body.lines() {
                    if let Some((_, v)) = line.split_once(':') {
                        let t = v.trim().to_ascii_lowercase();
                        return Ok(t == "1" || t == "on");
                    }
                }
                Ok(false)
            }
            Err(RigError::Rprt(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
    fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError> {
        self.cmd_raw(&format!("V {}", want.vfo))?;
        self.cmd_raw(&format!("F {}", want.freq_hz))?;
        self.cmd_raw(&format!("M {} {}", want.mode, want.passband_hz))?;
        self.cmd_raw(&format!("R {}", want.shift.as_rigctl()))?;
        self.cmd_raw(&format!("O {}", want.offset_hz))?;
        match (want.ctcss_tenths_hz, want.dcs_code) {
            (None, None) => {
                self.cmd_raw("C 0")?;
                self.cmd_raw("D 0")?;
            }
            (Some(t), None) => {
                self.cmd_raw(&format!("C {t}"))?;
                self.cmd_raw("D 0")?;
            }
            (None, Some(d)) => {
                self.cmd_raw("C 0")?;
                self.cmd_raw(&format!("D {d}"))?;
            }
            (Some(t), Some(d)) => {
                self.cmd_raw(&format!("C {t}"))?;
                self.cmd_raw(&format!("D {d}"))?;
            }
        }
        Ok(())
    }
    fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
        let vfo = self.read_simple("v")?;
        let freq: u64 = self
            .read_simple("f")?
            .split_whitespace()
            .next()
            .unwrap_or("0")
            .parse()
            .map_err(|e| RigError::Protocol(format!("freq: {e}")))?;
        let mode_line = self.read_simple("m")?;
        let mut mode_parts = mode_line.split_whitespace();
        let mode = mode_parts.next().unwrap_or("FM").to_string();
        let passband_hz = mode_parts
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(12_500);
        let shift = ShiftDir::parse(&self.read_simple("r")?)
            .ok_or_else(|| RigError::Protocol("bad shift".into()))?;
        let offset_hz: u64 = self
            .read_simple("o")?
            .split_whitespace()
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let ctcss_tenths_hz = self
            .read_simple("c")
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse().ok())
            .filter(|&t| t > 0);
        let dcs_code = self
            .read_simple("d")
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse().ok())
            .filter(|&t| t > 0);
        Ok(VfoState {
            vfo,
            freq_hz: freq,
            mode,
            passband_hz,
            shift,
            offset_hz,
            ctcss_tenths_hz,
            dcs_code,
        })
    }
}
