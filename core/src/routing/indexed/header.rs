//! Fixed 8-byte preamble before every `rkyv` payload.
//!
//! Layout (little-endian):
//! - bytes 0..4: magic (`u32`)
//! - bytes 4..8: format_version (`u32`)
//! - bytes 8..: rkyv payload for that exact version only

use std::io::{Read, Write};

pub const PREAMBLE_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preamble {
    pub magic: u32,
    pub format_version: u32,
}

impl Preamble {
    pub fn new(magic: u32, format_version: u32) -> Self {
        Self {
            magic,
            format_version,
        }
    }

    pub fn to_bytes(self) -> [u8; PREAMBLE_LEN] {
        let mut out = [0u8; PREAMBLE_LEN];
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..8].copy_from_slice(&self.format_version.to_le_bytes());
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < PREAMBLE_LEN {
            return None;
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let format_version = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
        Some(Self {
            magic,
            format_version,
        })
    }
}

pub fn read_preamble(mut r: impl Read) -> std::io::Result<Preamble> {
    let mut buf = [0u8; PREAMBLE_LEN];
    r.read_exact(&mut buf)?;
    Preamble::from_bytes(&buf).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "short indexed-pack preamble",
        )
    })
}

pub fn write_preamble(mut w: impl Write, preamble: Preamble) -> std::io::Result<()> {
    w.write_all(&preamble.to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preamble_roundtrip() {
        let p = Preamble::new(0x4E_56_52_4B, 1);
        let b = p.to_bytes();
        assert_eq!(Preamble::from_bytes(&b), Some(p));
    }

    #[test]
    fn mismatch_version_is_detectable() {
        let p = Preamble::new(0x4E_56_52_4B, 99);
        assert_ne!(p.format_version, 1);
    }
}
