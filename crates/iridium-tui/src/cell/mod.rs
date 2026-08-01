//! Cell buffers and the damage diff that drives repainting.
//!
//! Step 1 of `docs/TERMINAL-FACE-PLAN.md`. Everything here is pure logic:
//! there is no terminal, no escape sequence and no I/O in this module, and
//! that is deliberate. Turning a run into bytes is the driver's job in step 3;
//! keeping the two apart is what lets every rule below be tested without a
//! pty.
//!
//! # A cell is not a character
//!
//! [`unicode_width`] governs, not `char` counting:
//!
//! * A double-width glyph — CJK, most emoji — occupies **two** cells. The
//!   second is a [`CellContent::Continuation`]: it holds no character of its
//!   own, it is never written to the terminal on its own, and overwriting
//!   either half destroys both. Half a glyph is a state no terminal can render
//!   and no later repaint can repair.
//! * A grapheme cluster may be several `char`s in one cell. `e` plus a
//!   combining acute is one cell and two `char`s; a family emoji is two cells
//!   and seven.
//! * Zero-width characters consume no cell at all, and control characters
//!   never reach one — a tab or a newline in the damage output would move the
//!   terminal's cursor and desynchronise the whole screen.
//!
//! A double-width glyph asked to start in the last column of a row cannot fit.
//! It is refused and a space is written instead; see [`WriteOutcome`] for why
//! that is the only safe answer.
//!
//! # Damage is computed, never declared
//!
//! A [`Surface`] holds a front buffer (what the terminal is showing) and a
//! back buffer (what it should show). A frame is drawn into the back buffer in
//! any order, and the writes needed to catch the terminal up are derived by
//! comparing the two. No caller can mark a region dirty: one that forgets
//! leaves a stale cell on screen, and one that over-declares makes the screen
//! flicker. The single exception is [`Surface::invalidate`], which can only
//! declare *everything* — for the moments when the terminal changed underneath
//! us — and so cannot be subtly wrong.
//!
//! The diff coalesces runs rather than emitting one write per changed cell;
//! the rule and the byte accounting behind it are in [`damage`].
//!
//! # Colour degrades once, at the diff
//!
//! Cells store the colour the theme asked for at full 24-bit precision.
//! Degradation to a 256-colour or 16-colour terminal happens when damage is
//! computed, because the comparison has to happen in the colour space the
//! terminal was actually written in: two truecolours that collapse onto one
//! ANSI colour are not a visible change, and repainting them would be waste.
//! See [`color`] for the mapping rules.

mod buffer;
mod color;
mod damage;
mod grapheme;
mod style;
mod surface;

pub use buffer::{Cell, CellBuffer, CellContent, MAX_DIMENSION, WriteOutcome};
pub use color::{Color, ColorDepth};
pub use damage::{Damage, DamageRun, MAX_COALESCE_GAP, StyledSegment};
pub use grapheme::Grapheme;
pub use style::{Attributes, Style};
pub use surface::Surface;
