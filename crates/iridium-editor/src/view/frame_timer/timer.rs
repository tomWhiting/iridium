//! The frame timer itself.

use std::collections::VecDeque;
use std::time::Duration;

use web_time::Instant;

use super::budget::{
    BUDGET_CRITICAL_THRESHOLD, BUDGET_WARNING_THRESHOLD, FRAME_HISTORY_SIZE, FrameBudget,
    FrameStats, TARGET_FPS,
};

/// Frame timer for managing 120fps rendering.
///
/// This timer tracks frame timing and provides utilities for:
/// - Consistent frame pacing
/// - Frame budget management
/// - Performance statistics
///
/// Every clock-dependent method has an `_at` twin that takes the instant
/// explicitly, so that a test can ask about the arithmetic without asking
/// whether the operating system scheduled a thread on time.
///
/// # Usage Pattern
///
/// ```ignore
/// let mut timer = FrameTimer::new();
///
/// loop {
///     timer.begin_frame();
///
///     // Rendering work...
///     render_text();
///
///     // Check if we have time for optional work
///     if timer.current_budget() != FrameBudget::Critical {
///         render_minimap();
///     }
///
///     // Wait for next frame
///     timer.end_frame();
///     let wait = timer.time_until_next_frame();
///     if !wait.is_zero() {
///         std::thread::sleep(wait);
///     }
/// }
/// ```
#[derive(Debug)]
pub struct FrameTimer {
    /// When the current frame started
    frame_start: Instant,
    /// When the last frame ended
    last_frame_end: Instant,
    /// History of frame times for statistics
    frame_times: VecDeque<Duration>,
    /// Running total of frame times (for efficient average)
    frame_time_sum: Duration,
    /// Target frame duration
    target_duration: Duration,
    /// Number of dropped frames
    dropped_frames: u64,
    /// Whether we're currently in a frame
    in_frame: bool,
}

impl Default for FrameTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameTimer {
    /// Creates a new frame timer targeting 120fps.
    #[must_use]
    pub fn new() -> Self {
        Self::with_target_fps(TARGET_FPS)
    }

    /// Creates a new frame timer targeting 120fps, started at `now`.
    #[must_use]
    pub fn new_at(now: Instant) -> Self {
        Self::with_target_fps_at(TARGET_FPS, now)
    }

    /// Creates a new frame timer with a custom target frame rate.
    ///
    /// A target of zero is treated as one frame per second: a zero target
    /// has no finite frame duration, and returning a timer is preferable
    /// to refusing one for a value a caller is unlikely to have meant.
    #[must_use]
    pub fn with_target_fps(fps: u32) -> Self {
        Self::with_target_fps_at(fps, Instant::now())
    }

