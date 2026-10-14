//! GAIA framing and Benshi payload codecs.
//!
//! Framing and field layouts from HTCommander
//! `docs/blogs/radio-command-protocol.md` and `src/lib/radio/*.dart`
//! (Apache-2.0).

use crate::benshi::allowlist::{is_allowed, GROUP_BASIC};

pub const GAIA_SOF: u8 = 0xFF;
pub const GAIA_VER: u8 = 0x01;

pub const CMD_GET_DEV_INFO: u16 = 4;
/// HTCommander sends a single capability-revision byte with GET_DEV_INFO.
pub const GET_DEV_INFO_REQ_PAYLOAD: &[u8] = &[3];
pub const CMD_REGISTER_NOTIFICATION: u16 = 6;
pub const CMD_EVENT_NOTIFICATION: u16 = 9;
pub const CMD_READ_SETTINGS: u16 = 10;
pub const CMD_WRITE_SETTINGS: u16 = 11;
pub const CMD_READ_RF_CH: u16 = 13;
pub const CMD_WRITE_RF_CH: u16 = 14;
pub const CMD_GET_HT_STATUS: u16 = 20;
pub const CMD_FREQ_MODE_SET_PAR: u16 = 35;
pub const CMD_FREQ_MODE_GET_STATUS: u16 = 36;

pub const NOTIF_HT_STATUS_CHANGED: u8 = 1;
pub const NOTIF_FREQ_MODE_STATUS_CHANGED: u8 = 14;

pub const FREQ_MODE_STEP_CONST: u16 = 0x61A8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub group: u16,
    pub cmd: u16,
    pub data: Vec<u8>,
}

impl Command {
    pub fn basic(cmd: u16, data: Vec<u8>) -> Result<Self, String> {
        if !is_allowed(GROUP_BASIC, cmd) {
            return Err(format!("command {cmd} refused by never-transmit allowlist"));
        }
        Ok(Self {
            group: GROUP_BASIC,
            cmd,
            data,
        })
    }

    pub fn to_body(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(4 + self.data.len());
        body.push(((self.group >> 8) & 0xFF) as u8);
        body.push((self.group & 0xFF) as u8);
        body.push(((self.cmd >> 8) & 0xFF) as u8);
        body.push((self.cmd & 0xFF) as u8);
        body.extend_from_slice(&self.data);
        body
    }
}

/// Wrap a 4-byte command header + payload in GAIA RFCOMM framing.
pub fn gaia_encode(body: &[u8]) -> Result<Vec<u8>, String> {
    if body.len() < 4 {
        return Err("command body too short".into());
    }
    let payload_len = body.len() - 4;
    if payload_len > 0xFF {
        return Err("payload too long for GAIA len byte".into());
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.push(GAIA_SOF);
    out.push(GAIA_VER);
    out.push(0x00); // checksum flag unused
    out.push(payload_len as u8);
    out.extend_from_slice(body);
    Ok(out)
}

/// Decode one or more GAIA frames from a buffer. Returns unframed bodies and
/// bytes consumed. Skips leading noise until `FF 01`.
pub fn gaia_decode(buf: &[u8]) -> Result<(Vec<Vec<u8>>, usize), String> {
    let mut frames = Vec::new();
    let mut i = 0usize;
    while i + 8 <= buf.len() {
        if buf[i] != GAIA_SOF || buf[i + 1] != GAIA_VER {
            i += 1;
            continue;
        }
        let has_cs = (buf[i + 2] & 1) != 0;
        let payload_len = buf[i + 3] as usize;
        let total = payload_len + 8 + if has_cs { 1 } else { 0 };
        if i + total > buf.len() {
            break;
        }
        let body = buf[i + 4..i + 4 + 4 + payload_len].to_vec();
        frames.push(body);
        i += total;
    }
    Ok((frames, i))
}

pub fn parse_body(body: &[u8]) -> Result<Command, String> {
    if body.len() < 4 {
        return Err("short command body".into());
    }
    let group = u16::from_be_bytes([body[0], body[1]]);
    let cmd = u16::from_be_bytes([body[2], body[3]]);
    Ok(Command {
        group,
        cmd,
        data: body[4..].to_vec(),
    })
}

pub fn reply_cmd(request_cmd: u16) -> u16 {
    request_cmd | 0x8000
}

pub fn strip_response_bit(cmd: u16) -> u16 {
    cmd & 0x7FFF
}

pub fn get_u16_be(d: &[u8], p: usize) -> u16 {
    if p + 1 >= d.len() {
        return 0;
    }
    u16::from_be_bytes([d[p], d[p + 1]])
}

pub fn get_u32_be(d: &[u8], p: usize) -> u32 {
    if p + 3 >= d.len() {
        return 0;
    }
    u32::from_be_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]])
}

