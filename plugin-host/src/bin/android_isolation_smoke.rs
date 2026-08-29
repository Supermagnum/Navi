//! Android aarch64 isolation smoke binary.
//!
//! Expects two fixture directories (log-hello, busy-loop), each with
//! `plugin.json` + `plugin.wasm`. Built for `aarch64-linux-android` and run
//! under QEMU user-mode (NDK sysroot) or via `adb shell` on a real device.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let log_hello = match args.next() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!(
                "usage: android_isolation_smoke <log-hello-dir> <busy-loop-dir>\n\
                 each dir must contain plugin.json + plugin.wasm"
            );
            return ExitCode::from(2);
        }
    };
    let busy_loop = match args.next() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("usage: android_isolation_smoke <log-hello-dir> <busy-loop-dir>");
            return ExitCode::from(2);
        }
    };

    match navi_plugin_host::smoke::run_isolation_smoke(&log_hello, &busy_loop) {
        Ok(()) => {
            println!("android_isolation_smoke: all checks passed");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("android_isolation_smoke FAILED: {e:#}");
            ExitCode::FAILURE
        }
    }
}
