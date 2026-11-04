//! Entering and leaving the terminal, and proving the two are the same set.
//!
//! `termina` restores the platform's termios state for us, from a panic hook
//! and from a `Drop` impl. Everything *above* termios is ours: the alternate
//! screen, auto-wrap, bracketed paste, focus reporting, the
//! keyboard-enhancement stack, mouse reporting and the cursor. None of it is
//! restored by
//! anyone else, and leaving any of it on hands the user a shell that echoes
//! nothing, pastes as keystrokes, or has no cursor.
//!
//! # Undoing exactly what was done
//!
//! The naive way to write teardown is to emit every reset unconditionally.
//! That is wrong in two directions: it resets modes the process never set —
//! turning off a user's bracketed paste, for instance — and it silently
//! diverges from setup the moment setup grows a step. So setup does not write
//! a fixed list. It records what it turned on in [`TerminalState`], and
//! teardown undoes what that record says, in reverse order, and nothing else.
//!
//! [`TerminalState`] is shared with the panic hook, which is why every field
//! is an atomic and why a flag is set *before* its escape sequence is written.
//! A write that fails half way may still have reached the terminal, and a mode
//! that might be on has to be turned off; turning off a mode that is already
//! off is a no-op, while leaving one on is not.
//!
//! Teardown is idempotent: it clears each flag as it undoes it, so the `Drop`
//! that follows an explicit close writes nothing at all.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use termina::escape::csi::{Csi, DecPrivateMode, DecPrivateModeCode, Keyboard, Mode, Sgr};

use super::capabilities::{Capabilities, KEYBOARD_ENHANCEMENT_FLAGS};

#[cfg(test)]
mod tests;

/// The application-level terminal state that must be undone before exit.
///
/// Every field records a mode this process turned on. The type is all atomics
/// because the panic hook holds a reference to the same value and may run on
/// any thread; the ordering is [`Ordering::SeqCst`] throughout, because these
/// are read a handful of times per frame and being obviously correct is worth
/// more than the fence.
#[derive(Debug, Default)]
pub struct TerminalState {
    /// The alternate screen (DEC mode 1049) is in use.
    alternate_screen: AtomicBool,
    /// Auto-wrap (DEC mode 7) has been turned off.
    auto_wrap_disabled: AtomicBool,
    /// Bracketed paste (DEC mode 2004) is on.
    bracketed_paste: AtomicBool,
    /// Focus reporting (DEC mode 1004) is on.
    focus_tracking: AtomicBool,
    /// Keyboard-enhancement flags have been pushed onto the terminal's stack.
    keyboard_flags_pushed: AtomicBool,
    /// The cursor (DEC mode 25) is hidden.
    cursor_hidden: AtomicBool,
    /// Mouse reporting (DEC modes 1000 and 1006) is on.
    mouse_capture: AtomicBool,
}

impl TerminalState {
    /// A state in which nothing has been turned on.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            alternate_screen: AtomicBool::new(false),
            auto_wrap_disabled: AtomicBool::new(false),
            bracketed_paste: AtomicBool::new(false),
            focus_tracking: AtomicBool::new(false),
            keyboard_flags_pushed: AtomicBool::new(false),
            cursor_hidden: AtomicBool::new(false),
            mouse_capture: AtomicBool::new(false),
        }
    }

    /// Whether the terminal is reporting the mouse to this process.
    #[must_use]
    pub fn is_mouse_captured(&self) -> bool {
        self.mouse_capture.load(Ordering::SeqCst)
    }

    /// Whether the terminal's cursor is currently hidden.
    #[must_use]
    pub fn is_cursor_hidden(&self) -> bool {
        self.cursor_hidden.load(Ordering::SeqCst)
    }

    /// Records that the cursor has been hidden or shown.
    ///
    /// Called by the frame writer, which is the only thing that changes cursor
    /// visibility after setup. Recording it here is what lets the panic hook
    /// put the cursor back when a panic lands mid-frame.
    pub fn set_cursor_hidden(&self, hidden: bool) {
        self.cursor_hidden.store(hidden, Ordering::SeqCst);
    }

    /// Whether the keyboard-enhancement flags were pushed.
    #[must_use]
    pub fn are_keyboard_flags_pushed(&self) -> bool {
        self.keyboard_flags_pushed.load(Ordering::SeqCst)
    }

    /// Whether anything at all is on.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        !self.alternate_screen.load(Ordering::SeqCst)
            && !self.auto_wrap_disabled.load(Ordering::SeqCst)
            && !self.bracketed_paste.load(Ordering::SeqCst)
            && !self.focus_tracking.load(Ordering::SeqCst)
            && !self.keyboard_flags_pushed.load(Ordering::SeqCst)
            && !self.cursor_hidden.load(Ordering::SeqCst)
            && !self.mouse_capture.load(Ordering::SeqCst)
    }
}

