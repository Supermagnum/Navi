//! Native-backend identity gate (distinct from Hamlib dump_caps Stable).

use crate::gating::{GateDecision, GateStatus};

/// HTCommander productId for VGC/Vero VR-N76.
pub const ALLOWED_PRODUCT_VR_N76: u16 = 0x0002;

/// Fail closed on unknown / malformed / truncated identity.
pub fn gate_native_identity(vendor_id: Option<u8>, product_id: Option<u16>) -> GateDecision {
    let Some(product_id) = product_id else {
        return refused("GET_DEV_INFO missing or truncated productId");
    };
    let Some(_vendor) = vendor_id else {
        return refused("GET_DEV_INFO missing or truncated vendorId");
    };
    if product_id != ALLOWED_PRODUCT_VR_N76 {
        return refused(format!(
            "productId 0x{product_id:04x} not on native allowlist (VR-N76=0x0002)"
        ));
    }
    GateDecision {
        allowed: true,
        status: GateStatus::Stable,
        can_set_rptr_shift: true,
        can_set_rptr_offs: true,
        can_set_ctcss: true,
        reason: "native Benshi VR-N76 (0x0002)".into(),
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
        let g = gate_native_identity(Some(1), Some(ALLOWED_PRODUCT_VR_N76));
        assert!(g.allowed);
    }

    #[test]
    fn unknown_and_truncated_fail_closed() {
        assert!(!gate_native_identity(Some(1), Some(0x0001)).allowed);
        assert!(!gate_native_identity(Some(1), None).allowed);
        assert!(!gate_native_identity(None, Some(ALLOWED_PRODUCT_VR_N76)).allowed);
    }
}
