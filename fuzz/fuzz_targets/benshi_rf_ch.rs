#![no_main]
use libfuzzer_sys::fuzz_target;
use navi_cat::benshi::codec::ChannelRecord;

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    if let Ok(ch) = ChannelRecord::from_reply(data) {
        let payload = ch.to_write_payload();
        let mut reply = vec![0u8; 30];
        reply[4] = 0;
        reply[5..30].copy_from_slice(&payload);
        let _ = ChannelRecord::from_reply(&reply);
    }
});
