//! Frame timing and performance tracking.
//!
//! Provides accurate frame timing for 120fps rendering with performance metrics.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Target frame rates for the render loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetFrameRate {
    /// 60 frames per second (16.67ms per frame).
    Fps60,
    /// 120 frames per second (8.33ms per frame).
    #[default]
    Fps120,
    /// 144 frames per second (6.94ms per frame).
    Fps144,
    /// 240 frames per second (4.17ms per frame).
    Fps240,
    /// Custom frame rate.
    Custom(u32),
}

impl TargetFrameRate {
    /// Returns the target frame duration.
    #[must_use]
    pub fn frame_duration(&self) -> Duration {
        let fps = match self {
            TargetFrameRate::Fps60 => 60,
            TargetFrameRate::Fps120 => 120,
            TargetFrameRate::Fps144 => 144,
            TargetFrameRate::Fps240 => 240,
            TargetFrameRate::Custom(fps) => *fps,
        };

        Duration::from_secs_f64(1.0 / fps as f64)
    }

    /// Returns the frame rate as frames per second.
    #[must_use]
    pub fn fps(&self) -> u32 {
        match self {
            TargetFrameRate::Fps60 => 60,
            TargetFrameRate::Fps120 => 120,
            TargetFrameRate::Fps144 => 144,
            TargetFrameRate::Fps240 => 240,
            TargetFrameRate::Custom(fps) => *fps,
        }
    }
}

/// Frame timing statistics.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    /// Average frame time over the sample window.
    pub avg_frame_time_ms: f64,
    /// Minimum frame time in the sample window.
    pub min_frame_time_ms: f64,
    /// Maximum frame time in the sample window.
    pub max_frame_time_ms: f64,
    /// Current frames per second.
    pub fps: f64,
    /// Number of dropped frames (exceeded target time).
    pub dropped_frames: u64,
    /// Total frames rendered.
    pub total_frames: u64,
}

impl FrameStats {
    /// Returns true if the frame rate is meeting the target.
    #[must_use]
    pub fn is_meeting_target(&self, target: &TargetFrameRate) -> bool {
        self.fps >= (target.fps() as f64 * 0.95) // Allow 5% tolerance
    }

    /// Returns the frame drop percentage.
    #[must_use]
    pub fn drop_percentage(&self) -> f64 {
        if self.total_frames == 0 {
            0.0
        } else {
            (self.dropped_frames as f64 / self.total_frames as f64) * 100.0
        }
    }
}

/// Manages frame timing for smooth rendering.
///
/// The FrameTimer tracks frame times, calculates statistics, and provides
/// timing information for the render loop to achieve the target frame rate.
#[derive(Debug)]
pub struct FrameTimer {
    /// Target frame rate.
    target: TargetFrameRate,
    /// Time of the last frame.
    last_frame: Instant,
    /// Frame times for statistics (circular buffer).
    frame_times: VecDeque<Duration>,
    /// Maximum number of frame times to track.
    sample_size: usize,
    /// Number of dropped frames.
    dropped_frames: u64,
    /// Total frames rendered.
    total_frames: u64,
    /// Time when the timer was created.
    start_time: Instant,
}

impl FrameTimer {
    /// Creates a new frame timer with the target frame rate.
    #[must_use]
    pub fn new(target: TargetFrameRate) -> Self {
        let now = Instant::now();
        Self {
            target,
            last_frame: now,
            frame_times: VecDeque::with_capacity(120),
            sample_size: 120, // 1 second at 120fps
            dropped_frames: 0,
            total_frames: 0,
            start_time: now,
        }
    }

    /// Creates a frame timer with custom sample size.
    #[must_use]
    pub fn with_sample_size(target: TargetFrameRate, sample_size: usize) -> Self {
        let now = Instant::now();
        Self {
            target,
            last_frame: now,
            frame_times: VecDeque::with_capacity(sample_size),
            sample_size,
            dropped_frames: 0,
            total_frames: 0,
            start_time: now,
        }
    }

    /// Returns the target frame rate.
    #[must_use]
    pub fn target(&self) -> &TargetFrameRate {
        &self.target
    }

    /// Sets the target frame rate.
    pub fn set_target(&mut self, target: TargetFrameRate) {
        self.target = target;
    }

    /// Records the start of a new frame.
    ///
    /// Returns the time since the last frame.
    pub fn begin_frame(&mut self) -> Duration {
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame);
        self.last_frame = now;

        // Track frame time
        if self.frame_times.len() >= self.sample_size {
            self.frame_times.pop_front();
        }
        self.frame_times.push_back(delta);

        // Check for dropped frame
        let target_duration = self.target.frame_duration();
        if delta > target_duration + Duration::from_micros(500) {
            // Allow 0.5ms tolerance
            self.dropped_frames += 1;
        }

        self.total_frames += 1;

