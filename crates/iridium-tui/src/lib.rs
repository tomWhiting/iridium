//! The terminal face for the Iridium editor kernel.
//!
//! This is the only crate in the workspace that knows what a terminal is. It
//! turns the kernel's document, layout and highlight state into cells, and
//! terminal input back into the kernel's platform-neutral key presses.
//!
//! # What this crate must not do
//!
//! It must not reimplement a verb the kernel already has. Three separate bugs
//! this year were a face doing exactly that — multi-cursor, `deleteToLineStart`
//! and undo each had a face-local reimplementation that drifted from the
//! kernel's. Every editing action goes through the command registry.
//!
//! It must not grow a second layout model either. The kernel already owns
//! fold-aware layout; the terminal drives that same `Viewport` in **cell
//! units**, with a line height of one and a width in columns. See
//! `docs/TERMINAL-FACE-PLAN.md`.
//!
//! # A cell is not a character
//!
//! Cell width is governed by `unicode-width`. A double-width glyph occupies two
//! cells, the second of which is a continuation cell holding no character of
//! its own, and a grapheme cluster may span several `char`s within one cell.
//! Column arithmetic that advances by one per `char` is a bug.

/// Cell buffers and the damage diff that drives repainting.
pub mod cell;
/// Terminal input, converted to the kernel's platform-neutral key presses.
pub mod input;
