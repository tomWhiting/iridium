//! Turning damage into bytes: the whole of the repaint, and no terminal.
//!
//! This is the part of the driver that decides what a frame costs, and it is a
//! pure function of the damage, the capabilities and the recorded terminal
//! state. It writes into any [`io::Write`], which in production is the
//! terminal and in tests is a `Vec<u8>`, so every rule below is checked
//! without a pty.
//!
//! # A frame is atomic
//!
//! The whole frame is wrapped in synchronized output — `BSU` before, `ESU`
//! after — when the terminal supports it, so a fast scroll can never show the
//! top half of one frame above the bottom half of another. The wrapper is
//! written only when there is something inside it: an editor that is idle
//! writes nothing at all, not eight bytes of empty ceremony.
//!
//! # Cursor moves are minimised, but never guessed
//!
//! The writer tracks where the terminal's cursor is and emits the cheapest
//! move that gets to the next run:
//!
//! * nothing, when the cursor is already there — which is the common case,
//!   because runs on one row are emitted left to right;
//! * `CUF` (`ESC [ n C`) for a forward move on the same row, which is
//!   `3 + digits(n)` bytes;
//! * `CUP` (`ESC [ r ; c H`) otherwise, which is `4 + digits(r) + digits(c)`.
//!
//! `CUF` is always the cheaper of the two when it applies: its argument is at
//! most the destination column, so it can never have more digits than the
//! column alone, and `CUP` pays for the row as well.
//!
//! The tracked position is exact. A run's end column is its start plus the
//! columns its segments cover, and the only case where a terminal's cursor
//! would disagree — writing into the last column, where auto-wrap leaves the
//! cursor in a pending-wrap state that is not a column at all — cannot be
//! followed by another run on the same row, because there is no column left
//! for one to start in. Auto-wrap is also turned off for the whole session by
//! [`write_setup`](super::write_setup), for the same reason from the other
//! end: writing the bottom-right cell of the screen with auto-wrap on scrolls
//! the whole display.
//!
//! # Styles are absolute
//!
//! Each style change emits one `SGR` that carries a reset and then everything
//! the style asks for, rather than the difference from the previous style.
//! `termina` packs that into a single CSI, so it costs a handful of bytes more
//! than a minimal diff would, and it buys the property that matters: no
//! sequence depends on the terminal having correctly interpreted the last one,
//! so a style can never leak past the segment it belongs to.
//!
//! A run's segments already carry degraded styles, because the damage diff had
//! to compare them in the depth they would be sent in. They are degraded again
//! here — the operation is idempotent — so that this function is correct on
//! its own arguments rather than only in the company of a matching diff.

use std::io;

use termina::OneBased;
use termina::escape::csi::{
    Csi, Cursor, DecPrivateMode, DecPrivateModeCode, Edit, EraseInDisplay, Mode, Sgr, SgrAttributes,
    SgrModifiers,
};
use termina::style::{ColorSpec, RgbColor};

use super::capabilities::Capabilities;
use super::lifecycle::TerminalState;
use crate::cell::{Attributes, Color, Damage, Style, Surface};

#[cfg(test)]
mod tests;

/// Where the terminal's own cursor should be left when the frame ends.
///
/// This is the caret the user sees, not the write head the repaint moves
/// around: the write head's position is an implementation detail of the frame
/// and is never left anywhere meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorState {
    /// No cursor is shown. This is the right answer while a frame is being
    /// painted, and while the editor has no caret to show.
    #[default]
    Hidden,

    /// The cursor is shown at this cell, counted from the top left.
    At {
        /// The row, counted from the top.
        row: usize,
        /// The column, counted from the left.
        column: usize,
    },
}

/// Everything [`write_frame`] needs to decide what to write.
#[derive(Debug, Clone, Copy)]
pub struct Frame<'a> {
    /// What must be written to bring the terminal up to date.
    pub damage: &'a Damage,
    /// What the terminal can show. Colour is degraded to this depth and the
    /// synchronized-output wrapper is written only when it is supported.
    pub capabilities: Capabilities,
    /// Where to leave the caret once the frame has been written.
    pub cursor: CursorState,
}

