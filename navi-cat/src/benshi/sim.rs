//! In-process fake Benshi peer (CI plumbing — not a physical radio).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::benshi::allowlist::{is_allowed, GROUP_BASIC};
use crate::benshi::codec::*;
use crate::benshi::gate::ALLOWED_PRODUCT_VR_N76;

#[derive(Debug, Clone)]
pub struct FakeBenshiConfig {
    pub vendor_id: u8,
    pub product_id: u16,
    pub support_vfo: bool,
    pub support_dmr: bool,
    pub channel_count: u8,
    /// When false, frequency mode is unavailable (forces scratch / defer).
    pub freq_mode_addresses_vfo1: bool,
    pub emit_freq_mode_notif: bool,
}

impl Default for FakeBenshiConfig {
    fn default() -> Self {
        Self {
            vendor_id: 0x01,
            product_id: ALLOWED_PRODUCT_VR_N76,
            support_vfo: true,
            support_dmr: false,
            channel_count: 64,
            freq_mode_addresses_vfo1: true,
            emit_freq_mode_notif: true,
        }
    }
}

#[derive(Debug)]
struct Inner {
    cfg: FakeBenshiConfig,
    channels: Vec<Option<ChannelRecord>>,
    /// Full READ_SETTINGS-style reply body (group/cmd/status/payload).
    settings_reply: Vec<u8>,
    is_in_tx: bool,
    is_sq: bool,
    is_in_rx: bool,
    double_channel: u8,
    is_scan: bool,
    freq_mode: Option<FreqModePar>,
    notif_regs: Vec<u8>,
    rx_queue: VecDeque<Vec<u8>>,
    sent_cmds: Vec<(u16, u16)>,
}

impl Inner {
    fn new(cfg: FakeBenshiConfig) -> Self {
        let n = cfg.channel_count.max(1) as usize;
        let mut channels = vec![None; n];
        // Default scratch-friendly empty channel 0
        channels[0] = Some(ChannelRecord {
            channel_id: 0,
            tx_freq_hz: 145_125_000,
            rx_freq_hz: 145_725_000,
            tx_sub_audio: 8850,
            rx_sub_audio: 0,
            bandwidth_wide: false,
            raw_flags18: 0x40, // tx at max power
            raw_flags19: 0,
            name: *b"CH0\0\0\0\0\0\0\0",
        });
        // VFO 2 default channel 1 — distinctive so tests can detect mutation
        if n > 1 {
            channels[1] = Some(ChannelRecord {
                channel_id: 1,
                tx_freq_hz: 145_500_000,
                rx_freq_hz: 145_500_000,
                tx_sub_audio: 0,
                rx_sub_audio: 0,
                bandwidth_wide: false,
                raw_flags18: 0x40,
                raw_flags19: 0,
                name: *b"VFO2SIMPL\0",
            });
        }
        let settings_reply = default_settings_reply(0, 1, 0);
        Self {
            cfg,
            channels,
            settings_reply,
            is_in_tx: false,
            is_sq: false,
            is_in_rx: false,
            double_channel: 0,
            is_scan: false,
            freq_mode: None,
            notif_regs: Vec::new(),
            rx_queue: VecDeque::new(),
            sent_cmds: Vec::new(),
        }
    }

    fn enqueue_body(&mut self, body: Vec<u8>) {
        let frame = gaia_encode(&body).expect("gaia encode");
        self.rx_queue.push_back(frame);
    }

