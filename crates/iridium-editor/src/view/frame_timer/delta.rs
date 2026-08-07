//! Delta time tracking for animation and smooth updates.

use std::time::Duration;

use web_time::Instant;

/// Delta time provider for animation and smooth updates.
///
/// Provides delta-time-based updates for animations, scrolling, and other
/// time-dependent operations.
///
/// [`update`](Self::update) reads the clock; [`update_at`](Self::update_at)
/// takes the instant explicitly, so that a test can ask about the interval
/// arithmetic without also asking how the machine was loaded at the time.
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
        Self::new_at(Instant::now())
    }

    /// Creates a new delta time tracker whose first interval is measured
    /// from `now`.
    #[must_use]
    pub const fn new_at(now: Instant) -> Self {
        Self {
            last_frame: now,
            delta: Duration::ZERO,
            max_delta: Duration::from_millis(100), // Cap at 100ms (10fps minimum)
        }
    }

    /// Updates the delta time. Call once per frame.
    pub fn update(&mut self) {
        self.update_at(Instant::now());
    }

    /// Updates the delta time for a frame occurring at `now`.
    ///
    /// The interval is capped at the configured maximum, and an instant
    /// earlier than the previous frame's yields a zero delta rather than
    /// underflowing.
    pub fn update_at(&mut self, now: Instant) {
        self.delta = now
            .saturating_duration_since(self.last_frame)
            .min(self.max_delta);
        self.last_frame = now;
    }

    /// Returns the delta time in seconds.
    #[must_use]
    pub const fn delta_secs(&self) -> f32 {
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
    pub const fn set_max_delta(&mut self, max: Duration) {
        self.max_delta = max;
    }
}
