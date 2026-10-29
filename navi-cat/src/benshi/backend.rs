//! `BenshiBackend`: RigBackend over GAIA / Benshi control RFCOMM.

use std::time::{Duration, Instant};

use crate::benshi::allowlist::{is_allowed, GROUP_BASIC};
use crate::benshi::codec::*;
use crate::benshi::gate::gate_native_identity;
use crate::benshi::sim::FakeBenshiPeer;
use crate::gating::GateDecision;
use crate::types::{RigBackend, RigError, ShiftDir, VfoState};

/// How the last successful program was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningMethod {
    FrequencyMode,
    ScratchChannel,
}

/// Byte transport (real RFCOMM or in-process fake peer).
pub trait BenshiTransport: Send {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), RigError>;
    /// Read available framed bytes; may return empty on timeout.
    fn read_some(&mut self, timeout: Duration) -> Result<Vec<u8>, RigError>;
}

/// In-process transport backed by [`FakeBenshiPeer`] (CI plumbing).
pub struct InProcessTransport {
    peer: FakeBenshiPeer,
}

impl InProcessTransport {
    pub fn new(peer: FakeBenshiPeer) -> Self {
        Self { peer }
    }

    pub fn peer(&self) -> &FakeBenshiPeer {
        &self.peer
    }
}

impl BenshiTransport for InProcessTransport {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), RigError> {
        self.peer.write_from_host(bytes).map_err(RigError::Protocol)
    }

    fn read_some(&mut self, _timeout: Duration) -> Result<Vec<u8>, RigError> {
        // Drain all currently queued frames (simulator is synchronous).
        let mut out = Vec::new();
        while let Some(frame) = self.peer.read_to_host() {
            out.extend_from_slice(&frame);
        }
        Ok(out)
    }
}

pub struct BenshiBackend<T: BenshiTransport> {
    transport: T,
    connected: bool,
    gate: GateDecision,
    model_name: String,
    product_id: u16,
    support_vfo: bool,
    /// Scratch channel index (preference).
    scratch_channel: u8,
    /// When false and VFO 2 is selected, defer retune (test F variant).
    allow_scratch_while_vfo2: bool,
    /// Q3: wait for fresh notification 14 after FREQ_MODE_SET_PAR.
    freq_mode_notif_timeout: Duration,
    /// Fall back to scratch if notification 14 never arrives / mismatches.
    fallback_scratch_on_notif_fail: bool,
    rx_buf: Vec<u8>,
    /// Decoded unframed bodies waiting to be consumed.
    pending: std::collections::VecDeque<Vec<u8>>,
    last_method: Option<TuningMethod>,
    last_vfo2_fp: Vec<u8>,
    /// Cached programmed VFO 1 state (for read-back when in freq mode).
    programmed: Option<VfoState>,
    reply_timeout: Duration,
    /// Generation counter for “fresh” notification 14 (incremented on each SET_PAR).
    freq_mode_write_gen: u64,
    last_seen_notif_gen: u64,
}

impl BenshiBackend<InProcessTransport> {
    /// Connect to an in-process fake peer (tests / demo).
    pub fn connect_fake(peer: FakeBenshiPeer, scratch_channel: u8) -> Result<Self, RigError> {
        let mut s = Self::new(InProcessTransport::new(peer), scratch_channel);
        s.handshake()?;
        Ok(s)
    }
}

impl<T: BenshiTransport> BenshiBackend<T> {
    pub fn new(transport: T, scratch_channel: u8) -> Self {
        Self {
            transport,
            connected: false,
            gate: GateDecision {
                allowed: false,
                status: crate::gating::GateStatus::Unknown,
                can_set_rptr_shift: false,
                can_set_rptr_offs: false,
                can_set_ctcss: false,
                reason: "not gated".into(),
                beta_override_warning: None,
            },
            model_name: "Benshi".into(),
            product_id: 0,
            support_vfo: false,
            scratch_channel,
            allow_scratch_while_vfo2: true,
            freq_mode_notif_timeout: Duration::from_millis(500),
            fallback_scratch_on_notif_fail: true,
            rx_buf: Vec::new(),
            pending: std::collections::VecDeque::new(),
            last_method: None,
            last_vfo2_fp: Vec::new(),
            programmed: None,
            reply_timeout: Duration::from_millis(800),
            freq_mode_write_gen: 0,
            last_seen_notif_gen: 0,
        }
    }