    fn handle(&mut self, body: &[u8]) -> Result<(), String> {
        let cmd = parse_body(body)?;
        let req = strip_response_bit(cmd.cmd);
        self.sent_cmds.push((cmd.group, req));
        if !is_allowed(cmd.group, req) {
            // Peer still ACKs with notSupported so tests can observe refusal on host first.
            let mut reply = cmd.to_body();
            reply[2] = ((reply_cmd(req) >> 8) & 0xFF) as u8;
            reply[3] = (reply_cmd(req) & 0xFF) as u8;
            let mut data = vec![1u8]; // notSupported
            data.extend_from_slice(&cmd.data);
            let mut out = reply[..4].to_vec();
            out.extend_from_slice(&data);
            self.enqueue_body(out);
            return Ok(());
        }
        match req {
            CMD_GET_DEV_INFO => self.reply_dev_info(),
            CMD_REGISTER_NOTIFICATION => {
                if let Some(t) = cmd.data.first() {
                    if !self.notif_regs.contains(t) {
                        self.notif_regs.push(*t);
                    }
                }
                self.reply_status(CMD_REGISTER_NOTIFICATION, 0, &[]);
            }
            CMD_READ_SETTINGS => {
                let mut body = self.settings_reply.clone();
                body[2] = ((reply_cmd(CMD_READ_SETTINGS) >> 8) & 0xFF) as u8;
                body[3] = (reply_cmd(CMD_READ_SETTINGS) & 0xFF) as u8;
                self.enqueue_body(body);
            }
            CMD_WRITE_SETTINGS => {
                if cmd.data.len() + 5 > self.settings_reply.len() {
                    self.settings_reply.resize(cmd.data.len() + 5, 0);
                }
                // Keep group/cmd/status; replace payload
                self.settings_reply[5..5 + cmd.data.len()].copy_from_slice(&cmd.data);
                // Refresh parsed double_channel from settings
                if let Ok(s) = SettingsBlock::from_reply(&self.settings_reply) {
                    self.double_channel = s.double_channel;
                }
                self.reply_status(CMD_WRITE_SETTINGS, 0, &[]);
            }
            CMD_READ_RF_CH => {
                let id = cmd.data.first().copied().unwrap_or(0) as usize;
                let mut body = Vec::new();
                body.extend_from_slice(&GROUP_BASIC.to_be_bytes());
                body.extend_from_slice(&reply_cmd(CMD_READ_RF_CH).to_be_bytes());
                body.push(0);
                if let Some(ch) = self.channels.get(id).and_then(|c| c.as_ref()) {
                    body.extend_from_slice(&ch.to_write_payload());
                } else {
                    let mut empty = [0u8; 25];
                    empty[0] = id as u8;
                    body.extend_from_slice(&empty);
                }
                self.enqueue_body(body);
            }
            CMD_WRITE_RF_CH => {
                if cmd.data.len() >= 25 {
                    let id = cmd.data[0] as usize;
                    let mut msg = Vec::new();
                    msg.extend_from_slice(&u16::to_be_bytes(GROUP_BASIC));
                    msg.extend_from_slice(&u16::to_be_bytes(reply_cmd(CMD_READ_RF_CH)));
                    msg.push(0);
                    msg.extend_from_slice(&cmd.data);
                    if let Ok(ch) = ChannelRecord::from_reply(&msg) {
                        if id < self.channels.len() {
                            self.channels[id] = Some(ch);
                        }
                    }
                }
                self.reply_status(CMD_WRITE_RF_CH, 0, &[]);
            }
            CMD_GET_HT_STATUS => self.reply_ht_status(),
            CMD_FREQ_MODE_SET_PAR => {
                if cmd.data.len() >= 16 {
                    let rx = get_u32_be(&cmd.data, 0) & 0x3FFF_FFFF;
                    let tx = get_u32_be(&cmd.data, 4) & 0x3FFF_FFFF;
                    let rx_sa = get_u16_be(&cmd.data, 8);
                    let tx_sa = get_u16_be(&cmd.data, 10);
                    let flags = get_u16_be(&cmd.data, 12);
                    let step = get_u16_be(&cmd.data, 14);
                    if rx == 0 && tx == 0 {
                        self.freq_mode = None;
                    } else {
                        self.freq_mode = Some(FreqModePar {
                            rx_hz: rx,
                            tx_hz: tx,
                            rx_sub_audio: rx_sa,
                            tx_sub_audio: tx_sa,
                            flags,
                            step,
                        });
                    }
                }
                self.reply_status(CMD_FREQ_MODE_SET_PAR, 0, &[0]);
                if self.cfg.emit_freq_mode_notif {
                    let fm = self.freq_mode.clone();
                    if let Some(fm) = fm {
                        self.push_freq_mode_notif(&fm);
                    } else {
                        self.push_freq_mode_notif_inactive();
                    }
                }
            }
            CMD_FREQ_MODE_GET_STATUS => {
                let mut data = vec![0u8];
                if let Some(fm) = &self.freq_mode {
                    let mut freq = [0u8; 4];
                    put_u32_be(&mut freq, 0, encode_freq_hz(fm.rx_hz, 0));
                    data.extend_from_slice(&freq);
                } else {
                    data.extend_from_slice(&[0, 0, 0, 0]);
                }
                self.reply_status(CMD_FREQ_MODE_GET_STATUS, 0, &data[1..]);
            }
            _ => self.reply_status(req, 1, &[]),
        }
        Ok(())
    }

