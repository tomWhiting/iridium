//! The headless GPU harness the pixel-level integration tests share.
//!
//! Three test binaries compose real frames on a real device and read the
//! pixels back. They used to carry a copy of this machinery each — the same
//! `Gpu`, the same `die`, the same texture descriptor, the same readback,
//! written out three times. That is precisely the shape of defect those tests
//! exist to catch: two copies of one decision, agreeing until something moves
//! one of them. A control that can drift from itself is not a control.
//!
//! The copies had already drifted. Only `retained_shaping.rs` checked whether
//! its readback buffer actually mapped; the two inset harnesses passed
//! `|_| {}` and threw the result away, so a failed map would have reached
//! `get_mapped_range` with no diagnosis at all. The version kept here is the
//! one that checks. See [`frame::read_pixels`].
//!
//! # Why `allow(dead_code)` and not `expect`
//!
//! Rust compiles an integration-test support module once per test binary, and
//! each binary uses a different subset: `top_inset.rs` scans rows and never
//! columns, `left_inset.rs` the reverse, `retained_shaping.rs` neither. An
//! item unused by one binary is not dead — it is used by the binary next door.
//!
//! `#[expect]` is the wrong tool for exactly that reason: it is strict in both
//! directions, so it would fire as *unfulfilled* in whichever binary happened
//! to use everything. `allow` is the honest attribute here, and it is scoped to
//! this module rather than set across the crate.
#![allow(
    dead_code,
    reason = "compiled once per test binary; each binary uses a different subset"
)]

pub mod frame;
pub mod gpu;
pub mod pixels;
pub mod scene;