    pub fn set_allow_scratch_while_vfo2(&mut self, allow: bool) {
        self.allow_scratch_while_vfo2 = allow;
    }

    pub fn set_fallback_scratch_on_notif_fail(&mut self, v: bool) {
        self.fallback_scratch_on_notif_fail = v;
    }

    /// Hardware RFCOMM often needs a longer reply window than the in-process fake.
    pub fn set_reply_timeout(&mut self, d: Duration) {
        self.reply_timeout = d;
    }

    /// Drain and return any undecoded RX bytes (debug / hardware probe).
    pub fn take_rx_buf(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.rx_buf)
    }

    pub fn pending_frames(&self) -> usize {
        self.pending.len()
    }

    pub fn last_tuning_method(&self) -> Option<TuningMethod> {
        self.last_method
    }

    pub fn last_vfo2_fingerprint(&self) -> &[u8] {
        &self.last_vfo2_fp
    }

    /// Re-read settings and refresh the cached VFO 2 fingerprint.
    pub fn refresh_vfo2_fingerprint(&mut self) -> Result<Vec<u8>, RigError> {
        let fp = self.read_vfo2_fingerprint()?;
        self.last_vfo2_fp = fp.clone();
        Ok(fp)
    }

    /// Hardware-test / operator request only: select VFO A without changing
    /// channel B content. Not used by product auto-tune.
    pub fn select_vfo_a_for_display(&mut self) -> Result<(), RigError> {
        let before = self.read_settings()?;
        let b_fp = before.channel_b_content_fingerprint();
        let payload = before
            .write_payload_select_vfo_a()
            .map_err(RigError::Protocol)?;
        let _ = self.send_basic(CMD_WRITE_SETTINGS, payload)?;
        let after = self.read_settings()?;
        if after.channel_b_content_fingerprint() != b_fp {
            return Err(RigError::Protocol(
                "channel B content changed while selecting VFO A".into(),
            ));
        }
        if after.double_channel != 1 {
            return Err(RigError::Protocol(format!(
                "select VFO A failed: double_channel={}",
                after.double_channel
            )));
        }
        self.last_vfo2_fp = after.vfo2_fingerprint();
        Ok(())
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Refuse before I/O (never-transmit).
    pub fn send_command_checked(
        &mut self,
        group: u16,
        cmd: u16,
        data: Vec<u8>,
    ) -> Result<Vec<u8>, RigError> {
        if !is_allowed(group, cmd) {
            return Err(RigError::Unsupported(format!(
                "refusing Benshi cmd group={group} cmd={cmd} (never-transmit allowlist)"
            )));
        }
        let c = Command { group, cmd, data };
        self.transact(&c)
    }

    pub fn handshake(&mut self) -> Result<(), RigError> {
        let reply = self.send_basic(CMD_GET_DEV_INFO, GET_DEV_INFO_REQ_PAYLOAD.to_vec())?;
        let info = DevInfo::from_reply(&reply).map_err(RigError::Protocol)?;
        self.product_id = info.product_id;
        self.support_vfo = info.support_vfo;
        self.gate = gate_native_identity(Some(info.vendor_id), Some(info.product_id));
        if crate::benshi::gate::is_allowed_vr_n76_product(info.product_id) {
            self.model_name = "VR-N76".into();
        }
        if !self.gate.allowed {
            self.connected = false;
            return Err(RigError::Unsupported(self.gate.reason.clone()));
        }
        // REGISTER_NOTIFICATION has no reply on real firmware (HTCommander).
        self.send_basic_noreply(CMD_REGISTER_NOTIFICATION, vec![NOTIF_HT_STATUS_CHANGED])?;
        self.send_basic_noreply(
            CMD_REGISTER_NOTIFICATION,
            vec![NOTIF_FREQ_MODE_STATUS_CHANGED],
        )?;
        self.connected = true;
        self.last_vfo2_fp = self.read_vfo2_fingerprint()?;
        Ok(())
    }

    fn send_basic(&mut self, cmd: u16, data: Vec<u8>) -> Result<Vec<u8>, RigError> {
        self.send_command_checked(GROUP_BASIC, cmd, data)
    }

    fn send_basic_noreply(&mut self, cmd: u16, data: Vec<u8>) -> Result<(), RigError> {
        if !is_allowed(GROUP_BASIC, cmd) {
            return Err(RigError::Unsupported(format!(
                "refusing Benshi cmd group={GROUP_BASIC} cmd={cmd} (never-transmit allowlist)"
            )));
        }
        let c = Command {
            group: GROUP_BASIC,
            cmd,
            data,
        };
        self.transact_noreply(&c)
    }

    fn transact(&mut self, cmd: &Command) -> Result<Vec<u8>, RigError> {
        let body = cmd.to_body();
        let frame = gaia_encode(&body).map_err(RigError::Protocol)?;
        self.transport.write_all(&frame)?;
        let want = reply_cmd(cmd.cmd);
        let deadline = Instant::now() + self.reply_timeout;
        loop {
            self.pump_rx()?;
            if let Some(body) = self.take_matching_reply(want) {
                return Ok(body);
            }
            if Instant::now() >= deadline {
                return Err(RigError::Protocol(format!(
                    "timeout waiting for reply to cmd {}",
                    cmd.cmd
                )));
            }
        }
    }

    fn transact_noreply(&mut self, cmd: &Command) -> Result<(), RigError> {
        let body = cmd.to_body();
        let frame = gaia_encode(&body).map_err(RigError::Protocol)?;
        self.transport.write_all(&frame)?;
        // Brief drain so a surprise reply/notif does not stall the next command.
        let deadline = Instant::now() + Duration::from_millis(50);
        while Instant::now() < deadline {
            self.pump_rx()?;
        }
        Ok(())
    }

    fn pump_rx(&mut self) -> Result<(), RigError> {
        let chunk = self.transport.read_some(Duration::from_millis(1))?;
        self.rx_buf.extend_from_slice(&chunk);
        let (frames, n) = gaia_decode(&self.rx_buf).map_err(RigError::Protocol)?;
        if n > 0 {
            self.rx_buf.drain(..n);
        }
        for f in frames {
            self.pending.push_back(f);
        }
        Ok(())
    }

    fn take_matching_reply(&mut self, want_cmd: u16) -> Option<Vec<u8>> {
        let want = strip_response_bit(want_cmd);
        let mut i = 0;
        while i < self.pending.len() {
            let f = &self.pending[i];
            if f.len() >= 4 {
                let c = u16::from_be_bytes([f[2], f[3]]);
                if strip_response_bit(c) == want && (c & 0x8000) != 0 {
                    return self.pending.remove(i);
                }
            }
            i += 1;
        }
        None
    }

    /// Wait for a notification 14 that arrives after `since_gen` (Q3: no stale).
    fn wait_fresh_freq_mode_notif(
        &mut self,
        since_gen: u64,
        expect: &FreqModePar,
    ) -> Result<FreqModeStatusNotif, RigError> {
        let deadline = Instant::now() + self.freq_mode_notif_timeout;
        loop {
            self.pump_rx()?;
            let mut i = 0;
            while i < self.pending.len() {
                let f = self.pending[i].clone();
                let cmd = match parse_body(&f) {
                    Ok(c) => c,
                    Err(_) => {
                        i += 1;
                        continue;
                    }
                };
                let req = strip_response_bit(cmd.cmd);
                if req != CMD_EVENT_NOTIFICATION
                    || cmd.data.first().copied() != Some(NOTIF_FREQ_MODE_STATUS_CHANGED)
                {
                    i += 1;
                    continue;
                }
                // Drop this event from the queue either way.
                self.pending.remove(i);
                if self.freq_mode_write_gen <= since_gen {
                    // Stale relative to this write — discard, keep waiting.
                    continue;
                }
                let n = FreqModeStatusNotif::from_event_body(&f).map_err(RigError::Protocol)?;
                self.last_seen_notif_gen = self.freq_mode_write_gen;
                if !n.active {
                    return Err(RigError::Protocol(
                        "notification 14 inactive after FREQ_MODE_SET_PAR".into(),
                    ));
                }
                if n.rx_hz != expect.rx_hz
                    || n.tx_hz != expect.tx_hz
                    || (n.sub_audio as u16) != expect.tx_sub_audio
                {
                    return Err(RigError::Protocol(format!(
                        "notification 14 mismatch: got rx={} tx={} sa={} want rx={} tx={} sa={}",
                        n.rx_hz,
                        n.tx_hz,
                        n.sub_audio,
                        expect.rx_hz,
                        expect.tx_hz,
                        expect.tx_sub_audio
                    )));
                }
                return Ok(n);
            }
            if Instant::now() >= deadline {
                return Err(RigError::Protocol(
                    "timeout waiting for fresh notification 14".into(),
                ));
            }
        }
    }

    fn read_settings(&mut self) -> Result<SettingsBlock, RigError> {
        let reply = self.send_basic(CMD_READ_SETTINGS, vec![])?;
        SettingsBlock::from_reply(&reply).map_err(RigError::Protocol)
    }

    fn read_vfo2_fingerprint(&mut self) -> Result<Vec<u8>, RigError> {
        let s = self.read_settings()?;
        Ok(s.vfo2_fingerprint())
    }

    fn read_ht_status(&mut self) -> Result<HtStatus, RigError> {
        let reply = self.send_basic(CMD_GET_HT_STATUS, vec![])?;
        HtStatus::from_reply(&reply).map_err(RigError::Protocol)
    }

    fn read_channel(&mut self, id: u8) -> Result<ChannelRecord, RigError> {
        let reply = self.send_basic(CMD_READ_RF_CH, vec![id])?;
        ChannelRecord::from_reply(&reply).map_err(RigError::Protocol)
    }

    fn write_channel(&mut self, ch: &ChannelRecord) -> Result<(), RigError> {
        let payload = ch.to_write_payload();
        let _ = self.send_basic(CMD_WRITE_RF_CH, payload.to_vec())?;
        Ok(())
    }

    fn assert_vfo2_unchanged(&mut self, before: &[u8]) -> Result<(), RigError> {
        let after = self.read_vfo2_fingerprint()?;
        if after != before {
            return Err(RigError::Protocol(format!(
                "VFO 2 fingerprint changed: before={before:?} after={after:?}"
            )));
        }
        self.last_vfo2_fp = after;
        Ok(())
    }

    fn want_to_rx_tx(want: &VfoState) -> Result<(u32, u32, u16), RigError> {
        let rx = want.freq_hz as u32;
        let tx = match want.shift {
            ShiftDir::None | ShiftDir::Leave => rx,
            ShiftDir::Minus => rx.saturating_sub(want.offset_hz as u32),
            ShiftDir::Plus => rx.saturating_add(want.offset_hz as u32),
        };
        let sa = match (want.ctcss_tenths_hz, want.dcs_code) {
            (Some(t), _) => tenths_to_subaudio(t),
            (None, Some(d)) => d as u16,
            (None, None) => 0,
        };
        Ok((rx, tx, sa))
    }

    fn choose_method(&mut self) -> Result<TuningMethod, RigError> {
        let st = self.read_ht_status()?;
        if st.is_scan {
            return Err(RigError::Unsupported("deferred: radio is scanning".into()));
        }
        // Q1: frequency mode only when VFO 1 selected or dual off.
        if self.support_vfo && st.vfo1_selected_or_single() {
            return Ok(TuningMethod::FrequencyMode);
        }
        if st.double_channel == 2 && !self.allow_scratch_while_vfo2 {
            return Err(RigError::Unsupported(
                "deferred: waiting for VFO 1 (no VFO-1-addressed method while VFO 2 selected)"
                    .into(),
            ));
        }
        Ok(TuningMethod::ScratchChannel)
    }

    fn program_freq_mode(&mut self, want: &VfoState) -> Result<(), RigError> {
        let (rx, tx, sa) = Self::want_to_rx_tx(want)?;
        let par = FreqModePar::tune(rx, tx, sa);
        let before_gen = self.freq_mode_write_gen;
        self.freq_mode_write_gen = before_gen + 1;
        let _ = self.send_basic(CMD_FREQ_MODE_SET_PAR, par.to_payload().to_vec())?;
        match self.wait_fresh_freq_mode_notif(before_gen, &par) {
            Ok(_) => {
                self.last_method = Some(TuningMethod::FrequencyMode);
                self.programmed = Some(want.clone());
                Ok(())
            }
            Err(e) if self.fallback_scratch_on_notif_fail => {
                log::warn!("freq mode notif 14 failed ({e}); falling back to scratch");
                // Leave frequency mode first.
                let _ = self.send_basic(
                    CMD_FREQ_MODE_SET_PAR,
                    FreqModePar::stop().to_payload().to_vec(),
                );
                self.program_scratch(want)
            }
            Err(e) => Err(e),
        }
    }

    fn program_scratch(&mut self, want: &VfoState) -> Result<(), RigError> {
        let (rx, tx, sa) = Self::want_to_rx_tx(want)?;
        let settings = self.read_settings()?;
        let vfo2_before = settings.vfo2_fingerprint();
        let id = self.scratch_channel;
        let mut name = [0u8; 10];
        name[..3].copy_from_slice(b"NAV");
        let ch = ChannelRecord {
            channel_id: id,
            tx_freq_hz: tx,
            rx_freq_hz: rx,
            tx_sub_audio: sa,
            rx_sub_audio: 0,
            bandwidth_wide: want.passband_hz >= 20_000,
            raw_flags18: 0x40, // tx at max power bit; bandwidth applied in to_write
            raw_flags19: 0,
            name,
        };
        self.write_channel(&ch)?;
        // Point channel A at scratch without touching channel B / doubleChannel.
        if settings.channel_a != id {
            let payload = settings
                .write_payload_channel_a_only(id)
                .map_err(RigError::Protocol)?;
            let _ = self.send_basic(CMD_WRITE_SETTINGS, payload)?;
        }
        // Read-back channel
        let got = self.read_channel(id)?;
        if got.rx_freq_hz != rx || got.tx_freq_hz != tx || got.tx_sub_audio != sa {
            return Err(RigError::Protocol(format!(
                "scratch read-back mismatch rx/tx/sa got {}/{}/{} want {}/{}/{}",
                got.rx_freq_hz, got.tx_freq_hz, got.tx_sub_audio, rx, tx, sa
            )));
        }
        self.assert_vfo2_unchanged(&vfo2_before)?;
        // Also confirm selected VFO (double_channel) unchanged.
        let st = self.read_ht_status()?;
        let settings_after = self.read_settings()?;
        if settings_after.double_channel != settings.double_channel
            || settings_after.channel_b != settings.channel_b
        {
            return Err(RigError::Protocol(
                "selected VFO or channel B changed during scratch program".into(),
            ));
        }
        let _ = st;
        self.last_method = Some(TuningMethod::ScratchChannel);
        self.programmed = Some(want.clone());
        Ok(())
    }
}

