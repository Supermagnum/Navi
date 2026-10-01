use std::path::{Path, PathBuf};
use std::process::Command;

use navi_plugin_host::smoke::{check_busy_loop, check_capability_deny, check_log_hello};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn cargo_target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
}

fn build_plugin(crate_dir: &Path) -> PathBuf {
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--manifest-path",
        ])
        .arg(crate_dir.join("Cargo.toml"))
        .status()
        .expect("spawn cargo build for plugin");
    assert!(status.success(), "plugin build failed for {crate_dir:?}");

    let name = crate_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace('-', "_");
    // crate navi-plugin-log-hello -> libnavi_plugin_log_hello.wasm
    let pkg = format!(
        "navi_plugin_{}",
        crate_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace('-', "_")
    );
    let release = cargo_target_dir().join("wasm32-unknown-unknown/release");
    let wasm = release.join(format!("lib{pkg}.wasm"));
    // Also try without lib prefix / with hyphenated names cargo emits
    let candidates = [
        wasm.clone(),
        release.join(format!("{pkg}.wasm")),
        release.join(format!("libnavi_plugin_{name}.wasm")),
    ];
    for c in &candidates {
        if c.is_file() {
            return c.clone();
        }
    }
    panic!("compiled wasm not found for {crate_dir:?}; tried {candidates:?}");
}

fn stage_plugin(plugin_dir_name: &str) -> PathBuf {
    let root = workspace_root();
    let src = root.join("plugins").join(plugin_dir_name);
    let wasm_src = build_plugin(&src);
    // Unique per call: isolation tests run in parallel and must not race on a
    // shared plugin.json (empty partial write → "EOF while parsing a value").
    let unique = format!(
        "{}-{}-{}",
        plugin_dir_name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let stage = root.join("target").join("plugin-fixtures").join(unique);
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::copy(src.join("plugin.json"), stage.join("plugin.json")).unwrap();
    std::fs::copy(&wasm_src, stage.join("plugin.wasm")).unwrap();
    stage
}

#[test]
fn load_call_log_hello_end_to_end() {
    let stage = stage_plugin("log-hello");
    check_log_hello(&stage).expect("log-hello load/call");
}

#[test]
fn busy_loop_is_killed_by_fuel_or_timeout_without_blocking_host() {
    let stage = stage_plugin("busy-loop");
    check_busy_loop(&stage).expect("busy-loop fuel/timeout kill");
}

#[test]
fn manifest_capability_checked_before_load() {
    let stage = stage_plugin("log-hello");
    check_capability_deny(&stage).expect("capability deny");
}
