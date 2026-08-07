//! Frame budget classification and timing statistics.
//!
//! The thresholds live beside the enum they classify into, so a change to
//! one is made in sight of the other.

use std::time::Duration;

/// Target frame rate (120fps).
pub const TARGET_FPS: u32 = 120;

/// Target frame duration for 120fps (~8.33ms).
pub const TARGET_FRAME_DURATION: Duration = Duration::from_micros(8333);

/// Number of frames to track for averaging.
pub(super) const FRAME_HISTORY_SIZE: usize = 60;

/// Elapsed time at or past which a frame stops being [`FrameBudget::Comfortable`].
pub(super) const BUDGET_WARNING_THRESHOLD: Duration = Duration::from_millis(7);

/// Elapsed time at or past which a frame becomes [`FrameBudget::Critical`].
pub(super) const BUDGET_CRITICAL_THRESHOLD: Duration = Duration::from_millis(8);

/// Frame budget status for adaptive rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameBudget {
    /// Plenty of time remaining, can do extra work
    Comfortable,
    /// Running close to budget, be conservative
    Warning,
    /// Over budget, skip optional work
    Critical,
    /// Frame time exceeded budget
    Overrun,
}

/// Frame timing statistics.
#[derive(Debug, Clone)]
pub struct FrameStats {
    /// Average frame time over recent frames
    pub avg_frame_time: Duration,
    /// Minimum frame time in history
    pub min_frame_time: Duration,
    /// Maximum frame time in history
    pub max_frame_time: Duration,
    /// Current frames per second
    pub fps: f64,
    /// Number of dropped frames (over budget)
    pub dropped_frames: u64,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            avg_frame_time: Duration::ZERO,
            min_frame_time: Duration::MAX,
            max_frame_time: Duration::ZERO,
            fps: 0.0,
            dropped_frames: 0,
        }
    }
}
