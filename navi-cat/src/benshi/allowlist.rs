//! Never-transmit allowlist for Benshi basic-group commands.
//!
//! Commands outside this set are refused before I/O. The audio RFCOMM link is
//! never opened by this crate.

/// Basic command group (HTCommander / protocol reference).
pub const GROUP_BASIC: u16 = 2;
/// Firmware / GAIA VM group — always refused.
pub const GROUP_EXTENDED: u16 = 10;

/// Allowed outbound basic command ids (issue #149 table).
pub const ALLOWED_BASIC_CMDS: &[u16] = &[
    4,  // GET_DEV_INFO
    6,  // REGISTER_NOTIFICATION
    10, // READ_SETTINGS
    11, // WRITE_SETTINGS
    13, // READ_RF_CH
    14, // WRITE_RF_CH
    20, // GET_HT_STATUS
    35, // FREQ_MODE_SET_PAR
    36, // FREQ_MODE_GET_STATUS
];

/// Explicitly forbidden examples (must never ship).
pub const FORBIDDEN_BASIC_CMDS: &[u16] = &[
    12, // STORE_SETTINGS
    31, // HT_SEND_DATA
    66, // DO_PROG_FUNC
];

pub fn is_allowed(group: u16, cmd: u16) -> bool {
    if group == GROUP_EXTENDED {
        return false;
    }
    if group != GROUP_BASIC {
        return false;
    }
    let cmd = cmd & 0x7FFF;
    if FORBIDDEN_BASIC_CMDS.contains(&cmd) {
        return false;
    }
    ALLOWED_BASIC_CMDS.contains(&cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_matches_issue_table() {
        for c in ALLOWED_BASIC_CMDS {
            assert!(is_allowed(GROUP_BASIC, *c));
        }
        assert!(!is_allowed(GROUP_BASIC, 31));
        assert!(!is_allowed(GROUP_BASIC, 66));
        assert!(!is_allowed(GROUP_BASIC, 12));
        assert!(!is_allowed(GROUP_EXTENDED, 1));
    }
}