impl<T: BenshiTransport> RigBackend for BenshiBackend<T> {
    fn is_connected(&self) -> bool {
        self.connected && self.gate.allowed
    }
    fn model_name(&self) -> String {
        self.model_name.clone()
    }
    fn model_number(&self) -> i32 {
        i32::from(self.product_id)
    }
    fn gate(&self) -> GateDecision {
        self.gate.clone()
    }
    fn get_ptt(&mut self) -> Result<bool, RigError> {
        Ok(self.read_ht_status()?.is_in_tx)
    }
    fn get_dcd(&mut self) -> Result<bool, RigError> {
        let st = self.read_ht_status()?;
        Ok(st.is_sq || st.is_in_rx)
    }
    fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError> {
        if !self.gate.allowed {
            return Err(RigError::Unsupported(self.gate.reason.clone()));
        }
        let st = self.read_ht_status()?;
        if st.is_in_tx {
            return Err(RigError::Unsupported(
                "refuse program while PTT (isInTx)".into(),
            ));
        }
        if st.is_sq || st.is_in_rx {
            return Err(RigError::Unsupported("deferred: squelch open".into()));
        }
        let vfo2_before = self.read_vfo2_fingerprint()?;
        let method = self.choose_method()?;
        match method {
            TuningMethod::FrequencyMode => self.program_freq_mode(want)?,
            TuningMethod::ScratchChannel => self.program_scratch(want)?,
        }
        self.assert_vfo2_unchanged(&vfo2_before)?;
        Ok(())
    }
    fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
        // Prefer channel / notification-backed state over cached.
        match self.last_method {
            Some(TuningMethod::ScratchChannel) => {
                let ch = self.read_channel(self.scratch_channel)?;
                let (shift, offset) = if ch.tx_freq_hz == ch.rx_freq_hz {
                    (ShiftDir::None, 0u64)
                } else if ch.tx_freq_hz < ch.rx_freq_hz {
                    (ShiftDir::Minus, u64::from(ch.rx_freq_hz - ch.tx_freq_hz))
                } else {
                    (ShiftDir::Plus, u64::from(ch.tx_freq_hz - ch.rx_freq_hz))
                };
                Ok(VfoState {
                    vfo: "VFOA".into(),
                    freq_hz: u64::from(ch.rx_freq_hz),
                    mode: "FM".into(),
                    passband_hz: if ch.bandwidth_wide { 25_000 } else { 12_500 },
                    shift,
                    offset_hz: offset,
                    ctcss_tenths_hz: subaudio_to_tenths(ch.tx_sub_audio),
                    dcs_code: if ch.tx_sub_audio > 0 && ch.tx_sub_audio < 6700 {
                        Some(u32::from(ch.tx_sub_audio))
                    } else {
                        None
                    },
                })
            }
            Some(TuningMethod::FrequencyMode) => {
                // Re-verify via a GET_STATUS + last notif-backed programmed state.
                if let Some(p) = &self.programmed {
                    return Ok(p.clone());
                }
                Err(RigError::Unsupported(
                    "frequency mode state unverified".into(),
                ))
            }
            None => self
                .programmed
                .clone()
                .ok_or_else(|| RigError::Protocol("no programmed state".into())),
        }
    }
    fn freq_step_hz(&self) -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benshi::sim::FakeBenshiConfig;
    use crate::program::{program_vfo1_verified, ProgramRequest};

    fn la5mr_req() -> ProgramRequest {
        ProgramRequest {
            freq_out_mhz: 145.725,
            shift_mhz: -0.6,
            ctcss_hz: Some(88.5),
            dcs_code: None,
            mode: "FM".into(),
            passband_hz: 12_500,
            vfo: "VFOA".into(),
            leaves_follow: false,
        }
    }

    #[test]
    fn a_fake_peer_and_c_program_both_methods() {
        let peer = FakeBenshiPeer::default_vr_n76();
        let mut backend = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
        assert!(backend.is_connected());
        let reported = program_vfo1_verified(&mut backend, &la5mr_req()).unwrap();
        assert_eq!(reported.freq_hz, 145_725_000);
        assert_eq!(reported.offset_hz, 600_000);
        assert_eq!(reported.ctcss_tenths_hz, Some(885));
        assert_eq!(
            backend.last_tuning_method(),
            Some(TuningMethod::FrequencyMode)
        );

        // Force scratch: select VFO 2.
        peer.set_selected_vfo(2);
        let mut backend2 = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
        let reported2 = program_vfo1_verified(&mut backend2, &la5mr_req()).unwrap();
        assert_eq!(reported2.freq_hz, 145_725_000);
        assert_eq!(
            backend2.last_tuning_method(),
            Some(TuningMethod::ScratchChannel)
        );
        // Second independent connection read-back
        let mut reader = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
        let ch = reader.read_channel(0).unwrap();
        assert_eq!(ch.rx_freq_hz, 145_725_000);
        assert_eq!(ch.tx_freq_hz, 145_125_000);
        assert_eq!(ch.tx_sub_audio, 8850);
    }

    #[test]
    fn d_never_transmit_refuses_forbidden() {
        let peer = FakeBenshiPeer::default_vr_n76();
        let mut backend = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
        let err = backend
            .send_command_checked(GROUP_BASIC, 31, vec![])
            .unwrap_err();
        assert!(matches!(err, RigError::Unsupported(_)));
        let err2 = backend
            .send_command_checked(GROUP_BASIC, 66, vec![])
            .unwrap_err();
        assert!(matches!(err2, RigError::Unsupported(_)));
        // Ensure peer never saw forbidden cmds
        let sent = peer.sent_commands();
        assert!(!sent.iter().any(|(_, c)| *c == 31 || *c == 66));
    }

    #[test]
    fn e_interlocks_ptt_and_squelch() {
        let peer = FakeBenshiPeer::default_vr_n76();
        let mut backend = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
        peer.set_ptt(true);
        assert!(program_vfo1_verified(&mut backend, &la5mr_req()).is_err());
        peer.set_ptt(false);
        peer.set_squelch_open(true);
        let err = backend
            .set_vfo_state(&crate::types::VfoState {
                vfo: "VFOA".into(),
                freq_hz: 145_725_000,
                mode: "FM".into(),
                passband_hz: 12_500,
                shift: ShiftDir::Minus,
                offset_hz: 600_000,
                ctcss_tenths_hz: Some(885),
                dcs_code: None,
            })
            .unwrap_err();
        assert!(format!("{err}").contains("squelch"));
        peer.set_squelch_open(false);
        program_vfo1_verified(&mut backend, &la5mr_req()).unwrap();
    }

    #[test]
    fn f_vfo2_untouched_and_background_retune() {
        let peer = FakeBenshiPeer::default_vr_n76();
        peer.set_selected_vfo(2);
        let fp_before = peer.settings_vfo2_fingerprint();
        let mut backend = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
        program_vfo1_verified(&mut backend, &la5mr_req()).unwrap();
        assert_eq!(peer.settings_vfo2_fingerprint(), fp_before);
        assert_eq!(
            backend.last_tuning_method(),
            Some(TuningMethod::ScratchChannel)
        );
        // VFO 1 scratch has new repeater
        let ch = peer.channel(0).unwrap();
        assert_eq!(ch.rx_freq_hz, 145_725_000);
        // Channel 1 (VFO2 default) unchanged
        let ch1 = peer.channel(1).unwrap();
        assert_eq!(ch1.rx_freq_hz, 145_500_000);

        // Variant: no VFO1 method while on VFO2 → defer
        let mut backend2 = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
        backend2.set_allow_scratch_while_vfo2(false);
        let err = backend2.set_vfo_state(&VfoState {
            vfo: "VFOA".into(),
            freq_hz: 145_500_000,
            mode: "FM".into(),
            passband_hz: 12_500,
            shift: ShiftDir::None,
            offset_hz: 0,
            ctcss_tenths_hz: None,
            dcs_code: None,
        });
        assert!(format!("{}", err.unwrap_err()).contains("waiting for VFO 1"));
        peer.set_selected_vfo(1);
        program_vfo1_verified(&mut backend2, &la5mr_req()).unwrap();
    }

    #[test]
    fn g_gating_unknown_product() {
        let cfg = FakeBenshiConfig {
            product_id: 0x0001,
            ..Default::default()
        };
        let peer = FakeBenshiPeer::new(cfg);
        let err = BenshiBackend::connect_fake(peer, 0).err().expect("gated");
        assert!(matches!(err, RigError::Unsupported(_)));
    }
}
