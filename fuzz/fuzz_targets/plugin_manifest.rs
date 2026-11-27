#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = navi_plugin_host::PluginManifest::from_json_str(s);
});