        delta
    }

    /// Returns the time remaining until the next frame should start.
    ///
    /// If the frame took too long, returns Duration::ZERO.
    #[must_use]
    pub fn time_until_next_frame(&self) -> Duration {
        let target = self.target.frame_duration();
        let elapsed = self.last_frame.elapsed();

        if elapsed >= target {
            Duration::ZERO
        } else {
            target - elapsed
        }
    }

    /// Returns whether it's time to render a new frame.
    #[must_use]
    pub fn should_render(&self) -> bool {
        self.last_frame.elapsed() >= self.target.frame_duration()
    }

    /// Returns the current frame statistics.
    #[must_use]
    pub fn stats(&self) -> FrameStats {
        if self.frame_times.is_empty() {
            return FrameStats::default();
        }

        let sum: Duration = self.frame_times.iter().sum();
        let count = self.frame_times.len() as f64;

        let avg = sum.as_secs_f64() / count;
        let min = self
            .frame_times
            .iter()
            .min()
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let max = self
            .frame_times
            .iter()
            .max()
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        let fps = if avg > 0.0 { 1.0 / avg } else { 0.0 };

        FrameStats {
            avg_frame_time_ms: avg * 1000.0,
            min_frame_time_ms: min * 1000.0,
            max_frame_time_ms: max * 1000.0,
            fps,
            dropped_frames: self.dropped_frames,
            total_frames: self.total_frames,
        }
    }

    /// Returns the total time since the timer was created.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Returns the average frame time in milliseconds.
    #[must_use]
    pub fn avg_frame_time_ms(&self) -> f64 {
        self.stats().avg_frame_time_ms
    }

    /// Returns the current FPS.
    #[must_use]
    pub fn fps(&self) -> f64 {
        self.stats().fps
    }

    /// Resets the timer statistics.
    pub fn reset(&mut self) {
        self.frame_times.clear();
        self.dropped_frames = 0;
        self.total_frames = 0;
        self.last_frame = Instant::now();
        self.start_time = Instant::now();
    }
}

impl Default for FrameTimer {
    fn default() -> Self {
        Self::new(TargetFrameRate::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_target_frame_rate_duration() {
        let fps60 = TargetFrameRate::Fps60;
        assert!((fps60.frame_duration().as_secs_f64() - 0.01666).abs() < 0.001);

        let fps120 = TargetFrameRate::Fps120;
        assert!((fps120.frame_duration().as_secs_f64() - 0.00833).abs() < 0.001);
    }

    #[test]
    fn test_frame_timer_new() {
        let timer = FrameTimer::new(TargetFrameRate::Fps120);
        assert_eq!(timer.target().fps(), 120);
    }

    #[test]
    fn test_frame_timer_begin_frame() {
        let mut timer = FrameTimer::new(TargetFrameRate::Fps60);

        // First frame
        let delta1 = timer.begin_frame();
        assert!(delta1 < Duration::from_millis(100)); // Should be near instant

        // Wait a bit and record another frame
        thread::sleep(Duration::from_millis(10));
        let delta2 = timer.begin_frame();
        assert!(delta2 >= Duration::from_millis(9)); // At least 9ms
    }

    #[test]
    fn test_frame_timer_stats() {
        let mut timer = FrameTimer::with_sample_size(TargetFrameRate::Fps120, 10);

        // Record some frames
        for _ in 0..5 {
            timer.begin_frame();
            thread::sleep(Duration::from_millis(1));
        }

        let stats = timer.stats();
        assert!(stats.total_frames >= 5);
        assert!(stats.fps > 0.0);
    }

    #[test]
    fn test_frame_timer_should_render() {
        let mut timer = FrameTimer::new(TargetFrameRate::Fps60);
        timer.begin_frame();

        // Immediately after begin_frame, should not need to render
        assert!(!timer.should_render());

        // After waiting, should need to render
        thread::sleep(Duration::from_millis(20));
        assert!(timer.should_render());
    }

    #[test]
    fn test_frame_stats_meeting_target() {
        let stats = FrameStats {
            avg_frame_time_ms: 8.0,
            min_frame_time_ms: 7.5,
            max_frame_time_ms: 8.5,
            fps: 125.0,
            dropped_frames: 0,
            total_frames: 1000,
        };

        assert!(stats.is_meeting_target(&TargetFrameRate::Fps120));
        assert!(!stats.is_meeting_target(&TargetFrameRate::Fps144));
    }

    #[test]
    fn test_frame_stats_drop_percentage() {
        let stats = FrameStats {
            avg_frame_time_ms: 10.0,
            min_frame_time_ms: 8.0,
            max_frame_time_ms: 15.0,
            fps: 100.0,
            dropped_frames: 10,
            total_frames: 1000,
        };

        assert!((stats.drop_percentage() - 1.0).abs() < 0.01);
    }
}
