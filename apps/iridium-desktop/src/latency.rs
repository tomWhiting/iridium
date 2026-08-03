//! Keydown-to-present latency measurement, armed by the `IRIDIUM_LATENCY`
//! environment variable.
//!
//! Step 3 of `docs/DESKTOP-SHELL-PLAN.md`: the reason this shell exists is
//! latency, so it gets measured rather than asserted. When the variable is
//! set — any value — every keystroke that reaches the kernel is timed from
//! the winit `KeyboardInput` event's receipt to the end of the frame that
//! presents its effect, each sample is written to standard error as it
//! lands, and a summary follows on clean shutdown. When the variable is not
//! set, the normal case, the whole apparatus costs one branch per event: no
//! clock is read, nothing allocates, nothing is stored.
//!
//! # What one sample spans
//!
//! The clock starts when the keyboard event is received, *before* it is
//! translated or dispatched — queueing, translation and the kernel's own
//! work are all inside the measurement, because the user's finger does not
//! care where the time went. The clock stops after the frame's render pass
//! has been submitted and the surface texture presented, which is the last
//! moment this process can observe; the display's own scanout is beyond any
//! user-space clock.
//!
//! # The stale-timestamp policy
//!
//! A timestamp is recorded as pending only when its keystroke was actually
//! dispatched to the kernel. That boundary is exact at this face's seam:
//! every dispatched key requests a redraw (`DesktopApp::press` does so
//! unconditionally), and the only keydowns that never request one are those
//! that never dispatch — releases, synthetic focus bookkeeping, bare
//! modifier presses and other keys the translation drops. Those are
//! discarded on the spot, so an unbound chord cannot leave a stale
//! timestamp behind to inflate a later measurement. When several dispatched
//! keydowns precede one presented frame, the *earliest* pending timestamp
//! is the one measured — that frame answers all of them, and the earliest
//! is the honest worst case. A frame that fails to present leaves the
//! pending timestamp in place: the keystroke's effect is still unpresented,
//! and the measurement honestly spans the failed attempt. A keystroke whose
//! frame never comes at all — the quit chord closes the window before its
//! redraw — is dropped unreported at summary time, never counted.
//!
//! # The percentile method
//!
//! Percentiles are nearest-rank: over `n` sorted samples, the `p`th
//! percentile is the sample at rank `⌈p·n/100⌉` (1-based), computed in
//! integer arithmetic. Nearest-rank always answers with a sample that was
//! actually observed — no interpolation invents a latency nobody saw.

use std::fmt;
use std::time::Instant;

/// Milliseconds per second, for converting sampled durations.
const MILLIS_PER_SECOND: f64 = 1_000.0;

/// The keydown-to-present monitor: one per session, owned beside the editor.
///
/// The winit wiring stays thin on purpose — it reads the clock and hands
/// `Instant`s in, so everything here is pure bookkeeping a unit test can
/// drive without a window or a real clock.
#[derive(Debug)]
pub struct LatencyMonitor {
    /// Whether `IRIDIUM_LATENCY` was set when the session started. When
    /// false, every method returns before touching state.
    armed: bool,
    /// The earliest dispatched keydown not yet answered by a presented
    /// frame. See the module documentation for the exact policy.
    pending: Option<Instant>,
    /// Every recorded sample, in milliseconds, in arrival order.
    samples: Vec<f64>,
}

impl LatencyMonitor {
    /// Creates a monitor, armed or not.
    #[must_use]
    pub const fn new(armed: bool) -> Self {
        Self {
            armed,
            pending: None,
            samples: Vec::new(),
        }
    }

    /// Creates a monitor armed by the `IRIDIUM_LATENCY` environment
    /// variable — set to any value, including empty.
    #[must_use]
    pub fn from_env() -> Self {
        Self::new(std::env::var_os("IRIDIUM_LATENCY").is_some())
    }

    /// Whether the monitor is measuring. The wiring consults this before
    /// reading the clock, which is what keeps the disarmed cost to one
    /// branch.
    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.armed
    }

    /// Records that a keystroke was dispatched to the kernel at `at`.
    ///
    /// An earlier unpresented keydown keeps its claim: the next presented
    /// frame answers every keystroke before it, and the earliest is the
    /// honest worst case for that frame.
    pub const fn key_dispatched(&mut self, at: Instant) {
        if !self.armed {
            return;
        }
        if self.pending.is_none() {
            self.pending = Some(at);
        }
    }

    /// Records that a frame finished presenting at `at`, answering the
    /// pending keydown if there is one.
    ///
    /// Returns the sample in milliseconds when a keydown was waiting, `None`
    /// for a frame nothing keyboard-shaped asked for — a resize, a scroll, a
    /// mouse drag.
    pub fn frame_presented(&mut self, at: Instant) -> Option<f64> {
        if !self.armed {
            return None;
        }
        let keydown = self.pending.take()?;
        let milliseconds = at.saturating_duration_since(keydown).as_secs_f64() * MILLIS_PER_SECOND;
        self.samples.push(milliseconds);
        Some(milliseconds)
    }

    /// The number of samples recorded so far.
    #[must_use]
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// The distribution of everything recorded, or `None` when no keystroke
    /// was ever measured — a summary of nothing would have to invent its
    /// numbers.
    #[must_use]
    pub fn summary(&self) -> Option<Summary> {
        Summary::of(&self.samples)
    }
}

/// The distribution of a session's samples, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    /// How many samples the session recorded.
    pub count: usize,
    /// The smallest sample.
    pub min: f64,
    /// The nearest-rank 50th percentile.
    pub p50: f64,
    /// The nearest-rank 95th percentile.
    pub p95: f64,
    /// The largest sample.
    pub max: f64,
}