pub fn put_u16_be(d: &mut [u8], p: usize, v: u16) {
    let b = v.to_be_bytes();
    d[p] = b[0];
    d[p + 1] = b[1];
}

pub fn put_u32_be(d: &mut [u8], p: usize, v: u32) {
    let b = v.to_be_bytes();
    d[p..p + 4].copy_from_slice(&b);
}

/// Frequency encoding: top 2 bits modulation, low 30 bits Hz.
pub fn encode_freq_hz(hz: u32, mod_bits: u8) -> u32 {
    (hz & 0x3FFF_FFFF) | (((mod_bits as u32) & 0x03) << 30)
}

pub fn decode_freq_hz(v: u32) -> (u32, u8) {
    ((v & 0x3FFF_FFFF), ((v >> 30) & 0x03) as u8)
}

/// CTCSS: VfoState tenths-Hz (885 = 88.5) <-> protocol 0.01 Hz units (8850).
pub fn tenths_to_subaudio(tenths: u32) -> u16 {
    (tenths * 10) as u16
}

pub fn subaudio_to_tenths(sa: u16) -> Option<u32> {
    if sa == 0 {
        None
    } else if sa < 6700 {
        // DCS code space (benlink); surface as dcs elsewhere
        None
    } else {
        Some((sa as u32) / 10)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevInfo {
    pub vendor_id: u8,
    pub product_id: u16,
    pub support_vfo: bool,
    pub support_dmr: bool,
    pub channel_count: u8,
    pub raw: Vec<u8>,
}

impl DevInfo {
    /// `msg` is full unframed reply (group/cmd/status/payload...).
    pub fn from_reply(msg: &[u8]) -> Result<Self, String> {
        if msg.len() < 15 {
            return Err("GET_DEV_INFO truncated".into());
        }
        if msg[4] != 0 {
            return Err(format!("GET_DEV_INFO status {}", msg[4]));
        }
        Ok(Self {
            vendor_id: msg[5],
            product_id: get_u16_be(msg, 6),
            support_vfo: (msg[12] & 0x02) != 0,
            support_dmr: (msg[12] & 0x01) != 0,
            channel_count: msg[13],
            raw: msg.to_vec(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtStatus {
    pub is_in_tx: bool,
    pub is_sq: bool,
    pub is_in_rx: bool,
    pub double_channel: u8,
    pub is_scan: bool,
    pub curr_ch_id: u8,
    pub raw: Vec<u8>,
}

impl HtStatus {
    pub fn from_reply(msg: &[u8]) -> Result<Self, String> {
        if msg.len() < 9 {
            return Err("GET_HT_STATUS truncated".into());
        }
        // Reply status at [4] for command replies; notifications put type at [4].
        let b5 = msg[5];
        let curr_ch_id = (((msg.get(8).copied().unwrap_or(0) & 0x3C) >> 2) << 4)
            .wrapping_add(msg.get(6).copied().unwrap_or(0) >> 4);
        Ok(Self {
            is_in_tx: (b5 & 0x40) != 0,
            is_sq: (b5 & 0x20) != 0,
            is_in_rx: (b5 & 0x10) != 0,
            double_channel: (b5 & 0x0C) >> 2,
            is_scan: (b5 & 0x02) != 0,
            curr_ch_id,
            raw: msg.to_vec(),
        })
    }

    pub fn vfo1_selected_or_single(&self) -> bool {
        self.double_channel == 0 || self.double_channel == 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelRecord {
    pub channel_id: u8,
    pub tx_freq_hz: u32,
    pub rx_freq_hz: u32,
    pub tx_sub_audio: u16,
    pub rx_sub_audio: u16,
    pub bandwidth_wide: bool,
    pub raw_flags18: u8,
    pub raw_flags19: u8,
    pub name: [u8; 10],
}

impl ChannelRecord {
    /// Parse from full READ_RF_CH reply body.
    pub fn from_reply(msg: &[u8]) -> Result<Self, String> {
        if msg.len() < 30 {
            return Err("READ_RF_CH truncated".into());
        }
        if msg[4] != 0 {
            return Err(format!("READ_RF_CH status {}", msg[4]));
        }
        let tx = get_u32_be(msg, 6);
        let rx = get_u32_be(msg, 10);
        let mut name = [0u8; 10];
        name.copy_from_slice(&msg[20..30]);
        Ok(Self {
            channel_id: msg[5],
            tx_freq_hz: tx & 0x3FFF_FFFF,
            rx_freq_hz: rx & 0x3FFF_FFFF,
            tx_sub_audio: get_u16_be(msg, 14),
            rx_sub_audio: get_u16_be(msg, 16),
            bandwidth_wide: (msg[18] & 0x10) != 0,
            raw_flags18: msg[18],
            raw_flags19: msg[19],
            name,
        })
    }

    /// 25-byte WRITE_RF_CH payload (no status byte).
    pub fn to_write_payload(&self) -> [u8; 25] {
        let mut r = [0u8; 25];
        r[0] = self.channel_id;
        put_u32_be(&mut r, 1, encode_freq_hz(self.tx_freq_hz, 0));
        put_u32_be(&mut r, 5, encode_freq_hz(self.rx_freq_hz, 0));
        put_u16_be(&mut r, 9, self.tx_sub_audio);
        put_u16_be(&mut r, 11, self.rx_sub_audio);
        // WRITE_RF_CH layout (HTCommander RadioChannelInfo.toByteArray): flags at
        // 13/14, GBK name at 15..24.
        r[13] = (self.raw_flags18 & !0x10) | if self.bandwidth_wide { 0x10 } else { 0 };
        r[14] = self.raw_flags19;
        r[15..25].copy_from_slice(&self.name);
        r
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsBlock {
    /// Full unframed READ_SETTINGS reply (includes header + status).
    pub raw_reply: Vec<u8>,
    pub channel_a: u8,
    pub channel_b: u8,
    pub double_channel: u8,
}

impl SettingsBlock {
    pub fn from_reply(msg: &[u8]) -> Result<Self, String> {
        if msg.len() < 15 {
            return Err("READ_SETTINGS truncated".into());
        }
        if msg[4] != 0 {
            return Err(format!("READ_SETTINGS status {}", msg[4]));
        }
        let channel_a = ((msg[5] & 0xF0) >> 4) + (msg.get(14).copied().unwrap_or(0) & 0xF0);
        let channel_b = (msg[5] & 0x0F) + ((msg.get(14).copied().unwrap_or(0) & 0x0F) << 4);
        let double_channel = (msg[6] & 0x30) >> 4;
        Ok(Self {
            raw_reply: msg.to_vec(),
            channel_a,
            channel_b,
            double_channel,
        })
    }

    /// VFO 2 fingerprint: channelB + vfo2 power/mod fields when present.
    pub fn vfo2_fingerprint(&self) -> Vec<u8> {
        let mut fp = vec![self.channel_b, self.double_channel];
        if self.raw_reply.len() >= 25 {
            fp.extend_from_slice(&self.raw_reply[16..25.min(self.raw_reply.len())]);
        }
        fp
    }

    /// WRITE_SETTINGS payload: copy raw body from offset 5, change channel A only.
    pub fn write_payload_channel_a_only(&self, channel_a: u8) -> Result<Vec<u8>, String> {
        if self.raw_reply.len() < 6 {
            return Err("settings raw too short".into());
        }
        let mut data = self.raw_reply[5..].to_vec();
        let ch_b = self.channel_b;
        data[0] = ((channel_a & 0x0F) << 4) | (ch_b & 0x0F);
        // Preserve doubleChannel / scan / squelch in data[1]
        if data.len() > 9 {
            data[9] = (channel_a & 0xF0) | ((ch_b >> 4) & 0x0F);
        }
        Ok(data)
    }

    /// Channel-B content fingerprint (excludes selected-VFO / doubleChannel).
    pub fn channel_b_content_fingerprint(&self) -> Vec<u8> {
        let mut fp = vec![self.channel_b];
        if self.raw_reply.len() >= 25 {
            fp.extend_from_slice(&self.raw_reply[16..25.min(self.raw_reply.len())]);
        }
        fp
    }

    /// Operator/hardware-test only: set doubleChannel=1 (VFO A), keep channel B.
    /// Product auto-tune must not call this — VFO selection belongs to the user.
    pub fn write_payload_select_vfo_a(&self) -> Result<Vec<u8>, String> {
        if self.raw_reply.len() < 7 {
            return Err("settings raw too short".into());
        }
        let mut data = self.raw_reply[5..].to_vec();
        data[1] = (data[1] & !0x30) | (1 << 4);
        Ok(data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreqModePar {
    pub rx_hz: u32,
    pub tx_hz: u32,
    pub rx_sub_audio: u16,
    pub tx_sub_audio: u16,
    pub flags: u16,
    pub step: u16,
}

impl FreqModePar {
    pub fn tune(rx_hz: u32, tx_hz: u32, tx_sub_audio: u16) -> Self {
        Self {
            rx_hz,
            tx_hz,
            rx_sub_audio: 0,
            tx_sub_audio,
            flags: 0x0000, // ordinary VFO; never 0x0Axx (satellite screen)
            step: FREQ_MODE_STEP_CONST,
        }
    }

    pub fn stop() -> Self {
        Self {
            rx_hz: 0,
            tx_hz: 0,
            rx_sub_audio: 0,
            tx_sub_audio: 0,
            flags: 0,
            step: 0,
        }
    }

    pub fn to_payload(&self) -> [u8; 16] {
        let mut r = [0u8; 16];
        put_u32_be(&mut r, 0, encode_freq_hz(self.rx_hz, 0));
        put_u32_be(&mut r, 4, encode_freq_hz(self.tx_hz, 0));
        put_u16_be(&mut r, 8, self.rx_sub_audio);
        put_u16_be(&mut r, 10, self.tx_sub_audio);
        put_u16_be(&mut r, 12, self.flags);
        put_u16_be(&mut r, 14, self.step);
        r
    }
}

/// Parse freqModeStatusChanged (notification 14) payload in full body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreqModeStatusNotif {
    pub rx_hz: u32,
    pub tx_hz: u32,
    pub sub_audio: u32,
    pub flags_low: u8,
    pub active: bool,
}

impl FreqModeStatusNotif {
    pub fn from_event_body(msg: &[u8]) -> Result<Self, String> {
        if msg.len() < 19 {
            return Err("freqModeStatusChanged truncated".into());
        }
        // EVENT_NOTIFICATION body: [status-or-type...] — HTCommander layout:
        // data[4]=type 14, data[5..8]=RX, [9..12]=TX, [13..16]=sub-audio, [17..18]=flags
        let flags_low = msg[18];
        Ok(Self {
            rx_hz: get_u32_be(msg, 5) & 0x3FFF_FFFF,
            tx_hz: get_u32_be(msg, 9) & 0x3FFF_FFFF,
            sub_audio: get_u32_be(msg, 13),
            flags_low,
            active: flags_low != 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaia_roundtrip_and_concat() {
        let body = Command::basic(CMD_GET_HT_STATUS, vec![]).unwrap().to_body();
        let frame = gaia_encode(&body).unwrap();
        let mut buf = frame.clone();
        buf.extend_from_slice(&frame);
        let (frames, n) = gaia_decode(&buf).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(n, buf.len());
        assert_eq!(frames[0], body);
    }

    #[test]
    fn gaia_resync_skips_noise() {
        let body = Command::basic(CMD_GET_DEV_INFO, GET_DEV_INFO_REQ_PAYLOAD.to_vec())
            .unwrap()
            .to_body();
        let frame = gaia_encode(&body).unwrap();
        let mut buf = vec![0xAA, 0xBB];
        buf.extend_from_slice(&frame);
        let (frames, _) = gaia_decode(&buf).unwrap();
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn freq_mode_example_issue_values() {
        // 145.725 / −0.6 / 88.5 → RX 145725000, TX 145125000, sub 8850
        let p = FreqModePar::tune(145_725_000, 145_125_000, 8850);
        let b = p.to_payload();
        assert_eq!(get_u32_be(&b, 0) & 0x3FFF_FFFF, 145_725_000);
        assert_eq!(get_u32_be(&b, 4) & 0x3FFF_FFFF, 145_125_000);
        assert_eq!(get_u16_be(&b, 10), 8850);
        assert_eq!(get_u16_be(&b, 12), 0);
        assert_eq!(get_u16_be(&b, 14), FREQ_MODE_STEP_CONST);
    }

    #[test]
    fn allowlist_blocks_encode_path() {
        assert!(Command::basic(31, vec![]).is_err());
        assert!(Command::basic(66, vec![]).is_err());
    }

    #[test]
    fn channel_record_roundtrip_preserves_flags() {
        let ch = ChannelRecord {
            channel_id: 3,
            tx_freq_hz: 145_125_000,
            rx_freq_hz: 145_725_000,
            tx_sub_audio: 8850,
            rx_sub_audio: 0,
            bandwidth_wide: false,
            raw_flags18: 0x40 | 0x08, // max power + pre/de-emph
            raw_flags19: 0x10,        // mute bit preserved
            name: *b"LA5TRR\0\0\0\0",
        };
        let payload = ch.to_write_payload();
        let mut reply = Vec::new();
        reply.extend_from_slice(&GROUP_BASIC.to_be_bytes());
        reply.extend_from_slice(&reply_cmd(CMD_READ_RF_CH).to_be_bytes());
        reply.push(0);
        reply.extend_from_slice(&payload);
        let got = ChannelRecord::from_reply(&reply).unwrap();
        assert_eq!(got.rx_freq_hz, ch.rx_freq_hz);
        assert_eq!(got.tx_freq_hz, ch.tx_freq_hz);
        assert_eq!(got.tx_sub_audio, 8850);
        assert!(!got.bandwidth_wide);
        assert_eq!(got.raw_flags19 & 0x10, 0x10);
    }

    #[test]
    fn settings_channel_a_only_preserves_vfo2() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&GROUP_BASIC.to_be_bytes());
        raw.extend_from_slice(&reply_cmd(CMD_READ_SETTINGS).to_be_bytes());
        raw.push(0);
        let mut payload = vec![0u8; 20];
        payload[0] = (1 << 4) | 2; // chA=1 low nibble, chB=2
        payload[1] = 2 << 4; // doubleChannel = VFO2
        payload[9] = 0;
        put_u32_be(&mut payload, 16, 0xDEAD_BEEF);
        raw.extend_from_slice(&payload);
        let s = SettingsBlock::from_reply(&raw).unwrap();
        assert_eq!(s.channel_b, 2);
        assert_eq!(s.double_channel, 2);
        let fp = s.vfo2_fingerprint();
        let w = s.write_payload_channel_a_only(7).unwrap();
        assert_eq!(w[0] & 0x0F, 2); // channel B low preserved
        assert_eq!((w[1] >> 4) & 0x03, 2); // doubleChannel preserved
        let mut raw2 = raw[..5].to_vec();
        raw2.extend_from_slice(&w);
        let s2 = SettingsBlock::from_reply(&raw2).unwrap();
        assert_eq!(s2.channel_a, 7);
        assert_eq!(s2.vfo2_fingerprint(), fp);
    }
}
