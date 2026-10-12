//! Native-backend identity gate (distinct from Hamlib dump_caps Stable).

use crate::gating::{GateDecision, GateStatus};

/// HTCommander table: classic VR-N76 productId.
pub const ALLOWED_PRODUCT_VR_N76: u16 = 0x0002;
/// Live VR-N76 (this host's radio, 2026-10-12): GET_DEV_INFO reports 0x0103 (259).
/// Matches HTCommander firmware-server notes that key off live productId 259.
pub const ALLOWED_PRODUCT_VR_N76_FW: u16 = 0x0103;

const ALLOWED_PRODUCTS: &[u16] = &[ALLOWED_PRODUCT_VR_N76, ALLOWED_PRODUCT_VR_N76_FW];

pub fn is_allowed_vr_n76_product(product_id: u16) -> bool {
    ALLOWED_PRODUCTS.contains(&product_id)
}

/// Fail closed on unknown / malformed / truncated identity.
pub fn gate_native_identity(vendor_id: Option<u8>, product_id: Option<u16>) -> GateDecision {
    let Some(product_id) = product_id else {
        return refused("GET_DEV_INFO missing or truncated productId");
    };
    let Some(_vendor) = vendor_id else {
        return refused("GET_DEV_INFO missing or truncated vendorId");
    };
    if !ALLOWED_PRODUCTS.contains(&product_id) {
        return refused(format!(
            "productId 0x{product_id:04x} not on native allowlist (VR-N76=0x0002|0x0103)"
        ));
    }
    GateDecision {
        allowed: true,
        status: GateStatus::Stable,
        can_set_rptr_shift: true,
        can_set_rptr_offs: true,
        can_set_ctcss: true,
        reason: format!("native Benshi VR-N76 (0x{product_id:04x})"),
        beta_override_warning: None,
    }
}

fn refused(reason: impl Into<String>) -> GateDecision {
    GateDecision {
        allowed: false,
        status: GateStatus::Unknown,
        can_set_rptr_shift: false,
        can_set_rptr_offs: false,
        can_set_ctcss: false,
        reason: reason.into(),
        beta_override_warning: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vr_n76_allowed() {
        assert!(gate_native_identity(Some(1), Some(ALLOWED_PRODUCT_VR_N76)).allowed);
        assert!(gate_native_identity(Some(1), Some(ALLOWED_PRODUCT_VR_N76_FW)).allowed);
    }

    #[test]
    fn unknown_and_truncated_fail_closed() {
        assert!(!gate_native_identity(Some(1), Some(0x0001)).allowed);
        assert!(!gate_native_identity(Some(1), None).allowed);
        assert!(!gate_native_identity(None, Some(ALLOWED_PRODUCT_VR_N76)).allowed);
    }
}
