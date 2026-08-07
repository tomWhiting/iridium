//! Frame timer tests.
//!
//! Every question about the timer's arithmetic is asked against a synthetic
//! instant, so none of these assertions can be reddened by the machine's
//! load. The one exception is `the_wall_clock_wrappers_read_the_clock`, and
//! its reason for existing is documented at the test.

use std::time::Duration;

use web_time::Instant;

use super::budget::{FrameBudget, TARGET_FRAME_DURATION};
use super::timer::FrameTimer;

#[test]
fn frame_timer_default_target() {
    let timer = FrameTimer::new();
    // Allow small tolerance due to floating-point calculation
    let diff = timer.target_duration().abs_diff(TARGET_FRAME_DURATION);
    assert!(
        diff < Duration::from_micros(1),
        "Target duration should be approximately 8.33ms"
    );
}

#[test]
fn frame_timer_custom_fps() {
    let timer = FrameTimer::with_target_fps(60);
    // 60fps = ~16.67ms per frame
    let target = Duration::from_secs_f64(1.0 / 60.0);
    let diff = timer.target_duration().abs_diff(target);
    assert!(diff < Duration::from_micros(1)); // Allow 1us tolerance
}

/// A zero target has no finite frame duration, and `Duration::from_secs_f64`
/// panics on the infinity that `1.0 / 0.0` produces. The divisor is floored
/// at one so a public constructor cannot be made to panic by its argument.
#[test]
fn a_zero_target_fps_is_floored_rather_than_panicking() {
    let timer = FrameTimer::with_target_fps(0);
    assert_eq!(
        timer.target_duration(),
        Duration::from_secs(1),
        "a zero target must fall back to one frame per second"
    );
}

#[test]
fn frame_timer_tracks_time() {
    let start = Instant::now();
    let mut timer = FrameTimer::new_at(start);

    timer.begin_frame_at(start);

    assert_eq!(
        timer.elapsed_at(start + Duration::from_millis(1)),
        Duration::from_millis(1),
        "elapsed is the difference between the two instants, exactly"
    );
}

/// An instant before the frame started saturates to zero rather than
/// underflowing the subtraction.
#[test]
fn an_instant_before_the_frame_start_yields_zero() {
    let start = Instant::now();
    let mut timer = FrameTimer::new_at(start);
    timer.begin_frame_at(start + Duration::from_millis(10));

    assert_eq!(timer.elapsed_at(start), Duration::ZERO);
    assert_eq!(timer.remaining_at(start), timer.target_duration());
}

#[test]
fn frame_timer_records_frames() {
    let start = Instant::now();
    let mut timer = FrameTimer::new_at(start);
    let frame = Duration::from_micros(100);

    for index in 0..5 {
        let frame_start = start + frame * index * 2;
        timer.begin_frame_at(frame_start);
        timer.end_frame_at(frame_start + frame);
    }

    let stats = timer.stats();
    assert_eq!(
        stats.avg_frame_time, frame,
        "five identical frames average to exactly that frame time"
    );
    assert_eq!(stats.min_frame_time, frame);
    assert_eq!(stats.max_frame_time, frame);
    assert!(stats.fps > 0.0);
}

/// The budget is a function of elapsed time alone, so every band can be
/// named exactly. Only `Comfortable` was covered before the timer took its
/// instant explicitly — the other three were unreachable without sleeping
/// through them.
#[test]
fn every_frame_budget_band_is_reachable() {
    let start = Instant::now();
    let mut timer = FrameTimer::new_at(start);
    timer.begin_frame_at(start);

    // The Overrun edge is asked of the timer rather than restated as a
    // literal. `TARGET_FRAME_DURATION` is `from_micros(8333)`, but the timer
    // divides — `from_secs_f64(1.0 / 120.0)` is 8333.333µs — so a literal
    // `8_333` sits 333ns *below* the real edge and lands in `Critical`.
    // Writing the constant here would test the test's arithmetic, not the
    // band.
    let target = timer.target_duration();
    let band = |offset: Duration| timer.current_budget_at(start + offset);

    assert_eq!(band(Duration::ZERO), FrameBudget::Comfortable);
    assert_eq!(band(Duration::from_micros(6_999)), FrameBudget::Comfortable);
    assert_eq!(band(Duration::from_millis(7)), FrameBudget::Warning);
    assert_eq!(band(Duration::from_micros(7_999)), FrameBudget::Warning);
    assert_eq!(band(Duration::from_millis(8)), FrameBudget::Critical);
    assert_eq!(
        band(target.saturating_sub(Duration::from_nanos(1))),
        FrameBudget::Critical
    );
    assert_eq!(band(target), FrameBudget::Overrun);
    assert_eq!(band(Duration::from_secs(1)), FrameBudget::Overrun);
}

