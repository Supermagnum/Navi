//! Runtime feature flag for the MWM client path (default **on**).

use std::sync::atomic::{AtomicI8, Ordering};

/// Environment variable. Unset → enabled. Explicit disable:
/// `0` / `false` / `no` / `off`. Explicit enable: `1` / `true` / `yes` / `on`.
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

fn env_enabled() -> bool {
    match std::env::var(MWM_CLIENT_ENABLED_ENV) {
        Ok(v) => parse_env_flag(&v).unwrap_or(true),
        Err(_) => true,
    }
}

/// Whether MWM catalog/download APIs should be used. Default **true** when the
/// env var is unset (all build types / F-Droid). Set
/// `NAVI_MWM_CLIENT_ENABLED=0` to force the classic `.rkyv` path for debugging.
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
    fn override_forces_on_and_off() {
        set_mwm_client_enabled_override(Some(false));
        assert!(!mwm_client_enabled());
        set_mwm_client_enabled_override(Some(true));
        assert!(mwm_client_enabled());
        set_mwm_client_enabled_override(None);
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
        set_mwm_client_enabled_override(None);
        // When the developer has not exported the env, default is ON.
        if std::env::var(MWM_CLIENT_ENABLED_ENV).is_err() {
            assert!(mwm_client_enabled());
        }
    }
}