/// Writes one frame: the damage, wrapped in synchronized output.
///
/// Nothing is flushed. A caller that writes a frame and then waits for input
/// must flush, and doing it here would cost a syscall per frame for callers
/// that batch.
///
/// `state` is consulted and updated for cursor visibility only; every other
/// mode it records belongs to the session rather than to a frame.
///
/// # Errors
///
/// Returns the first error from the sink. Unlike teardown this stops at the
/// first failure: a half-written frame cannot be repaired by writing the rest
/// of it, and the caller has not committed the surface, so the same damage
/// will be offered again.
pub fn write_frame<W: io::Write>(
    out: &mut W,
    frame: &Frame<'_>,
    state: &TerminalState,
) -> io::Result<()> {
    let paints = !frame.damage.is_empty();
    let hides = matches!(frame.cursor, CursorState::Hidden) && !state.is_cursor_hidden();
    let places = matches!(frame.cursor, CursorState::At { .. });
    if !paints && !hides && !places {
        return Ok(());
    }

    let synchronized = frame.capabilities.synchronized_output;
    if synchronized {
        write!(
            out,
            "{}",
            Csi::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::SynchronizedOutput
            )))
        )?;
    }

    let result = write_body(out, frame, state);

    if synchronized {
        write!(
            out,
            "{}",
            Csi::Mode(Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::SynchronizedOutput
            )))
        )?;
    }
    result
}

/// The contents of the synchronized-output wrapper.
fn write_body<W: io::Write>(
    out: &mut W,
    frame: &Frame<'_>,
    state: &TerminalState,
) -> io::Result<()> {
    let depth = frame.capabilities.color_depth;

    if !frame.damage.is_empty() {
        // The cursor is hidden for the duration of the paint. Inside a
        // synchronized frame this is invisible either way; without one it is
        // the difference between a caret that sits still and a caret that
        // skates across every run.
        hide_cursor(out, state)?;

        let mut position: Option<(usize, usize)> = None;
        // A clear leaves the rendition at the terminal's default, so the first
        // run may skip its `SGR` if that is what it wanted anyway. Without one
        // nothing is known about the rendition and the first run must state
        // it. The cursor is unknown in both cases: `ED` does not move it, but
        // nothing says where the last frame left it either.
        let mut style: Option<Style> = if frame.damage.requires_clear() {
            write_clear(out)?;
            Some(Style::DEFAULT)
        } else {
            None
        };

        for run in frame.damage.runs() {
            move_to(out, &mut position, run.row(), run.column())?;
            for segment in run.segments() {
                let wanted = segment.style().degrade(depth);
                if style != Some(wanted) {
                    write!(out, "{}", Csi::Sgr(Sgr::Attributes(attributes(wanted))))?;
                    style = Some(wanted);
                }
                out.write_all(segment.text().as_bytes())?;
            }
            position = Some((run.row(), run.column() + run.columns()));
        }
    }

    match frame.cursor {
        CursorState::At { row, column } => {
            write!(
                out,
                "{}",
                Csi::Cursor(Cursor::Position {
                    line: one_based(row),
                    col: one_based(column),
                })
            )?;
            show_cursor(out, state)
        },
        CursorState::Hidden => hide_cursor(out, state),
    }
}

/// Erases the whole screen, first putting the rendition back to the default.
///
/// The order is load-bearing: an erase paints with the *current* background
/// colour on any terminal that implements background-colour erase, which is
/// most of them, so erasing under a leftover style would flood the screen with
/// it.
fn write_clear<W: io::Write>(out: &mut W) -> io::Result<()> {
    write!(out, "{}", Csi::Sgr(Sgr::Reset))?;
    write!(
        out,
        "{}",
        Csi::Edit(Edit::EraseInDisplay(EraseInDisplay::EraseDisplay))
    )
}

/// Hides the terminal's cursor, if it is not already hidden.
fn hide_cursor<W: io::Write>(out: &mut W, state: &TerminalState) -> io::Result<()> {
    if state.is_cursor_hidden() {
        return Ok(());
    }
    state.set_cursor_hidden(true);
    write!(
        out,
        "{}",
        Csi::Mode(Mode::ResetDecPrivateMode(DecPrivateMode::Code(
            DecPrivateModeCode::ShowCursor
        )))
    )
}

