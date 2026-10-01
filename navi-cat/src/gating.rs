//! Backend gating from `\dump_caps` / FFI caps.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Stable,
    Beta,
    Alpha,
    Untested,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateDecision {
    pub allowed: bool,
    pub status: GateStatus,
    pub can_set_rptr_shift: bool,
    pub can_set_rptr_offs: bool,
    pub can_set_ctcss: bool,
    pub reason: String,
    pub beta_override_warning: Option<String>,
}

fn parse_yn(v: &str) -> bool {
    matches!(v.trim().to_ascii_uppercase().as_str(), "Y" | "YES" | "1")
}

fn parse_status(v: &str) -> GateStatus {
    let s = v.trim().to_ascii_lowercase();
    if s.contains("stable") {
        GateStatus::Stable
    } else if s.contains("beta") {
        GateStatus::Beta
    } else if s.contains("alpha") {
        GateStatus::Alpha
    } else if s.contains("untested") || s == "new" {
        GateStatus::Untested
    } else {
        GateStatus::Unknown
    }
}

fn line_key_value(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once(':')?;
    Some((k.trim(), v.trim()))
}

fn is_shift_key(key: &str) -> bool {
    // Doc wording vs Hamlib 4.6+/4.7 "Repeater Duplex"
    key.eq_ignore_ascii_case("Can set Repeater Shift")
        || key.eq_ignore_ascii_case("Can set Repeater Duplex")
}

fn is_offset_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("Can set Repeater Offset")
}

fn is_ctcss_key(key: &str) -> bool {
    // Doc: "CTCSS Tone"; Hamlib 4.7: "CTCSS"
    key.eq_ignore_ascii_case("Can set CTCSS Tone") || key.eq_ignore_ascii_case("Can set CTCSS")
}

/// Parse Hamlib `\dump_caps` text. Fail closed on missing lines.
pub fn gate_from_dump_caps(text: &str, allow_beta: bool, allow_dummy: bool) -> GateDecision {
    let mut status = GateStatus::Unknown;
    let mut saw_status = false;
    let mut can_shift = false;
    let mut saw_shift = false;
    let mut can_offs = false;
    let mut saw_offs = false;
    let mut can_ctcss = false;
    let mut saw_ctcss = false;
    let mut model = 0i32;
    let mut model_name = String::new();

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Caps dump for model:") {
            model = rest.trim().parse().unwrap_or(model);
            continue;
        }
        let Some((key, val)) = line_key_value(line) else {
            continue;
        };
        if key.eq_ignore_ascii_case("Backend status") {
            status = parse_status(val);
            saw_status = true;
        } else if is_shift_key(key) {
            can_shift = parse_yn(val);
            saw_shift = true;
        } else if is_offset_key(key) {
            can_offs = parse_yn(val);
            saw_offs = true;
        } else if is_ctcss_key(key) {
            can_ctcss = parse_yn(val);
            saw_ctcss = true;
        } else if key.eq_ignore_ascii_case("Rig model") {
            model = val.parse().unwrap_or(0);
        } else if key.eq_ignore_ascii_case("Model name") {
            model_name = val.to_string();
        }
    }

    let is_dummy = model == 1 || model_name.eq_ignore_ascii_case("Dummy");
    if allow_dummy && is_dummy {
        return GateDecision {
            allowed: true,
            status: GateStatus::Stable,
            can_set_rptr_shift: true,
            can_set_rptr_offs: true,
            can_set_ctcss: true,
            reason: "dummy rig allowed in test build".into(),
            beta_override_warning: None,
        };
    }
    if is_dummy && !allow_dummy {
        return GateDecision {
            allowed: false,
            status,
            can_set_rptr_shift: can_shift,
            can_set_rptr_offs: can_offs,
            can_set_ctcss: can_ctcss,
            reason: "dummy rig (model 1) is not allowed outside test builds".into(),
            beta_override_warning: None,
        };
    }

    if !saw_status || !saw_shift || !saw_offs || !saw_ctcss {
        return GateDecision {
            allowed: false,
            status,
            can_set_rptr_shift: can_shift,
            can_set_rptr_offs: can_offs,
            can_set_ctcss: can_ctcss,
            reason: "dump_caps missing required lines (fail closed)".into(),
            beta_override_warning: None,
        };
    }

    let mut beta_warn = None;
    let status_ok = match status {
        GateStatus::Stable => true,
        GateStatus::Beta if allow_beta => {
            beta_warn = Some("Beta backend override enabled — use with care".into());
            true
        }
        GateStatus::Beta => false,
        GateStatus::Alpha | GateStatus::Untested | GateStatus::Unknown => false,
    };

    let funcs_ok = can_shift && can_offs && can_ctcss;
    let allowed = status_ok && funcs_ok;
    let reason = if allowed {
        "ok".into()
    } else if !status_ok {
        format!("backend status {:?} refused", status)
    } else {
        "radio cannot set shift/offset/CTCSS via CAT".into()
    };

    GateDecision {
        allowed,
        status,
        can_set_rptr_shift: can_shift,
        can_set_rptr_offs: can_offs,
        can_set_ctcss: can_ctcss,
        reason,
        beta_override_warning: beta_warn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STABLE: &str = r#"
Rig model: 2
Backend status: Stable
Can set Repeater Shift: Y
Can set Repeater Offset: Y
Can set CTCSS Tone: Y
"#;

    #[test]
    fn stable_passes() {
        let d = gate_from_dump_caps(STABLE, false, false);
        assert!(d.allowed);
        assert_eq!(d.status, GateStatus::Stable);
    }

    #[test]
    fn hamlib47_duplex_ctcss_wording() {
        let t = r#"
Caps dump for model: 1201
Backend status:	Stable
Can set Repeater Duplex:	Y
Can set Repeater Offset:	Y
Can set CTCSS:	Y
"#;
        let d = gate_from_dump_caps(t, false, false);
        assert!(d.allowed, "{}", d.reason);
    }

    #[test]
    fn beta_refused_without_override() {
        let t = STABLE.replace("Stable", "Beta");
        let d = gate_from_dump_caps(&t, false, false);
        assert!(!d.allowed);
    }

    #[test]
    fn beta_override_warns() {
        let t = STABLE.replace("Stable", "Beta");
        let d = gate_from_dump_caps(&t, true, false);
        assert!(d.allowed);
        assert!(d.beta_override_warning.is_some());
    }

    #[test]
    fn alpha_always_refused() {
        let t = STABLE.replace("Stable", "Alpha");
        let d = gate_from_dump_caps(&t, true, false);
        assert!(!d.allowed);
    }

    #[test]
    fn missing_line_fail_closed() {
        let t = "Backend status: Stable\nCan set Repeater Shift: Y\n";
        let d = gate_from_dump_caps(t, false, false);
        assert!(!d.allowed);
    }

    #[test]
    fn dummy_allowed_in_test() {
        let t = "Caps dump for model: 1\nModel name:\tDummy\nBackend status: Untested\n";
        let d = gate_from_dump_caps(t, false, true);
        assert!(d.allowed);
    }

    #[test]
    fn dummy_refused_outside_test() {
        let t = "Caps dump for model: 1\nModel name:\tDummy\nBackend status: Stable\nCan set Repeater Duplex: Y\nCan set Repeater Offset: Y\nCan set CTCSS: Y\n";
        let d = gate_from_dump_caps(t, false, false);
        assert!(!d.allowed);
    }
}
