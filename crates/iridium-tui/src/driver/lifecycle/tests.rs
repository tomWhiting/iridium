//! Proving that leaving the terminal undoes exactly what entering it did.
//!
//! The interesting property is not that any particular sequence is written —
//! it is that the two sets match. These tests therefore scan the *bytes* for
//! mode changes rather than asking the production code what it did, so a step
//! added to entry and forgotten on exit fails here even though both halves of
//! the code would still agree with themselves.

use super::*;
use crate::cell::ColorDepth;

/// A terminal-mode change, recovered from the bytes that were written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// `CSI ? n h`.
    Set(u16),
    /// `CSI ? n l`.
    Reset(u16),
    /// `CSI > flags u`.
    PushKeyboard(u8),
    /// `CSI < n u`.
    PopKeyboard(u8),
}

impl Op {
    /// The change that undoes this one.
    fn inverse(self) -> Self {
        match self {
            Self::Set(code) => Self::Reset(code),
            Self::Reset(code) => Self::Set(code),
            Self::PushKeyboard(_) => Self::PopKeyboard(1),
            Self::PopKeyboard(count) => Self::PushKeyboard(count),
        }
    }

    /// Whether `self` undoes `other`.
    ///
    /// Not simply `self == other.inverse()`, because the keyboard stack is the
    /// one pair whose two halves carry *different kinds* of number: a push
    /// carries a flag bitmask (`CSI > 11 u`) and a pop carries a count of stack
    /// entries to remove (`CSI < 1 u`). Comparing the two payloads asks whether
    /// a bitmask equals a count, which is meaningless — and answering it "no"
    /// would report a correct teardown as an unbalanced one.
    ///
    /// A pop of one therefore undoes a push whatever flags that push carried.
    fn undoes(self, other: Self) -> bool {
        match (self, other) {
            (Self::PopKeyboard(count), Self::PushKeyboard(_)) => count == 1,
            _ => self == other.inverse(),
        }
    }
}

/// Every mode change in a byte stream, in the order it was written.
///
/// Sequences that are not mode changes — the `SGR` reset, in practice — are
/// skipped rather than rejected, because they have no inverse to pair with.
fn scan(bytes: &[u8]) -> Vec<Op> {
    let text = std::str::from_utf8(bytes).expect("the escape sequences are ASCII");
    let mut ops = Vec::new();
    for chunk in text.split('\u{1b}').skip(1) {
        let Some(body) = chunk.strip_prefix('[') else {
            continue;
        };
        let Some(marker) = body.chars().next() else {
            continue;
        };
        let payload = &body[marker.len_utf8()..];
        let (digits, terminator) = payload.split_at(payload.len().saturating_sub(1));
        match (marker, terminator) {
            ('?', "h") => ops.push(Op::Set(digits.parse().expect("a mode number"))),
            ('?', "l") => ops.push(Op::Reset(digits.parse().expect("a mode number"))),
            ('>', "u") => ops.push(Op::PushKeyboard(digits.parse().expect("a flag set"))),
            ('<', "u") => ops.push(Op::PopKeyboard(digits.parse().expect("a count"))),
            _ => {},
        }
    }
    ops
}

/// The capabilities of a terminal that speaks the kitty keyboard protocol.
const MODERN: Capabilities = Capabilities {
    color_depth: ColorDepth::TrueColor,
    keyboard_enhancement: true,
    synchronized_output: true,
};

/// The capabilities of a terminal that does not.
const LEGACY: Capabilities = Capabilities {
    color_depth: ColorDepth::Ansi16,
    keyboard_enhancement: false,
    synchronized_output: false,
};

/// Enters and leaves a terminal, returning the two byte streams.
fn round_trip(capabilities: Capabilities) -> (Vec<u8>, Vec<u8>) {
    let state = TerminalState::new();
    let mut entry = Vec::new();
    write_setup(&mut entry, capabilities, &state).expect("a `Vec` cannot fail");
    let mut exit = Vec::new();
    write_teardown(&mut exit, &state).expect("a `Vec` cannot fail");
    (entry, exit)
}

