//! Runtime feature flag for the MWM client path (default **on**).

use std::sync::atomic::{AtomicI8, Ordering};
use std::sync::{Mutex, MutexGuard};

/// Environment variable. Unset → enabled. Explicit disable:
/// `0` / `false` / `no` / `off`. Explicit enable: `1` / `true` / `yes` / `on`.
pub const MWM_CLIENT_ENABLED_ENV: &str = "NAVI_MWM_CLIENT_ENABLED";

/// `-1` = consult env; `0` = force off; `1` = force on (tests).
static OVERRIDE: AtomicI8 = AtomicI8::new(-1);

/// Serialises test overrides so parallel `cargo test` cannot interleave
/// `set`/`assert`/`clear` across tests (see `flag_off_blocks_fetch`).
static FLAG_TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_flag() -> MutexGuard<'static, ()> {
    FLAG_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Force the flag for tests. Pass `None` to restore env-based behaviour.
///
/// Prefer [`with_mwm_client_override`] so the lock is held for the whole
/// set → assert → clear critical section under parallel tests.
pub fn set_mwm_client_enabled_override(enabled: Option<bool>) {
    let _g = lock_flag();
    set_override_unlocked(enabled);
}

fn set_override_unlocked(enabled: Option<bool>) {
    let v = match enabled {
        None => -1,
        Some(false) => 0,
        Some(true) => 1,
    };
    OVERRIDE.store(v, Ordering::SeqCst);
}

/// Run `f` with a forced override while holding the test lock, then clear.
pub fn with_mwm_client_override<R>(enabled: Option<bool>, f: impl FnOnce() -> R) -> R {
    let _g = lock_flag();
    set_override_unlocked(enabled);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    set_override_unlocked(None);
    match result {
        Ok(r) => r,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

/// Whether MWM catalog/download APIs should be used.
///
/// Phase 8 / G4: always **true** for product builds (env OFF no longer selects
/// classic `.rkyv`). Unit tests may still force OFF via
/// [`set_mwm_client_enabled_override`] / [`with_mwm_client_override`].
pub fn mwm_client_enabled() -> bool {
    match OVERRIDE.load(Ordering::SeqCst) {
        -1 => true,
        0 => false,
        _ => true,
    }
}

#[cfg(test)]
fn parse_env_flag(raw: &str) -> Option<bool> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if t == "1"
        || t.eq_ignore_ascii_case("true")
        || t.eq_ignore_ascii_case("yes")
        || t.eq_ignore_ascii_case("on")
    {
        return Some(true);
    }
    if t == "0"
        || t.eq_ignore_ascii_case("false")
        || t.eq_ignore_ascii_case("no")
        || t.eq_ignore_ascii_case("off")
    {
        return Some(false);
    }
    // Unknown values: treat as disabled so a typo does not silently enable in
    // an intentional OFF debug session — but default (unset) remains ON.
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_forces_on_and_off() {
        with_mwm_client_override(Some(false), || {
            assert!(!mwm_client_enabled());
        });
        with_mwm_client_override(Some(true), || {
            assert!(mwm_client_enabled());
        });
    }

    #[test]
    fn parse_env_flag_matrix() {
        assert_eq!(parse_env_flag("1"), Some(true));
        assert_eq!(parse_env_flag("true"), Some(true));
        assert_eq!(parse_env_flag("YES"), Some(true));
        assert_eq!(parse_env_flag("on"), Some(true));
        assert_eq!(parse_env_flag("0"), Some(false));
        assert_eq!(parse_env_flag("false"), Some(false));
        assert_eq!(parse_env_flag("NO"), Some(false));
        assert_eq!(parse_env_flag("off"), Some(false));
        assert_eq!(parse_env_flag(""), None);
        assert_eq!(parse_env_flag("maybe"), Some(false));
    }

    #[test]
    fn default_when_override_cleared_is_enabled_if_env_unset() {
        with_mwm_client_override(None, || {
            if std::env::var(MWM_CLIENT_ENABLED_ENV).is_err() {
                assert!(mwm_client_enabled());
            }
        });
    }

    #[test]
    fn flag_off_blocks_under_parallel_stress() {
        // Re-entrant style: nested with_mwm_client_override would deadlock on
        // the same mutex — so stress via sequential holds only.
        for _ in 0..50 {
            with_mwm_client_override(Some(false), || {
                assert!(!mwm_client_enabled());
            });
            with_mwm_client_override(Some(true), || {
                assert!(mwm_client_enabled());
            });
        }
    }
}