/// Shows the terminal's cursor, if it is not already shown.
fn show_cursor<W: io::Write>(out: &mut W, state: &TerminalState) -> io::Result<()> {
    if !state.is_cursor_hidden() {
        return Ok(());
    }
    state.set_cursor_hidden(false);
    write!(
        out,
        "{}",
        Csi::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(
            DecPrivateModeCode::ShowCursor
        )))
    )
}

/// Emits the cheapest move from the tracked position to `row`, `column`.
///
/// See the module documentation for why the forward move is always cheaper
/// than the absolute one when it applies.
fn move_to<W: io::Write>(
    out: &mut W,
    position: &mut Option<(usize, usize)>,
    row: usize,
    column: usize,
) -> io::Result<()> {
    let forward = match *position {
        Some((tracked_row, tracked_column)) if tracked_row == row && tracked_column <= column => {
            u32::try_from(column - tracked_column).ok()
        },
        _ => None,
    };
    *position = Some((row, column));
    match forward {
        Some(0) => Ok(()),
        Some(distance) => write!(out, "{}", Csi::Cursor(Cursor::Right(distance))),
        None => write!(
            out,
            "{}",
            Csi::Cursor(Cursor::Position {
                line: one_based(row),
                col: one_based(column),
            })
        ),
    }
}

/// A zero-based index as the one-based value the protocol uses.
///
/// [`MAX_DIMENSION`](crate::cell::MAX_DIMENSION) bounds a buffer to 65,535
/// cells on a side, so an index from a [`Damage`] is at most 65,534 and adding
/// one always fits. Clamping is the only total answer for anything larger, and
/// it cannot make a reachable frame wrong — a panic here could, because it
/// would land with the terminal in raw mode.
fn one_based(index: usize) -> OneBased {
    let one_based = u16::try_from(index.saturating_add(1)).unwrap_or(u16::MAX);
    OneBased::new(one_based).unwrap_or_default()
}

/// One style as a single self-contained `SGR`.
///
/// The reset is always present, which is what makes it self-contained, and it
/// is also why a default colour contributes nothing: `SGR 0` has already put
/// both colours back to the terminal's own.
fn attributes(style: Style) -> SgrAttributes {
    let mut modifiers = SgrModifiers::RESET;
    for (attribute, modifier) in [
        (Attributes::BOLD, SgrModifiers::INTENSITY_BOLD),
        (Attributes::DIM, SgrModifiers::INTENSITY_DIM),
        (Attributes::ITALIC, SgrModifiers::ITALIC),
        (Attributes::UNDERLINE, SgrModifiers::UNDERLINE_SINGLE),
        (Attributes::REVERSE, SgrModifiers::REVERSE),
        (Attributes::STRIKETHROUGH, SgrModifiers::STRIKE_THROUGH),
    ] {
        if style.attributes.contains(attribute) {
            modifiers |= modifier;
        }
    }
    SgrAttributes {
        foreground: color_spec(style.foreground),
        background: color_spec(style.background),
        underline_color: None,
        modifiers,
        ..SgrAttributes::default()
    }
}

/// One colour as `termina`'s colour specification, or `None` for the
/// terminal's own default, which the reset has already restored.
fn color_spec(color: Color) -> Option<ColorSpec> {
    match color {
        Color::Default => None,
        Color::Indexed(index) => Some(ColorSpec::PaletteIndex(index)),
        Color::Rgb(red, green, blue) => Some(RgbColor::new(red, green, blue).into()),
    }
}

/// Resizes the surface to a size the terminal reported, in cells.
///
/// Returns whether the size actually changed. A change invalidates the
/// surface, so the next frame is a full repaint: a terminal reflows, scrolls
/// or truncates its own contents when it changes size, in ways no diff against
/// the previous frame can predict, and diffing against a buffer of the old
/// size would leave stale cells everywhere the two disagree.
///
/// The conversion is saturating rather than fallible because the buffer clamps
/// to [`MAX_DIMENSION`](crate::cell::MAX_DIMENSION) anyway; a terminal cannot
/// report a size larger than that, since every size protocol carries 16-bit
/// values.
pub fn apply_resize(surface: &mut Surface, columns: u32, rows: u32) -> bool {
    let columns = usize::try_from(columns).unwrap_or(usize::MAX);
    let rows = usize::try_from(rows).unwrap_or(usize::MAX);
    if columns == surface.width() && rows == surface.height() {
        return false;
    }
    surface.resize(columns, rows);
    true
}
