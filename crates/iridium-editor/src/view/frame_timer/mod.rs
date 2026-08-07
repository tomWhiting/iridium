//! Frame timing for 120fps render loop.
//!
//! This module provides frame timing utilities for achieving consistent
//! 120fps rendering. It tracks frame times, calculates delta time, and
//! provides frame budget management for adaptive rendering.
//!
//! # Reading the clock
//!
//! Every operation whose answer depends on the current time comes in two
//! forms: a convenience method that reads the clock itself, and an `_at`
//! method that takes the instant explicitly. The `_at` form is the real
//! implementation and the convenience form is a one-line wrapper over it.
//!
//! That split exists so a test never has to sleep. An assertion written
//! against a synthetic instant asks a question about the arithmetic; the
//! same assertion written against `Instant::now()` and a `thread::sleep`
//! asks whether the operating system scheduled the thread in time, which
//! is a question about the machine's load and not about this code. The
//! second kind reddens under load for reasons unconnected to the change
//! being tested, and — worse — its greens cannot be read at all unless
//! the load at the time is known. The cursor renderer in `render::cursor`
//! already takes its instant explicitly for the same reason.

mod budget;
mod delta;
mod timer;

#[cfg(test)]
mod delta_tests;
#[cfg(test)]
mod timer_tests;

pub use budget::{FrameBudget, FrameStats, TARGET_FPS, TARGET_FRAME_DURATION};
pub use delta::DeltaTime;
pub use timer::FrameTimer;
