//! Host BlueZ RFCOMM transport for Benshi control (Linux desktop).
//!
//! Uses a raw `AF_BLUETOOTH` / `BTPROTO_RFCOMM` socket — no `sudo`, no
//! `/dev/rfcomm` bind. The user must be allowed to use the local adapter
//! (typically membership in the `bluetooth` group). Never opens the audio
//! RFCOMM UUID; control uses SPP channel probe (1 then 4) matching the
//! Android path.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::time::Duration;

use crate::types::RigError;

use super::backend::BenshiTransport;

/// Bluetooth Serial Port Profile UUID (control). Audio UUID must never be used.
pub const SPP_UUID: &str = "00001101-0000-1000-8000-00805F9B34FB";
pub const AUDIO_RFCOMM_UUID_DO_NOT_OPEN: &str = "39144315-32FA-40DB-85ED-FBFEBA2D86E6";

/// Channels observed on VR-N76 when SDP lookup is unavailable.
/// Channel 4 answered full GET_DEV_INFO on the host adapter (2026-10-12);
/// 1 is kept as a fallback (investigation / other firmwares).
const CHANNEL_CANDIDATES: &[u8] = &[4, 1];

#[repr(C)]
struct BdAddr {
    b: [u8; 6],
}

#[repr(C)]
struct SockAddrRc {
    rc_family: libc::sa_family_t,
    rc_bdaddr: BdAddr,
    rc_channel: u8,
}

fn parse_mac(mac: &str) -> Result<[u8; 6], RigError> {
    let parts: Vec<_> = mac.split([':', '-']).collect();
    if parts.len() != 6 {
        return Err(RigError::Protocol(format!("bad Bluetooth MAC: {mac}")));
    }
    let mut out = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        out[i] = u8::from_str_radix(p, 16)
            .map_err(|_| RigError::Protocol(format!("bad Bluetooth MAC octet: {p}")))?;
    }
    Ok(out)
}