    /// Creates a new frame timer with a custom target frame rate, started
    /// at `now`.
    ///
    /// See [`with_target_fps`](Self::with_target_fps) for the handling of a
    /// zero target.
    #[must_use]
    pub fn with_target_fps_at(fps: u32, now: Instant) -> Self {
        // `1.0 / 0.0` is infinite and `Duration::from_secs_f64` panics on a
        // non-finite value, so the divisor is floored at one.
        let target_duration = Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));

        Self {
            frame_start: now,
            last_frame_end: now,
            frame_times: VecDeque::with_capacity(FRAME_HISTORY_SIZE),
            frame_time_sum: Duration::ZERO,
            target_duration,
            dropped_frames: 0,
            in_frame: false,
        }
    }

    /// Returns the target frame duration.
    #[must_use]
    pub const fn target_duration(&self) -> Duration {
        self.target_duration
    }

    /// Returns the target frames per second.
    #[must_use]
    pub fn target_fps(&self) -> f64 {
        1.0 / self.target_duration.as_secs_f64()
    }

    /// Marks the beginning of a frame.
    ///
    /// Call this at the start of your render loop iteration.
    pub fn begin_frame(&mut self) {
        self.begin_frame_at(Instant::now());
    }

    /// Marks the beginning of a frame that started at `now`.
    pub const fn begin_frame_at(&mut self, now: Instant) {
        self.frame_start = now;
        self.in_frame = true;
    }

    /// Marks the end of a frame and records timing.
    ///
    /// Call this at the end of your render loop iteration.
    /// Returns the frame duration.
    pub fn end_frame(&mut self) -> Duration {
        self.end_frame_at(Instant::now())
    }

    /// Marks the end of a frame that ended at `now`, and records timing.
    ///
    /// Returns the frame duration. An instant earlier than the frame's
    /// start yields a zero duration rather than underflowing.
    pub fn end_frame_at(&mut self, now: Instant) -> Duration {
        let frame_time = now.saturating_duration_since(self.frame_start);
        self.last_frame_end = now;
        self.in_frame = false;

        // Track dropped frames
        if frame_time > self.target_duration {
            self.dropped_frames += 1;
        }

        // Add to history
        self.frame_times.push_back(frame_time);
        self.frame_time_sum += frame_time;

        // Remove old entries
        while self.frame_times.len() > FRAME_HISTORY_SIZE {
            if let Some(old) = self.frame_times.pop_front() {
                self.frame_time_sum = self.frame_time_sum.saturating_sub(old);
            }
        }

        frame_time
    }

    /// Returns the elapsed time since frame start.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed_at(Instant::now())
    }

    /// Returns the time elapsed from the frame's start to `now`.
    ///
    /// An instant earlier than the frame's start yields a zero duration.
    #[must_use]
    pub fn elapsed_at(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.frame_start)
    }

    /// Returns the time remaining in the current frame budget.
    #[must_use]
    pub fn remaining(&self) -> Duration {
        self.remaining_at(Instant::now())
    }

    /// Returns the budget remaining at `now`, saturating at zero once the
    /// target duration has been spent.
    #[must_use]
    pub fn remaining_at(&self, now: Instant) -> Duration {
        self.target_duration.saturating_sub(self.elapsed_at(now))
    }

    /// Returns the time until the next frame should start.
    ///
    /// Returns `Duration::ZERO` if we're already past the target time.
    #[must_use]
    pub fn time_until_next_frame(&self) -> Duration {
        self.time_until_next_frame_at(Instant::now())
    }

    /// Returns the time from `now` until the next frame should start.
    ///
    /// Returns `Duration::ZERO` if `now` is already past the target time.
    #[must_use]
    pub fn time_until_next_frame_at(&self, now: Instant) -> Duration {
        let since_last = now.saturating_duration_since(self.last_frame_end);
        self.target_duration.saturating_sub(since_last)
    }

    /// Returns the current frame budget status.
    #[must_use]
    pub fn current_budget(&self) -> FrameBudget {
        self.current_budget_at(Instant::now())
    }

    /// Returns the frame budget status as of `now`.
    #[must_use]
    pub fn current_budget_at(&self, now: Instant) -> FrameBudget {
        let elapsed = self.elapsed_at(now);

        if elapsed >= self.target_duration {
            FrameBudget::Overrun
        } else if elapsed >= BUDGET_CRITICAL_THRESHOLD {
            FrameBudget::Critical
        } else if elapsed >= BUDGET_WARNING_THRESHOLD {
            FrameBudget::Warning
        } else {
            FrameBudget::Comfortable
        }
    }

    /// Returns true if there's time for optional work.
    #[must_use]
    pub fn has_budget_for_optional_work(&self) -> bool {
        self.has_budget_for_optional_work_at(Instant::now())
    }

    /// Returns whether there is time for optional work as of `now`.
    #[must_use]
    pub fn has_budget_for_optional_work_at(&self, now: Instant) -> bool {
        matches!(
            self.current_budget_at(now),
            FrameBudget::Comfortable | FrameBudget::Warning
        )
    }

    /// Returns frame timing statistics.
    #[must_use]
    pub fn stats(&self) -> FrameStats {
        if self.frame_times.is_empty() {
            return FrameStats::default();
        }

        // The history is trimmed to `FRAME_HISTORY_SIZE` on every push, so the
        // conversion cannot saturate; the guard above rules out a zero divisor.
        let count = u32::try_from(self.frame_times.len()).unwrap_or(u32::MAX);
        let avg_frame_time = self.frame_time_sum / count;

        let mut min_frame_time = Duration::MAX;
        let mut max_frame_time = Duration::ZERO;

        for &time in &self.frame_times {
            min_frame_time = min_frame_time.min(time);
            max_frame_time = max_frame_time.max(time);
        }

        let fps = 1.0 / avg_frame_time.as_secs_f64();

        FrameStats {
            avg_frame_time,
            min_frame_time,
            max_frame_time,
            fps,
            dropped_frames: self.dropped_frames,
        }
    }

    /// Returns the average frame time.
    #[must_use]
    pub fn avg_frame_time(&self) -> Duration {
        if self.frame_times.is_empty() {
            Duration::ZERO
        } else {
            // Bounded by `FRAME_HISTORY_SIZE`, so the conversion cannot saturate.
            let count = u32::try_from(self.frame_times.len()).unwrap_or(u32::MAX);
            self.frame_time_sum / count
        }
    }

    /// Returns the current FPS based on recent frame history.
    #[must_use]
    pub fn current_fps(&self) -> f64 {
        let avg = self.avg_frame_time();
        if avg.is_zero() {
            0.0
        } else {
            1.0 / avg.as_secs_f64()
        }
    }

    /// Returns the number of dropped frames.
    #[must_use]
    pub const fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    /// Resets all statistics.
    pub fn reset_stats(&mut self) {
        self.frame_times.clear();
        self.frame_time_sum = Duration::ZERO;
        self.dropped_frames = 0;
    }

    /// Returns true if we're currently within a frame (between `begin_frame` and `end_frame`).
    #[must_use]
    pub const fn in_frame(&self) -> bool {
        self.in_frame
    }
}
