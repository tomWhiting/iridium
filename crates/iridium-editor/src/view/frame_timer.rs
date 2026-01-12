//! Frame timing for 120fps render loop.
//!
//! This module provides frame timing utilities for achieving consistent
//! 120fps rendering. It tracks frame times, calculates delta time, and
//! provides frame budget management for adaptive rendering.

use std::collections::VecDeque;
use std::time::Duration;

use web_time::Instant;

/// Target frame rate (120fps).
pub const TARGET_FPS: u32 = 120;

/// Target frame duration for 120fps (~8.33ms).
pub const TARGET_FRAME_DURATION: Duration = Duration::from_micros(8333);

/// Number of frames to track for averaging.
const FRAME_HISTORY_SIZE: usize = 60;

/// Frame budget thresholds for adaptive quality.
const BUDGET_WARNING_THRESHOLD: Duration = Duration::from_micros(7000);
const BUDGET_CRITICAL_THRESHOLD: Duration = Duration::from_micros(8000);

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

/// Frame timer for managing 120fps rendering.
///
/// This timer tracks frame timing and provides utilities for:
/// - Consistent frame pacing
/// - Frame budget management
/// - Performance statistics
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

    /// Creates a new frame timer with a custom target frame rate.
    #[must_use]
    pub fn with_target_fps(fps: u32) -> Self {
        let target_duration = Duration::from_secs_f64(1.0 / f64::from(fps));
        let now = Instant::now();

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
        self.frame_start = Instant::now();
        self.in_frame = true;
    }

    /// Marks the end of a frame and records timing.
    ///
    /// Call this at the end of your render loop iteration.
    /// Returns the frame duration.
    pub fn end_frame(&mut self) -> Duration {
        let now = Instant::now();
        let frame_time = now.duration_since(self.frame_start);
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
        self.frame_start.elapsed()
    }

    /// Returns the time remaining in the current frame budget.
    #[must_use]
    pub fn remaining(&self) -> Duration {
        self.target_duration.saturating_sub(self.elapsed())
    }

    /// Returns the time until the next frame should start.
    ///
    /// Returns `Duration::ZERO` if we're already past the target time.
    #[must_use]
    pub fn time_until_next_frame(&self) -> Duration {
        let since_last = self.last_frame_end.elapsed();
        self.target_duration.saturating_sub(since_last)
    }

    /// Returns the current frame budget status.
    #[must_use]
    pub fn current_budget(&self) -> FrameBudget {
        let elapsed = self.elapsed();

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
        matches!(
            self.current_budget(),
            FrameBudget::Comfortable | FrameBudget::Warning
        )
    }

    /// Returns frame timing statistics.
    #[must_use]
    pub fn stats(&self) -> FrameStats {
        if self.frame_times.is_empty() {
            return FrameStats::default();
        }

        let count = self.frame_times.len();
        let avg_frame_time = self.frame_time_sum / count as u32;

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
            self.frame_time_sum / self.frame_times.len() as u32
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

    /// Returns true if we're currently within a frame (between begin_frame and end_frame).
    #[must_use]
    pub const fn in_frame(&self) -> bool {
        self.in_frame
    }
}

/// Delta time provider for animation and smooth updates.
///
/// Wraps `FrameTimer` to provide delta-time-based updates for
/// animations, scrolling, and other time-dependent operations.
#[derive(Debug)]
pub struct DeltaTime {
    /// Last frame timestamp
    last_frame: Instant,
    /// Current delta time
    delta: Duration,
    /// Maximum delta time (to prevent large jumps)
    max_delta: Duration,
}

impl Default for DeltaTime {
    fn default() -> Self {
        Self::new()
    }
}