#[test]
fn leaving_is_the_exact_reverse_of_entering() {
    for capabilities in [MODERN, LEGACY] {
        let (entry, exit) = round_trip(capabilities);
        let entered = scan(&entry);
        let left = scan(&exit);
        let expected: Vec<Op> = entered.iter().rev().map(|op| op.inverse()).collect();
        assert_eq!(
            left, expected,
            "entering wrote {entered:?}, so leaving must write {expected:?}"
        );
    }
}

#[test]
fn every_mode_that_is_set_is_reset() {
    let (entry, exit) = round_trip(MODERN);
    let entered = scan(&entry);
    let left = scan(&exit);
    assert!(!entered.is_empty(), "entering must change something");
    for op in &entered {
        assert!(
            left.iter().any(|undo| undo.undoes(*op)),
            "{op:?} was never undone; leaving wrote {left:?}"
        );
    }
    for op in &left {
        assert!(
            entered.iter().any(|done| op.undoes(*done)),
            "{op:?} undoes something that was never done; entering wrote {entered:?}"
        );
    }
}

#[test]
fn entering_names_the_modes_it_is_supposed_to() {
    let (entry, _) = round_trip(MODERN);
    assert_eq!(
        scan(&entry),
        vec![
            Op::Set(1049),
            Op::Reset(7),
            Op::Set(2004),
            Op::Set(1004),
            Op::PushKeyboard(KEYBOARD_ENHANCEMENT_FLAGS.bits()),
            Op::Reset(25),
        ],
        "alternate screen, auto-wrap off, bracketed paste, focus, keyboard, cursor"
    );
}

#[test]
fn the_alternate_screen_is_entered_first_and_left_last() {
    let (entry, exit) = round_trip(MODERN);
    assert!(
        entry.starts_with(b"\x1b[?1049h"),
        "nothing may be visible on the user's own screen"
    );
    assert!(
        exit.ends_with(b"\x1b[?1049l"),
        "the alternate screen must outlive every other restore"
    );
}

#[test]
fn a_legacy_terminal_is_never_popped() {
    let (entry, exit) = round_trip(LEGACY);
    for op in scan(&entry).iter().chain(scan(&exit).iter()) {
        assert!(
            !matches!(op, Op::PushKeyboard(_) | Op::PopKeyboard(_)),
            "a terminal without the protocol has no stack to push or pop: {op:?}"
        );
    }
    assert!(
        scan(&entry).contains(&Op::Set(1049)),
        "it is still a screen"
    );
}

#[test]
fn the_rendition_is_reset_before_the_screen_is_left() {
    let (_, exit) = round_trip(MODERN);
    let text = std::str::from_utf8(&exit).expect("ASCII");
    let reset = text.find("\x1b[m").expect("the rendition must be reset");
    let leave = text.find("\x1b[?1049l").expect("the screen must be left");
    assert!(reset < leave, "a shell prompt starts from `SGR 0`");
}

#[test]
fn leaving_twice_writes_nothing_the_second_time() {
    let state = TerminalState::new();
    let mut sink = Vec::new();
    write_setup(&mut sink, MODERN, &state).expect("a `Vec` cannot fail");

    let mut first = Vec::new();
    write_teardown(&mut first, &state).expect("a `Vec` cannot fail");
    assert!(!first.is_empty());

    let mut second = Vec::new();
    write_teardown(&mut second, &state).expect("a `Vec` cannot fail");
    assert!(
        second.is_empty(),
        "`Drop` after an explicit close must be silent, not a second reset"
    );
    assert!(state.is_clean());
}