    fn reply_status(&mut self, cmd: u16, status: u8, payload: &[u8]) {
        let mut body = Vec::new();
        body.extend_from_slice(&GROUP_BASIC.to_be_bytes());
        body.extend_from_slice(&reply_cmd(cmd).to_be_bytes());
        body.push(status);
        body.extend_from_slice(payload);
        self.enqueue_body(body);
    }

    fn reply_dev_info(&mut self) {
        let mut payload = vec![0u8; 10];
        payload[0] = self.cfg.vendor_id; // at msg[5]
        put_u16_be(&mut payload, 1, self.cfg.product_id); // msg[6..7]
        payload[3] = 1; // hw
        put_u16_be(&mut payload, 4, 1); // soft
        payload[6] = 0x80; // supportRadio
        let mut b12 = 0u8;
        if self.cfg.support_vfo {
            b12 |= 0x02;
        }
        if self.cfg.support_dmr {
            b12 |= 0x01;
        }
        payload[7] = b12;
        payload[8] = self.cfg.channel_count;
        payload[9] = 0x10; // freqRangeCount nibble
        self.reply_status(CMD_GET_DEV_INFO, 0, &payload);
    }

    fn reply_ht_status(&mut self) {
        let mut b = [0u8; 4];
        let mut flags = 0u8;
        if self.is_in_tx {
            flags |= 0x40;
        }
        if self.is_sq {
            flags |= 0x20;
        }
        if self.is_in_rx {
            flags |= 0x10;
        }
        flags |= (self.double_channel & 0x03) << 2;
        if self.is_scan {
            flags |= 0x02;
        }
        flags |= 0x80; // power on
        b[0] = flags;
        let ch = SettingsBlock::from_reply(&self.settings_reply)
            .map(|s| {
                if self.double_channel == 2 {
                    s.channel_b
                } else {
                    s.channel_a
                }
            })
            .unwrap_or(0);
        b[1] = (ch & 0x0F) << 4;
        b[2] = 0;
        b[3] = ((ch >> 4) & 0x0F) << 2;
        self.reply_status(CMD_GET_HT_STATUS, 0, &b);
    }

    fn push_freq_mode_notif(&mut self, fm: &FreqModePar) {
        if !self.notif_regs.contains(&NOTIF_FREQ_MODE_STATUS_CHANGED) {
            return;
        }
        let mut body = Vec::new();
        body.extend_from_slice(&GROUP_BASIC.to_be_bytes());
        body.extend_from_slice(&reply_cmd(CMD_EVENT_NOTIFICATION).to_be_bytes());
        // Layout per HTCommander: [4]=type 14, [5..8]=RX, [9..12]=TX, [13..16]=sub, [17..18]=flags
        // Our parse_body puts data after 4-byte header; event uses data[0] as type in some paths.
        // FreqModeStatusNotif::from_event_body expects full body with type at [4].
        let mut data = vec![0u8; 15];
        data[0] = NOTIF_FREQ_MODE_STATUS_CHANGED; // becomes msg[4]
        put_u32_be(&mut data, 1, encode_freq_hz(fm.rx_hz, 0));
        put_u32_be(&mut data, 5, encode_freq_hz(fm.tx_hz, 0));
        put_u32_be(&mut data, 9, u32::from(fm.tx_sub_audio));
        data[13] = 0x00;
        data[14] = 0x40; // active
        body.extend_from_slice(&data);
        self.enqueue_body(body);
    }

