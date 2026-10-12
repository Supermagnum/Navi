//! Native Benshi Bluetooth backend (VR-N76 and family).
//!
//! Protocol ported from HTCommander / benlink (Apache-2.0). Application logic
//! is not copied. Safety (allowlist, never-transmit, VFO 2 protection, gating)
//! is enforced here in the host, not in the WASM guest.
//!
//! Copyright notice for protocol facts derived from HTCommander:
//! Copyright 2026 Ylian Saint-Hilaire / HTCommander contributors
//! Licensed under the Apache License, Version 2.0.

pub mod allowlist;
pub mod backend;
pub mod codec;
pub mod gate;
pub mod sim;

pub use backend::{BenshiBackend, InProcessTransport, TuningMethod};
pub use gate::{gate_native_identity, ALLOWED_PRODUCT_VR_N76};
pub use sim::FakeBenshiPeer;