#[test]
fn leaving_a_terminal_that_was_never_entered_writes_nothing() {
    let state = TerminalState::new();
    assert!(state.is_clean());
    let mut sink = Vec::new();
    write_teardown(&mut sink, &state).expect("a `Vec` cannot fail");
    assert!(
        sink.is_empty(),
        "a driver that failed to open must not reset a shell it never touched"
    );
}

#[test]
fn a_cursor_hidden_by_a_frame_is_shown_again() {
    // The frame writer hides and shows the cursor as the caret requires, so
    // the state at exit is not the state at entry. Only what is on may be
    // undone, and a hidden cursor is always on.
    let state = TerminalState::new();
    state.set_cursor_hidden(true);
    let mut sink = Vec::new();
    write_teardown(&mut sink, &state).expect("a `Vec` cannot fail");
    assert_eq!(scan(&sink), vec![Op::Set(25)]);
    assert!(!state.is_cursor_hidden());
}

#[test]
fn a_cursor_left_visible_is_not_hidden_on_the_way_out() {
    let state = TerminalState::new();
    let mut sink = Vec::new();
    write_setup(&mut sink, LEGACY, &state).expect("a `Vec` cannot fail");
    state.set_cursor_hidden(false);
    let mut exit = Vec::new();
    write_teardown(&mut exit, &state).expect("a `Vec` cannot fail");
    assert!(
        !scan(&exit).contains(&Op::Set(25)),
        "showing a cursor that is already shown is a mode this process did not set"
    );
}

/// A sink that fails one write and records everything else.
#[derive(Debug)]
struct FlakySink {
    /// The index of the write that fails.
    fails_at: usize,
    /// How many writes have been attempted.
    attempts: usize,
    /// Everything that was not rejected.
    written: Vec<u8>,
}

impl FlakySink {
    /// A sink whose `fails_at`th write fails.
    const fn failing_at(fails_at: usize) -> Self {
        Self {
            fails_at,
            attempts: 0,
            written: Vec::new(),
        }
    }
}

impl io::Write for FlakySink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let attempt = self.attempts;
        self.attempts += 1;
        if attempt == self.fails_at {
            return Err(io::Error::other("the terminal went away"));
        }
        self.written.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn leaving_attempts_every_step_after_one_fails() {
    let state = TerminalState::new();
    let mut entry = Vec::new();
    write_setup(&mut entry, MODERN, &state).expect("a `Vec` cannot fail");

    let mut sink = FlakySink::failing_at(0);
    let outcome = write_teardown(&mut sink, &state);
    assert!(outcome.is_err(), "the failure must be reported");
    assert!(
        scan(&sink.written).contains(&Op::Reset(1049)),
        "giving up half way leaves a terminal nobody will ever fix"
    );
    assert!(state.is_clean(), "every step still ran");
}

#[test]
fn a_mode_whose_write_failed_is_still_undone() {
    // A failed write may still have reached the terminal, so the record is
    // made before the write. Anything else risks leaving a mode on forever.
    let state = TerminalState::new();
    let mut sink = FlakySink::failing_at(0);
    let outcome = write_setup(&mut sink, MODERN, &state);
    assert!(outcome.is_err());

    let mut exit = Vec::new();
    write_teardown(&mut exit, &state).expect("a `Vec` cannot fail");
    assert!(
        scan(&exit).contains(&Op::Reset(1049)),
        "the alternate screen may be on despite the error"
    );
}

#[test]
fn the_state_reports_what_it_turned_on() {
    let legacy = TerminalState::new();
    let mut sink = Vec::new();
    assert!(legacy.is_clean());
    write_setup(&mut sink, LEGACY, &legacy).expect("a `Vec` cannot fail");
    assert!(!legacy.is_clean());
    assert!(!legacy.are_keyboard_flags_pushed());
    assert!(legacy.is_cursor_hidden());

    let modern = TerminalState::new();
    let mut sink = Vec::new();
    write_setup(&mut sink, MODERN, &modern).expect("a `Vec` cannot fail");
    assert!(modern.are_keyboard_flags_pushed());
}