impl Summary {
    /// Summarizes a set of samples, or `None` when there are none.
    #[must_use]
    pub fn of(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        Some(Self {
            count: sorted.len(),
            min: *sorted.first()?,
            p50: percentile(&sorted, 50),
            p95: percentile(&sorted, 95),
            max: *sorted.last()?,
        })
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "latency: keydown->present summary — {} samples, min {:.2}ms, p50 {:.2}ms, \
             p95 {:.2}ms, max {:.2}ms",
            self.count, self.min, self.p50, self.p95, self.max,
        )
    }
}

/// The nearest-rank `percent`th percentile of a non-empty sorted slice.
///
/// Rank `⌈percent·n/100⌉`, 1-based, clamped into the slice — integer
/// arithmetic throughout, so no float-to-index cast and no interpolation.
fn percentile(sorted: &[f64], percent: usize) -> f64 {
    let rank = (percent * sorted.len()).div_ceil(100).max(1);
    sorted[rank.min(sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{LatencyMonitor, Summary};

    /// An armed monitor, as every wiring path that records would hold one.
    fn armed() -> LatencyMonitor {
        LatencyMonitor::new(true)
    }

    /// Asserts bit-exact float equality. Every summary field is a *copy* of
    /// an input sample — sorted and selected, never arithmetic — so exact
    /// equality is the honest assertion, stated through bits as the crate's
    /// numeric tests state it.
    fn assert_same(actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn a_summary_of_nothing_is_none() {
        assert_eq!(Summary::of(&[]), None);
        assert_eq!(armed().summary(), None);
    }

    #[test]
    fn a_single_sample_is_its_own_distribution() {
        let summary = Summary::of(&[3.5]).expect("one sample summarizes");
        assert_eq!(summary.count, 1);
        assert_same(summary.min, 3.5);
        assert_same(summary.p50, 3.5);
        assert_same(summary.p95, 3.5);
        assert_same(summary.max, 3.5);
    }

    #[test]
    fn percentiles_are_nearest_rank_over_the_sorted_samples() {
        // 1..=100, shuffled enough to prove the sort: nearest-rank p50 is
        // the 50th smallest, p95 the 95th.
        let mut samples: Vec<f64> = (1..=100).rev().map(f64::from).collect();
        samples.swap(3, 60);
        let summary = Summary::of(&samples).expect("a hundred samples summarize");
        assert_same(summary.min, 1.0);
        assert_same(summary.p50, 50.0);
        assert_same(summary.p95, 95.0);
        assert_same(summary.max, 100.0);
    }

    #[test]
    fn small_sets_round_their_ranks_up() {
        // n = 4: p50 sits at rank ⌈2⌉ = 2, p95 at rank ⌈3.8⌉ = 4.
        let summary = Summary::of(&[10.0, 20.0, 30.0, 40.0]).expect("four samples summarize");
        assert_same(summary.p50, 20.0);
        assert_same(summary.p95, 40.0);
    }

    #[test]
    fn a_frame_answers_the_earliest_unpresented_keydown() {
        let mut monitor = armed();
        let first = Instant::now();
        let second = first + Duration::from_millis(5);
        let presented = first + Duration::from_millis(20);

        monitor.key_dispatched(first);
        monitor.key_dispatched(second);
        let sample = monitor
            .frame_presented(presented)
            .expect("the frame answers the pending keydown");
        assert!(
            (sample - 20.0).abs() < 1e-6,
            "the earliest keydown is the one measured, got {sample}ms"
        );
        assert_eq!(monitor.sample_count(), 1);
    }

    #[test]
    fn a_frame_nobody_asked_for_records_nothing() {
        let mut monitor = armed();
        assert_eq!(monitor.frame_presented(Instant::now()), None);
        assert_eq!(monitor.sample_count(), 0);
    }

    #[test]
    fn presenting_clears_the_pending_keydown() {
        let mut monitor = armed();
        let keydown = Instant::now();
        monitor.key_dispatched(keydown);
        assert!(
            monitor
                .frame_presented(keydown + Duration::from_millis(2))
                .is_some()
        );
        assert_eq!(
            monitor.frame_presented(keydown + Duration::from_millis(4)),
            None,
            "the next frame has no keydown to answer"
        );
        assert_eq!(monitor.sample_count(), 1);
    }

    #[test]
    fn a_disarmed_monitor_stores_nothing() {
        let mut monitor = LatencyMonitor::new(false);
        assert!(!monitor.is_armed());
        monitor.key_dispatched(Instant::now());
        assert_eq!(monitor.frame_presented(Instant::now()), None);
        assert_eq!(monitor.sample_count(), 0);
        assert_eq!(monitor.summary(), None);
    }

    #[test]
    fn a_clock_that_ran_backwards_saturates_to_zero() {
        // Never observable from one monotonic clock, but the arithmetic
        // must not panic if the instants arrive out of order.
        let mut monitor = armed();
        let later = Instant::now() + Duration::from_millis(10);
        monitor.key_dispatched(later);
        let sample = monitor
            .frame_presented(Instant::now())
            .expect("the sample still records");
        assert_same(sample, 0.0);
    }

    #[test]
    fn the_summary_prints_two_decimal_milliseconds() {
        let summary = Summary::of(&[1.234, 2.345, 7.891]).expect("three samples summarize");
        assert_eq!(
            summary.to_string(),
            "latency: keydown->present summary — 3 samples, min 1.23ms, p50 2.35ms, \
             p95 7.89ms, max 7.89ms"
        );
    }
}
