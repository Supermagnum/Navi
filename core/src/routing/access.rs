//! Static OSM access / barrier evaluation for profile-specific routing filters.
//!
//! Tag specificity (more specific wins over general `access`):
//! - Motor: `motor_vehicle` → else `access`
//! - Foot: `foot` → else `access`
//! - Bicycle: `bicycle` → else `access`
//!
//! Explicit yes-like values grant access even when `access=no`.
//! Explicit no-like values forbid. Unset → not forbidden here (highway class
//! filtering already decided the edge belongs in the profile graph).

use osm4routing::NodeId;
use std::collections::{HashMap, HashSet};

/// Values that explicitly forbid general through-access.
pub fn is_access_no(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "no" | "false" | "0" | "private"
    )
}

/// Values that explicitly permit access (override a general `access=no`).
pub fn is_access_yes(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "yes" | "true" | "1" | "designated" | "permissive" | "official"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    Motor,
    Foot,
    Bicycle,
}

/// True when static way/node tags forbid `mode` (OSM specificity rules).
pub fn mode_access_forbidden(
    mode: AccessMode,
    motor_vehicle: Option<&str>,
    access: Option<&str>,
    foot: Option<&str>,
    bicycle: Option<&str>,
) -> bool {
    let specific = match mode {
        AccessMode::Motor => motor_vehicle,
        AccessMode::Foot => foot,
        AccessMode::Bicycle => bicycle,
    };
    if let Some(v) = specific {
        if is_access_no(v) {
            return true;
        }
        if is_access_yes(v) {
            return false;
        }
        // Unknown specific value (e.g. `destination`, `customers`): treat as
        // restricted for through-routing.
        return true;
    }
    access.is_some_and(is_access_no)
}

pub fn tags_forbid_mode(tags: &HashMap<String, String>, mode: AccessMode) -> bool {
    mode_access_forbidden(
        mode,
        tags.get("motor_vehicle").map(String::as_str),
        tags.get("access").map(String::as_str),
        tags.get("foot").map(String::as_str),
        tags.get("bicycle").map(String::as_str),
    )
}

/// OSM `barrier=*` node that carries access tags relevant to routing.
pub fn barrier_node_forbids_mode(tags: &HashMap<String, String>, mode: AccessMode) -> bool {
    if !tags.contains_key("barrier") {
        return false;
    }
    tags_forbid_mode(tags, mode)
}

/// Collect mode-blocked barrier node ids from a tag map keyed by OSM node id.
pub fn blocked_barrier_nodes(
    node_tags: &HashMap<i64, HashMap<String, String>>,
    mode: AccessMode,
) -> HashSet<NodeId> {
    node_tags
        .iter()
        .filter(|(_, tags)| barrier_node_forbids_mode(tags, mode))
        .map(|(&id, _)| NodeId(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motor_vehicle_no_blocks_motor_not_foot() {
        assert!(mode_access_forbidden(
            AccessMode::Motor,
            Some("no"),
            None,
            None,
            None
        ));
        assert!(!mode_access_forbidden(
            AccessMode::Foot,
            Some("no"),
            None,
            None,
            None
        ));
        assert!(!mode_access_forbidden(
            AccessMode::Bicycle,
            Some("no"),
            None,
            None,
            None
        ));
    }

    #[test]
    fn access_no_blocks_all_unless_specific_yes() {
        assert!(mode_access_forbidden(
            AccessMode::Motor,
            None,
            Some("no"),
            None,
            None
        ));
        assert!(mode_access_forbidden(
            AccessMode::Foot,
            None,
            Some("no"),
            None,
            None
        ));
        assert!(!mode_access_forbidden(
            AccessMode::Foot,
            None,
            Some("no"),
            Some("yes"),
            None
        ));
        assert!(!mode_access_forbidden(
            AccessMode::Bicycle,
            None,
            Some("no"),
            None,
            Some("designated")
        ));
        assert!(mode_access_forbidden(
            AccessMode::Motor,
            None,
            Some("no"),
            Some("yes"),
            Some("yes")
        ));
    }

    #[test]
    fn specific_motor_yes_overrides_access_no() {
        assert!(!mode_access_forbidden(
            AccessMode::Motor,
            Some("yes"),
            Some("no"),
            None,
            None
        ));
    }

    #[test]
    fn bollard_with_motor_no_blocks_motor_only() {
        let mut tags = HashMap::new();
        tags.insert("barrier".into(), "bollard".into());
        tags.insert("motor_vehicle".into(), "no".into());
        assert!(barrier_node_forbids_mode(&tags, AccessMode::Motor));
        assert!(!barrier_node_forbids_mode(&tags, AccessMode::Foot));
        assert!(!barrier_node_forbids_mode(&tags, AccessMode::Bicycle));
    }

    #[test]
    fn barrier_without_access_tags_does_not_block() {
        let mut tags = HashMap::new();
        tags.insert("barrier".into(), "gate".into());
        assert!(!barrier_node_forbids_mode(&tags, AccessMode::Motor));
    }
}
