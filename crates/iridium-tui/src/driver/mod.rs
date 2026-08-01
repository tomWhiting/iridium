//! The driver: the only thing in this crate that owns a real terminal.
//!
//! Step 3 of `docs/TERMINAL-FACE-PLAN.md`. It ties steps 1 and 2 together —
//! the cell buffer's damage becomes escape sequences, and the terminal's bytes
//! become the input adapter's events — and it owns the terminal's lifecycle
//! while it does.
//!
//! # The split that makes this testable
//!
//! Almost none of this module needs a terminal, and that is deliberate. The
//! decision of *what bytes to emit* is a pure function of the damage, the
//! capabilities and the recorded terminal state, so it is written against
//! [`io::Write`](std::io::Write) and tested against a `Vec<u8>`:
//!
//! * [`frame::write_frame`] turns a [`Damage`](crate::cell::Damage) into the
//!   minimum cursor moves, SGR changes and text, wrapped in synchronized
//!   output.
//! * [`lifecycle::write_setup`] and [`lifecycle::write_teardown`] turn the
//!   terminal-mode changes into bytes, and the second is provably the exact
//!   undo of the first.
//! * [`Negotiation`] decides truecolor versus 256 versus 16, and whether the
//!   kitty keyboard protocol is available, from replies that a test can
//!   construct by hand.
//!
//! What is left over — [`Driver`] itself — opens the terminal, flips termios,
//! blocks on `poll`, and hands those bytes to the functions above. It is the
//! only part that cannot run in CI, and it is kept as small as that sentence
//! suggests.
//!
//! # Nothing may be left behind
//!
//! A panic with the terminal in raw mode leaves the user's shell unusable, so
//! restoring is not best-effort. `termina` restores the platform's termios
//! state from both a panic hook and a `Drop` impl, but *application*-level
//! state — the alternate screen, bracketed paste, focus reporting, the
//! keyboard-enhancement stack, cursor visibility — is ours, and [`Driver`]
//! undoes all of it from the same two places. [`TerminalState`] is what makes
//! that possible: it records what is currently on, it is shared with the panic
//! hook, and [`lifecycle::write_teardown`] undoes exactly what it says is on
//! and nothing else.
//!
//! # The event loop
//!
//! `termina`'s `poll(filter, None)` blocks, so the idle loop costs nothing —
//! there is no polling interval and no sleep:
//!
//! ```no_run
//! # use iridium_tui::driver::{Driver, DriverEvent};
//! # fn main() -> std::io::Result<()> {
//! let mut driver = Driver::open()?;
//! loop {
//!     driver.present()?;
//!     match driver.next_event()? {
//!         DriverEvent::Input(_input) => { /* draw into `driver.surface_mut()` */ },
//!         DriverEvent::Resize { .. } => { /* the surface is already resized */ },
//!         DriverEvent::Unrecognised => {},
//!     }
//! #   break;
//! }
//! # Ok(())
//! # }
//! ```

pub mod capabilities;
pub mod frame;
pub mod lifecycle;
mod terminal;

pub use capabilities::{
    Capabilities, KEYBOARD_ENHANCEMENT_FLAGS, Negotiation, PROBE_COLOR, color_depth_from_env,
};
pub use frame::{CursorState, Frame, write_frame};
pub use lifecycle::{TerminalState, write_setup, write_teardown};
pub use terminal::{Driver, DriverEvent, NEGOTIATION_TIMEOUT};
