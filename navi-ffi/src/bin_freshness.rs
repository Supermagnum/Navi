//! Refuse to run diag/gate binaries that are older than their Rust sources.
//!
//! The sandbox `CARGO_TARGET_DIR` trap previously let a stale
//! `navi-indexed-plan-bench` ignore `--t6` while reporting PASS.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn newest_mtime_under(dir: &Path, newest: &mut Option<SystemTime>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(name, "target" | ".git" | "node_modules") {
                continue;
            }
            newest_mtime_under(&p, newest);
            continue;
        }
        if p.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        if let Ok(meta) = p.metadata() {
            if let Ok(m) = meta.modified() {
                if newest.is_none_or(|n| m > n) {
                    *newest = Some(m);
                }
            }
        }
    }
}

fn git_head(repo: &Path) -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".into())
}

/// Print commit + binary mtime; exit process if binary is older than sources.
pub fn require_fresh_binary(label: &str) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{label}: WARN cannot resolve current_exe: {e}");
            return;
        }
    };
    let exe_mtime = match exe.metadata().and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{label}: WARN cannot read binary mtime: {e}");
            return;
        }
    };
    // navi-ffi/src/bin → navi-ffi → workspace root
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest_dir.clone());
    let mut newest: Option<SystemTime> = None;
    for rel in ["core/src", "navi-ffi/src", "navi-ffi/tests"] {
        newest_mtime_under(&root.join(rel), &mut newest);
    }
    let commit = git_head(&root);
    let exe_age = exe_mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    eprintln!(
        "{label}: binary={} commit={} binary_mtime_unix={exe_age} CARGO_TARGET_DIR={}",
        exe.display(),
        commit,
        std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "(unset/default)".into())
    );
    if let Some(src) = newest {
        // 3s slack: cargo write + concurrent editor/touch must not flake workers.
        let slack = std::time::Duration::from_secs(3);
        if exe_mtime + slack < src {
            let src_age = src
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // Isolated gate workers inherit a parent that already checked; a
            // mid-run source touch must not abort the remaining letter cases.
            if std::env::var_os("NAVI_GATE_WORKER").is_some() {
                eprintln!(
                    "{label}: WARN binary older than sources (binary_mtime_unix={exe_age} < newest_src_unix={src_age}); continuing worker"
                );
            } else {
                eprintln!(
                    "{label}: REFUSING stale binary (binary_mtime_unix={exe_age} < newest_src_unix={src_age}).\n                     Rebuild with unset CARGO_TARGET_DIR so the binary is written under the workspace target/."
                );
                std::process::exit(2);
            }
        }
    }
}