#[test]
fn optional_work_is_allowed_up_to_the_critical_threshold() {
    let start = Instant::now();
    let mut timer = FrameTimer::new_at(start);
    timer.begin_frame_at(start);

    assert!(timer.has_budget_for_optional_work_at(start));
    assert!(
        timer.has_budget_for_optional_work_at(start + Duration::from_millis(7)),
        "Warning still permits optional work"
    );
    assert!(
        !timer.has_budget_for_optional_work_at(start + Duration::from_millis(8)),
        "Critical does not"
    );
}

#[test]
fn frame_timer_dropped_frames() {
    let start = Instant::now();
    let mut timer = FrameTimer::with_target_fps_at(1000, start); // 1ms target

    timer.begin_frame_at(start);
    timer.end_frame_at(start + Duration::from_millis(5));

    assert_eq!(
        timer.dropped_frames(),
        1,
        "a 5ms frame against a 1ms target drops exactly one"
    );
}

/// A frame that lands exactly on the target is not dropped: the comparison
/// is strictly greater-than.
#[test]
fn a_frame_exactly_on_target_is_not_dropped() {
    let start = Instant::now();
    let mut timer = FrameTimer::with_target_fps_at(1000, start);

    timer.begin_frame_at(start);
    timer.end_frame_at(start + Duration::from_millis(1));

    assert_eq!(timer.dropped_frames(), 0);
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
    let start = Instant::now();
    let mut timer = FrameTimer::new_at(start);
    timer.begin_frame_at(start);

    assert_eq!(
        timer.remaining_at(start),
        timer.target_duration(),
        "nothing spent, the whole budget remains"
    );
    assert_eq!(
        timer.remaining_at(start + Duration::from_millis(4)),
        timer
            .target_duration()
            .saturating_sub(Duration::from_millis(4))
    );
    assert_eq!(
        timer.remaining_at(start + Duration::from_secs(1)),
        Duration::ZERO,
        "the budget saturates at zero rather than wrapping"
    );
}

#[test]
fn time_until_next_frame_counts_from_the_last_frame_end() {
    let start = Instant::now();
    let mut timer = FrameTimer::with_target_fps_at(1000, start); // 1ms target
    timer.begin_frame_at(start);
    timer.end_frame_at(start + Duration::from_micros(200));

    assert_eq!(
        timer.time_until_next_frame_at(start + Duration::from_micros(200)),
        Duration::from_millis(1),
        "the wait is measured from the frame's end, not its start"
    );
    // 500µs, not 700µs: the wait counts from the frame's *end* at +200µs, so
    // 500µs of the 1ms target has been spent by +700µs.
    assert_eq!(
        timer.time_until_next_frame_at(start + Duration::from_micros(700)),
        Duration::from_micros(500)
    );
    assert_eq!(
        timer.time_until_next_frame_at(start + Duration::from_secs(1)),
        Duration::ZERO
    );
}

#[test]
fn in_frame_tracks_the_frame_boundary() {
    let start = Instant::now();
    let mut timer = FrameTimer::new_at(start);
    assert!(!timer.in_frame());

    timer.begin_frame_at(start);
    assert!(timer.in_frame());

    timer.end_frame_at(start + Duration::from_millis(1));
    assert!(!timer.in_frame());
}

#[test]
fn reset_stats_clears_history_and_drops() {
    let start = Instant::now();
    let mut timer = FrameTimer::with_target_fps_at(1000, start);
    timer.begin_frame_at(start);
    timer.end_frame_at(start + Duration::from_millis(5));
    assert_eq!(timer.dropped_frames(), 1);

    timer.reset_stats();

    assert_eq!(timer.dropped_frames(), 0);
    assert_eq!(timer.avg_frame_time(), Duration::ZERO);
    assert!((timer.current_fps() - 0.0).abs() < f64::EPSILON);
}

/// Every other test here drives the `_at` methods, which means all of them
/// would still pass if a convenience wrapper stopped reading the clock — if
/// `begin_frame` never called `begin_frame_at`, say. This is the one
/// assertion that would notice.
///
/// It sleeps, and that is deliberate. The assertion is a **lower** bound, so
/// load can only make it more true: a busy machine sleeps longer, never
/// shorter. That is the difference between this sleep and the ones it
/// replaced, which asserted upper bounds and so failed under exactly the
/// conditions that make a test suite hardest to trust.
#[test]
fn the_wall_clock_wrappers_read_the_clock() {
    let mut timer = FrameTimer::new();

    timer.begin_frame();
    assert!(timer.in_frame());
    std::thread::sleep(Duration::from_millis(1));

    assert!(
        timer.elapsed() >= Duration::from_millis(1),
        "elapsed() must advance with the real clock"
    );

    let frame_time = timer.end_frame();
    assert!(frame_time >= Duration::from_millis(1));
    assert!(!timer.in_frame());
    assert!(timer.avg_frame_time() >= Duration::from_millis(1));
}
