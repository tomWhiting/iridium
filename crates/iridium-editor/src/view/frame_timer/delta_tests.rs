//! Delta time tests.
//!
//! Driven by synthetic instants for the same reason as the timer's tests:
//! an assertion against a synthetic clock asks about this code, and one
//! against a sleep asks about the scheduler.

use std::time::Duration;

use web_time::Instant;

use super::delta::DeltaTime;

#[test]
fn a_fresh_tracker_has_no_delta() {
    let dt = DeltaTime::new_at(Instant::now());
    assert_eq!(dt.delta(), Duration::ZERO);
    assert!((dt.scale(100.0) - 0.0).abs() < f32::EPSILON);
}

#[test]
fn delta_time_scales() {
    let start = Instant::now();
    let mut dt = DeltaTime::new_at(start);
    dt.update_at(start + Duration::from_millis(10));

    // Scaling 100 units/sec by exactly 10ms gives exactly 1 unit, up to the
    // representation of 0.01 in f32.
    let scaled = dt.scale(100.0);
    assert!(
        (scaled - 1.0).abs() < 1e-5,
        "100 units/sec over 10ms is 1 unit, got {scaled}"
    );
}

/// The cap is the whole point of `max_delta`, so the assertion names the
/// capped value rather than bounding it.
///
/// The previous form of this test asserted `delta() <= 60ms` after sleeping
/// 60ms — a bound *above* the 50ms the cap should produce. Whether that
/// caught a missing cap depended on `thread::sleep` overshooting its
/// argument, which is to say it depended on the machine's load. Against a
/// synthetic clock a missing cap yields exactly 60ms and the old assertion
/// passes outright.
#[test]
fn delta_time_caps() {
    let start = Instant::now();
    let mut dt = DeltaTime::new_at(start);
    dt.set_max_delta(Duration::from_millis(50));

    dt.update_at(start + Duration::from_millis(60));

    assert_eq!(
        dt.delta(),
        Duration::from_millis(50),
        "a 60ms gap must be capped to the configured 50ms maximum"
    );
}

/// The cap must not touch an interval below it, or every animation would
/// run at the cap.
#[test]
fn an_interval_under_the_cap_passes_through_unchanged() {
    let start = Instant::now();
    let mut dt = DeltaTime::new_at(start);
    dt.set_max_delta(Duration::from_millis(50));

    dt.update_at(start + Duration::from_millis(30));

    assert_eq!(dt.delta(), Duration::from_millis(30));
}

/// Each update measures from the previous update, not from construction.
#[test]
fn successive_updates_measure_the_interval_between_them() {
    let start = Instant::now();
    let mut dt = DeltaTime::new_at(start);

    dt.update_at(start + Duration::from_millis(10));
    assert_eq!(dt.delta(), Duration::from_millis(10));

    dt.update_at(start + Duration::from_millis(25));
    assert_eq!(
        dt.delta(),
        Duration::from_millis(15),
        "the second interval is 25ms - 10ms, not 25ms"
    );
}

/// An instant earlier than the last frame's saturates to zero rather than
/// underflowing.
#[test]
fn a_backwards_instant_yields_a_zero_delta() {
    let start = Instant::now();
    let mut dt = DeltaTime::new_at(start + Duration::from_millis(10));

    dt.update_at(start);

    assert_eq!(dt.delta(), Duration::ZERO);
}

/// As with the timer, every other test here drives `update_at`, so all of
/// them would pass even if `update` stopped reading the clock. The bound is
/// a lower one, so load can only make it more true.
#[test]
fn the_wall_clock_wrapper_reads_the_clock() {
    let mut dt = DeltaTime::new();

    std::thread::sleep(Duration::from_millis(1));
    dt.update();

    assert!(
        dt.delta() >= Duration::from_millis(1),
        "update() must advance with the real clock"
    );
}
