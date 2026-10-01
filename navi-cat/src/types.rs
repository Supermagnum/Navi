//! Shared types and the `RigBackend` trait.

use serde::{Deserialize, Serialize};

use crate::gating::GateDecision;

#[derive(Debug, thiserror::Error)]
pub enum RigError {
    #[error("not connected")]
    NotConnected,
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("RPRT {0}")]
    Rprt(i32),
    #[error("io: {0}")]
    Io(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShiftDir {
    None,
    Minus,
    Plus,
}

impl ShiftDir {
    pub fn as_rigctl(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Minus => "-",
            Self::Plus => "+",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "None" | "0" | "" => Some(Self::None),
            "-" | "Minus" => Some(Self::Minus),
            "+" | "Plus" => Some(Self::Plus),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfoState {
    pub vfo: String,
    pub freq_hz: u64,
    pub mode: String,
    pub passband_hz: u32,
    pub shift: ShiftDir,
    pub offset_hz: u64,
    pub ctcss_tenths_hz: Option<u32>,
    pub dcs_code: Option<u32>,
}

/// Backend abstraction: TCP rigctld or Hamlib FFI.
pub trait RigBackend: Send {
    fn is_connected(&self) -> bool;
    fn model_name(&self) -> String;
    fn model_number(&self) -> i32;
    fn gate(&self) -> GateDecision;
    fn get_ptt(&mut self) -> Result<bool, RigError>;
    fn get_dcd(&mut self) -> Result<bool, RigError>;
    fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError>;
    fn read_vfo_state(&mut self) -> Result<VfoState, RigError>;
    /// Frequency step reported by backend for tolerance (0 = exact).
    fn freq_step_hz(&self) -> u64 {
        0
    }
}