    fn push_freq_mode_notif_inactive(&mut self) {
        if !self.notif_regs.contains(&NOTIF_FREQ_MODE_STATUS_CHANGED) {
            return;
        }
        let mut body = Vec::new();
        body.extend_from_slice(&GROUP_BASIC.to_be_bytes());
        body.extend_from_slice(&reply_cmd(CMD_EVENT_NOTIFICATION).to_be_bytes());
        let mut data = vec![0u8; 15];
        data[0] = NOTIF_FREQ_MODE_STATUS_CHANGED;
        body.extend_from_slice(&data);
        self.enqueue_body(body);
    }
}

fn default_settings_reply(ch_a: u8, ch_b: u8, double_channel: u8) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&GROUP_BASIC.to_be_bytes());
    body.extend_from_slice(&reply_cmd(CMD_READ_SETTINGS).to_be_bytes());
    body.push(0); // status
                  // payload from offset 5 — keep room for vfo2 fields through index 24
    let mut payload = vec![0u8; 20];
    payload[0] = ((ch_a & 0x0F) << 4) | (ch_b & 0x0F);
    payload[1] = (double_channel & 0x03) << 4;
    payload[9] = (ch_a & 0xF0) | ((ch_b >> 4) & 0x0F);
    // Distinct VFO2 mod freq marker
    put_u32_be(&mut payload, 16, 0xA5A5_0002);
    body.extend_from_slice(&payload);
    body
}

/// Shared fake peer for injected Bluetooth / in-process transport.
#[derive(Clone)]
pub struct FakeBenshiPeer {
    inner: Arc<Mutex<Inner>>,
}

impl FakeBenshiPeer {
    pub fn new(cfg: FakeBenshiConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::new(cfg))),
        }
    }

    pub fn default_vr_n76() -> Self {
        Self::new(FakeBenshiConfig::default())
    }

    pub fn set_ptt(&self, on: bool) {
        self.inner.lock().unwrap().is_in_tx = on;
    }

    pub fn set_squelch_open(&self, open: bool) {
        let mut g = self.inner.lock().unwrap();
        g.is_sq = open;
        g.is_in_rx = open;
    }

    pub fn set_selected_vfo(&self, double_channel: u8) {
        let mut g = self.inner.lock().unwrap();
        g.double_channel = double_channel;
        if let Ok(s) = SettingsBlock::from_reply(&g.settings_reply) {
            let mut data = s.write_payload_channel_a_only(s.channel_a).unwrap();
            // force doubleChannel in data[1]
            data[1] = (data[1] & !0x30) | ((double_channel & 0x03) << 4);
            g.settings_reply[5..5 + data.len()].copy_from_slice(&data);
        }
    }

    pub fn settings_vfo2_fingerprint(&self) -> Vec<u8> {
        let g = self.inner.lock().unwrap();
        SettingsBlock::from_reply(&g.settings_reply)
            .map(|s| s.vfo2_fingerprint())
            .unwrap_or_default()
    }

    pub fn sent_commands(&self) -> Vec<(u16, u16)> {
        self.inner.lock().unwrap().sent_cmds.clone()
    }

    /// Host → radio bytes (GAIA frames).
    pub fn write_from_host(&self, bytes: &[u8]) -> Result<(), String> {
        let mut g = self.inner.lock().unwrap();
        let (frames, _) = gaia_decode(bytes)?;
        for f in frames {
            g.handle(&f)?;
        }
        Ok(())
    }

    /// Radio → host: pop one framed reply if available.
    pub fn read_to_host(&self) -> Option<Vec<u8>> {
        self.inner.lock().unwrap().rx_queue.pop_front()
    }

    pub fn channel(&self, id: u8) -> Option<ChannelRecord> {
        self.inner
            .lock()
            .unwrap()
            .channels
            .get(id as usize)
            .and_then(|c| c.clone())
    }
}