/// Turns mouse reporting on or off, recording it in `state`.
///
/// Mode 1000 reports presses, releases and the wheel; mode 1006 makes the
/// report SGR-encoded, which has no column limit. With capture on, the
/// terminal's own click-to-select needs a modifier (usually `Shift`), which is
/// why it is a toggle and not a fixture. Writing the mode it is already in
/// writes nothing.
///
/// Off is recorded *after* its bytes and on *before*, for the reason the module
/// documentation gives: a mode that might be on has to be turned off by
/// teardown.
///
/// # Errors
///
/// Returns the first error from the sink.
pub fn write_mouse_capture<W: io::Write>(
    out: &mut W,
    state: &TerminalState,
    capture: bool,
) -> io::Result<()> {
    if state.is_mouse_captured() == capture {
        return Ok(());
    }
    if capture {
        state.mouse_capture.store(true, Ordering::SeqCst);
        write!(
            out,
            "{}{}",
            Csi::Mode(Mode::SetDecPrivateMode(dec(
                DecPrivateModeCode::MouseTracking
            ))),
            Csi::Mode(Mode::SetDecPrivateMode(dec(DecPrivateModeCode::SGRMouse)))
        )
    } else {
        write_mouse_off(out)?;
        state.mouse_capture.store(false, Ordering::SeqCst);
        Ok(())
    }
}

/// The bytes that end mouse reporting, in the reverse order of turning it on.
fn write_mouse_off<W: io::Write>(out: &mut W) -> io::Result<()> {
    write!(
        out,
        "{}{}",
        Csi::Mode(Mode::ResetDecPrivateMode(dec(DecPrivateModeCode::SGRMouse))),
        Csi::Mode(Mode::ResetDecPrivateMode(dec(
            DecPrivateModeCode::MouseTracking
        )))
    )
}

/// A DEC private mode as a value, ready to be set or reset.
const fn dec(code: DecPrivateModeCode) -> DecPrivateMode {
    DecPrivateMode::Code(code)
}

/// Enters the terminal, recording each mode turned on in `state`.
///
/// The order is: alternate screen first, so nothing that follows can be seen
/// on the user's shell; then auto-wrap off, so that writing the bottom-right
/// cell cannot scroll the screen; then the input modes; then the cursor is
/// hidden, because the first frame has not been painted yet and a cursor
/// parked at the top-left corner is the flicker every editor start-up is
/// judged on.
///
/// The keyboard-enhancement stack is pushed only when the terminal was shown
/// to implement it. Pushing at a terminal that does not is harmless, but the
/// matching pop would be equally ignored, so recording a push that never
/// happened would make [`write_teardown`] describe a terminal that does not
/// exist.
///
/// Nothing here flushes: a caller that enters the terminal and then writes a
/// frame should pay for one flush, not two.
///
/// # Errors
///
/// Returns the first error from the sink. Any mode whose flag was already set
/// will still be undone by [`write_teardown`], including the one whose write
/// failed.
pub fn write_setup<W: io::Write>(
    out: &mut W,
    capabilities: Capabilities,
    state: &TerminalState,
) -> io::Result<()> {
    state.alternate_screen.store(true, Ordering::SeqCst);
    write!(
        out,
        "{}",
        Csi::Mode(Mode::SetDecPrivateMode(dec(
            DecPrivateModeCode::ClearAndEnableAlternateScreen
        )))
    )?;

    state.auto_wrap_disabled.store(true, Ordering::SeqCst);
    write!(
        out,
        "{}",
        Csi::Mode(Mode::ResetDecPrivateMode(dec(DecPrivateModeCode::AutoWrap)))
    )?;

    state.bracketed_paste.store(true, Ordering::SeqCst);
    write!(
        out,
        "{}",
        Csi::Mode(Mode::SetDecPrivateMode(dec(
            DecPrivateModeCode::BracketedPaste
        )))
    )?;

    state.focus_tracking.store(true, Ordering::SeqCst);
    write!(
        out,
        "{}",
        Csi::Mode(Mode::SetDecPrivateMode(dec(
            DecPrivateModeCode::FocusTracking
        )))
    )?;

    if capabilities.keyboard_enhancement {
        state.keyboard_flags_pushed.store(true, Ordering::SeqCst);
        write!(
            out,
            "{}",
            Csi::Keyboard(Keyboard::PushFlags(KEYBOARD_ENHANCEMENT_FLAGS))
        )?;
    }

    state.cursor_hidden.store(true, Ordering::SeqCst);
    write!(
        out,
        "{}",
        Csi::Mode(Mode::ResetDecPrivateMode(dec(
            DecPrivateModeCode::ShowCursor
        )))
    )
}

