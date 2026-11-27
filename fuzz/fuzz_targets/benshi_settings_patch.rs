#![no_main]
use libfuzzer_sys::fuzz_target;
use navi_cat::benshi::codec::SettingsBlock;

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    if let Ok(sb) = SettingsBlock::from_reply(data) {
        let before = sb.channel_b_content_fingerprint();
        if let Ok(payload) = sb.write_payload_channel_a_only(7) {
            let mut reply = vec![0u8; 5];
            reply.extend_from_slice(&payload);
            if let Ok(again) = SettingsBlock::from_reply(&reply) {
                assert_eq!(before, again.channel_b_content_fingerprint());
            }
        }
        let _ = sb.write_payload_select_vfo_a();
    }
});
