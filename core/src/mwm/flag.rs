//! Runtime feature flag for the MWM client path (default off).

use std::sync::atomic::{AtomicI8, Ordering};

/// Environment variable: `1` / `true` / `yes` / `on` enable; anything else (or
/// unset) keeps the classic `.rkyv` path.
pub const MWM_CLIENT_ENABLED_ENV: &str = "NAVI_MWM_CLIENT_ENABLED";

/// `-1` = consult env; `0` = force off; `1` = force on (tests).
static OVERRIDE: AtomicI8 = AtomicI8::new(-1);

/// Force the flag for tests. Pass `None` to restore env-based behaviour.
pub fn set_mwm_client_enabled_override(enabled: Option<bool>) {
    let v = match enabled {
        None => -1,
        Some(false) => 0,
        Some(true) => 1,
    };
    OVERRIDE.store(v, Ordering::SeqCst);
}

fn env_enabled() -> bool {
    match std::env::var(MWM_CLIENT_ENABLED_ENV) {
        Ok(v) => {
            let t = v.trim();
            t == "1"
                || t.eq_ignore_ascii_case("true")
                || t.eq_ignore_ascii_case("yes")
                || t.eq_ignore_ascii_case("on")
        }
        Err(_) => false,
    }
}

/// Whether MWM catalog/download APIs should be used. Default **false**.
pub fn mwm_client_enabled() -> bool {
    match OVERRIDE.load(Ordering::SeqCst) {
        0 => false,
        1 => true,
        _ => env_enabled(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_override_cleared_is_env_or_false() {
        set_mwm_client_enabled_override(None);
        // Unset in unit tests → false unless the developer exported the env.
        let _ = mwm_client_enabled();
        set_mwm_client_enabled_override(Some(false));
        assert!(!mwm_client_enabled());
        set_mwm_client_enabled_override(Some(true));
        assert!(mwm_client_enabled());
        set_mwm_client_enabled_override(None);
    }
}