/// Leaves the terminal, undoing exactly what `state` says is on.
///
/// Every step is attempted even when an earlier one failed, and the first
/// error is returned afterwards. Giving up half way through would leave the
/// terminal in a state no later code will ever fix — this runs from `Drop` and
/// from the panic hook, and neither gets a second chance.
///
/// Each flag is cleared as its mode is undone, so calling this twice writes
/// nothing the second time.
///
/// # Errors
///
/// Returns the first error from the sink, after attempting every step.
pub fn write_teardown<W: io::Write>(out: &mut W, state: &TerminalState) -> io::Result<()> {
    if state.is_clean() {
        return Ok(());
    }
    let mut failure: Option<io::Error> = None;

    // First, because it was turned on last: a shell left with mouse reporting
    // on prints escape sequences on every click.
    if state.mouse_capture.swap(false, Ordering::SeqCst) {
        record(&mut failure, write_mouse_off(out));
    }

    if state.cursor_hidden.swap(false, Ordering::SeqCst) {
        record(
            &mut failure,
            write!(
                out,
                "{}",
                Csi::Mode(Mode::SetDecPrivateMode(dec(DecPrivateModeCode::ShowCursor)))
            ),
        );
    }

    // The rendition is not recorded in `state` because it is not a mode: a
    // frame changes it constantly and no record could keep up. Resetting it is
    // unconditional and cannot be wrong — `SGR 0` is what a shell prompt
    // assumes it starts from.
    record(&mut failure, write!(out, "{}", Csi::Sgr(Sgr::Reset)));

    if state.keyboard_flags_pushed.swap(false, Ordering::SeqCst) {
        record(
            &mut failure,
            write!(out, "{}", Csi::Keyboard(Keyboard::PopFlags(1))),
        );
    }

    if state.focus_tracking.swap(false, Ordering::SeqCst) {
        record(
            &mut failure,
            write!(
                out,
                "{}",
                Csi::Mode(Mode::ResetDecPrivateMode(dec(
                    DecPrivateModeCode::FocusTracking
                )))
            ),
        );
    }

    if state.bracketed_paste.swap(false, Ordering::SeqCst) {
        record(
            &mut failure,
            write!(
                out,
                "{}",
                Csi::Mode(Mode::ResetDecPrivateMode(dec(
                    DecPrivateModeCode::BracketedPaste
                )))
            ),
        );
    }

    if state.auto_wrap_disabled.swap(false, Ordering::SeqCst) {
        record(
            &mut failure,
            write!(
                out,
                "{}",
                Csi::Mode(Mode::SetDecPrivateMode(dec(DecPrivateModeCode::AutoWrap)))
            ),
        );
    }

    if state.alternate_screen.swap(false, Ordering::SeqCst) {
        record(
            &mut failure,
            write!(
                out,
                "{}",
                Csi::Mode(Mode::ResetDecPrivateMode(dec(
                    DecPrivateModeCode::ClearAndEnableAlternateScreen
                )))
            ),
        );
    }

    record(&mut failure, out.flush());

    failure.map_or(Ok(()), Err)
}

/// Keeps the first error of a sequence of best-effort writes.
fn record(slot: &mut Option<io::Error>, outcome: io::Result<()>) {
    if let Err(error) = outcome {
        slot.get_or_insert(error);
    }
}
