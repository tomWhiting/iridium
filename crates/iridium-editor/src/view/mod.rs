//! View layer for editor rendering and frame management.
//!
//! This module provides:
//! - Frame timing for 120fps rendering
//! - Line caching for large file performance
//! - Editor view integration

mod frame_timer;

pub use frame_timer::{
    DeltaTime, FrameBudget, FrameStats, FrameTimer, TARGET_FPS, TARGET_FRAME_DURATION,
};