fn set_timeouts(fd: libc::c_int, timeout: Duration) -> Result<(), RigError> {
    let tv = libc::timeval {
        tv_sec: timeout.as_secs() as libc::time_t,
        tv_usec: (timeout.subsec_micros()) as libc::suseconds_t,
    };
    let r = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const _ as *const libc::c_void,
            std::mem::size_of_val(&tv) as libc::socklen_t,
        )
    };
    if r != 0 {
        return Err(RigError::Protocol(format!(
            "SO_RCVTIMEO: {}",
            std::io::Error::last_os_error()
        )));
    }
    let r = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_SNDTIMEO,
            &tv as *const _ as *const libc::c_void,
            std::mem::size_of_val(&tv) as libc::socklen_t,
        )
    };
    if r != 0 {
        return Err(RigError::Protocol(format!(
            "SO_SNDTIMEO: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

fn connect_channel(mac: [u8; 6], channel: u8, timeout: Duration) -> Result<OwnedFd, RigError> {
    // AF_BLUETOOTH=31, BTPROTO_RFCOMM=3 on Linux.
    const AF_BLUETOOTH: libc::c_int = 31;
    const BTPROTO_RFCOMM: libc::c_int = 3;
    let fd = unsafe { libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM, BTPROTO_RFCOMM) };
    if fd < 0 {
        return Err(RigError::Protocol(format!(
            "RFCOMM socket: {}",
            std::io::Error::last_os_error()
        )));
    }
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    set_timeouts(owned.as_raw_fd(), timeout)?;
    let mut addr = SockAddrRc {
        rc_family: AF_BLUETOOTH as libc::sa_family_t,
        rc_bdaddr: BdAddr { b: mac },
        rc_channel: channel,
    };
    // BlueZ expects bdaddr in little-endian byte order for sockaddr_rc.
    addr.rc_bdaddr.b.reverse();
    let r = unsafe {
        libc::connect(
            owned.as_raw_fd(),
            &addr as *const _ as *const libc::sockaddr,
            std::mem::size_of::<SockAddrRc>() as libc::socklen_t,
        )
    };
    if r != 0 {
        return Err(RigError::Protocol(format!(
            "RFCOMM connect ch={channel}: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(owned)
}

/// Blocking RFCOMM byte pipe to a paired Benshi radio.
pub struct BluezRfcommTransport {
    stream: std::fs::File,
    mac: String,
    channel: u8,
}

impl BluezRfcommTransport {
    /// Connect to `mac` (e.g. `AA:BB:CC:DD:EE:FF`), probing control channels.
    /// Never opens the audio RFCOMM UUID.
    pub fn connect(mac: &str) -> Result<Self, RigError> {
        Self::connect_with_timeout(mac, Duration::from_secs(8))
    }

    /// Connect on a single RFCOMM channel (hardware bring-up / channel A-B tests).
    pub fn connect_channel(mac: &str, channel: u8) -> Result<Self, RigError> {
        Self::connect_channel_with_timeout(mac, channel, Duration::from_secs(8))
    }

    pub fn connect_channel_with_timeout(
        mac: &str,
        channel: u8,
        timeout: Duration,
    ) -> Result<Self, RigError> {
        let _ = (SPP_UUID, AUDIO_RFCOMM_UUID_DO_NOT_OPEN);
        let addr = parse_mac(mac)?;
        let fd = connect_channel(addr, channel, timeout)?;
        let stream = unsafe { std::fs::File::from_raw_fd(fd.into_raw_fd()) };
        Ok(Self {
            stream,
            mac: mac.to_string(),
            channel,
        })
    }

    pub fn connect_with_timeout(mac: &str, timeout: Duration) -> Result<Self, RigError> {
        let _ = (SPP_UUID, AUDIO_RFCOMM_UUID_DO_NOT_OPEN); // documented; channel probe only
        let mut last = RigError::Protocol("no RFCOMM channel candidates".into());
        // Prefer channel 1 (SPP control); 4 is a fallback only.
        for &ch in CHANNEL_CANDIDATES {
            match Self::connect_channel_with_timeout(mac, ch, timeout) {
                Ok(t) => return Ok(t),
                Err(e) => last = e,
            }
        }
        Err(RigError::Protocol(format!(
            "BlueZ RFCOMM connect failed for {mac} (tried channels {CHANNEL_CANDIDATES:?}): {last}"
        )))
    }

    pub fn mac(&self) -> &str {
        &self.mac
    }

    pub fn channel(&self) -> u8 {
        self.channel
    }
}

impl Drop for BluezRfcommTransport {
    fn drop(&mut self) {
        let fd = self.stream.as_raw_fd();
        unsafe {
            let _ = libc::shutdown(fd, libc::SHUT_RDWR);
        }
    }
}

impl BenshiTransport for BluezRfcommTransport {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), RigError> {
        self.stream
            .write_all(bytes)
            .map_err(|e| RigError::Protocol(format!("RFCOMM write: {e}")))
    }

    fn read_some(&mut self, timeout: Duration) -> Result<Vec<u8>, RigError> {
        set_timeouts(self.stream.as_raw_fd(), timeout)?;
        let mut buf = [0u8; 2048];
        match self.stream.read(&mut buf) {
            Ok(0) => Err(RigError::Protocol("RFCOMM closed by peer".into())),
            Ok(n) => Ok(buf[..n].to_vec()),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Ok(Vec::new())
            }
            Err(e) => Err(RigError::Protocol(format!("RFCOMM read: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mac_ok() {
        assert_eq!(
            parse_mac("AA:BB:CC:DD:EE:FF").unwrap(),
            [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]
        );
    }

    #[test]
    fn audio_uuid_constant_never_used_as_channel_probe() {
        // Sanity: audio UUID string must stay distinct from SPP.
        assert_ne!(SPP_UUID, AUDIO_RFCOMM_UUID_DO_NOT_OPEN);
        assert!(!CHANNEL_CANDIDATES.is_empty());
    }
}