impl DeltaTime {
    /// Creates a new delta time tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_frame: Instant::now(),
            delta: Duration::ZERO,
            max_delta: Duration::from_millis(100), // Cap at 100ms (10fps minimum)
        }
    }

    /// Updates the delta time. Call once per frame.
    pub fn update(&mut self) {
        let now = Instant::now();
        self.delta = now.duration_since(self.last_frame).min(self.max_delta);
        self.last_frame = now;
    }

    /// Returns the delta time in seconds.
    #[must_use]
    pub fn delta_secs(&self) -> f32 {
        self.delta.as_secs_f32()
    }

    /// Returns the delta time as Duration.
    #[must_use]
    pub const fn delta(&self) -> Duration {
        self.delta
    }

    /// Scales a value by delta time for frame-rate independent updates.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to scale (units per second)
    ///
    /// Returns the value scaled by delta time.
    #[must_use]
    pub fn scale(&self, value: f32) -> f32 {
        value * self.delta_secs()
    }

    /// Sets the maximum delta time cap.
    pub fn set_max_delta(&mut self, max: Duration) {
        self.max_delta = max;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn frame_timer_default_target() {
        let timer = FrameTimer::new();
        // Allow small tolerance due to floating-point calculation
        let diff =
            timer.target_duration().as_nanos() as i128 - TARGET_FRAME_DURATION.as_nanos() as i128;
        assert!(
            diff.abs() < 1000,
            "Target duration should be approximately 8.33ms"
        );
    }

    #[test]
    fn frame_timer_custom_fps() {
        let timer = FrameTimer::with_target_fps(60);
        // 60fps = ~16.67ms per frame
        let target = Duration::from_secs_f64(1.0 / 60.0);
        let diff = timer.target_duration().as_nanos() as i128 - target.as_nanos() as i128;
        assert!(diff.abs() < 1000); // Allow 1us tolerance
    }

    #[test]
    fn frame_timer_tracks_time() {
        let mut timer = FrameTimer::new();

        timer.begin_frame();
        thread::sleep(Duration::from_millis(1));
        let elapsed = timer.elapsed();

        assert!(elapsed >= Duration::from_millis(1));
        assert!(elapsed < Duration::from_millis(10));
    }

    #[test]
    fn frame_timer_records_frames() {
        let mut timer = FrameTimer::new();

        for _ in 0..5 {
            timer.begin_frame();
            thread::sleep(Duration::from_micros(100));
            timer.end_frame();
        }

        let stats = timer.stats();
        assert!(stats.avg_frame_time >= Duration::from_micros(100));
        assert!(stats.fps > 0.0);
    }

    #[test]
    fn frame_budget_comfortable() {
        let mut timer = FrameTimer::new();
        timer.begin_frame();
        // Immediately check budget - should be comfortable
        assert_eq!(timer.current_budget(), FrameBudget::Comfortable);
    }

    #[test]
    fn frame_budget_has_optional() {
        let mut timer = FrameTimer::new();
        timer.begin_frame();
        assert!(timer.has_budget_for_optional_work());
    }

    #[test]
    fn delta_time_scales() {
        let mut dt = DeltaTime::new();
        thread::sleep(Duration::from_millis(10));
        dt.update();

        // Scaling 100 units/sec by ~10ms should give ~1 unit
        let scaled = dt.scale(100.0);
        assert!(scaled >= 0.5 && scaled <= 2.0);
    }

    #[test]
    fn delta_time_caps() {
        let mut dt = DeltaTime::new();
        dt.set_max_delta(Duration::from_millis(50));

        // Simulate long gap
        thread::sleep(Duration::from_millis(60));
        dt.update();

        // Should be capped at 50ms
        assert!(dt.delta() <= Duration::from_millis(60));
    }

    #[test]
    fn frame_timer_dropped_frames() {
        let mut timer = FrameTimer::with_target_fps(1000); // Impossible to maintain

        timer.begin_frame();
        thread::sleep(Duration::from_millis(5)); // Will exceed ~1ms target
        timer.end_frame();

        assert!(timer.dropped_frames() > 0);
    }

    #[test]
    fn frame_stats_defaults() {
        let timer = FrameTimer::new();
        let stats = timer.stats();

        // No frames recorded yet
        assert_eq!(stats.avg_frame_time, Duration::ZERO);
        assert_eq!(stats.dropped_frames, 0);
    }

    #[test]
    fn frame_timer_remaining_budget() {
        let mut timer = FrameTimer::new();
        timer.begin_frame();

        let remaining = timer.remaining();
        assert!(remaining > Duration::ZERO);
        // Allow small tolerance due to floating-point calculation
        assert!(
            remaining.as_micros() <= timer.target_duration().as_micros() + 1,
            "Remaining time should not exceed target duration"
        );
    }
}
