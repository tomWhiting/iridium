//! Terminal input, converted to the kernel's platform-neutral key presses.
//!
//! Step 2 of `docs/TERMINAL-FACE-PLAN.md`. The pipeline from bytes to a bound
//! command is three stages and **only the last one is in this module**:
//!
//! ```text
//! terminal bytes -> termina::Event -> terminput::Event -> iridium KeyEvent
//!                                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ here
//! ```
//!
//! `termina` does the VT parsing, [`terminput_termina::to_terminput`] does the
//! middle conversion, and [`translate`](crate::input::translate) does the last
//! one. Because that last one is a pure function of its argument, and because
//! `terminput` ships an *encoder* as well as a parser, the whole path above the
//! terminal is testable by synthesising the exact bytes a real terminal sends
//! and parsing them back — no pty, no timing, no flake.
//!
//! # No second notion of a keystroke
//!
//! Everything this module produces is expressed in the kernel's own types:
//! [`KeyEvent`](iridium_editor::KeyEvent),
//! [`KeyPress`](iridium_editor::KeyPress),
//! [`KeyCode`](iridium_editor::KeyCode) and
//! [`Modifiers`](iridium_editor::Modifiers), all from `iridium-editor`. This
//! crate defines no key type of its own. A face that grows a parallel key
//! representation drifts from the kernel's, and the
//! keymap — which resolves [`KeyPress`](iridium_editor::KeyPress) and nothing
//! else — stops being the single place a binding is decided.
//!
//! # Legacy fallback is a first-class path
//!
//! Two encodings reach this module, and they do not carry the same information.
//!
//! Under the legacy xterm encoding a control chord is a single C0 byte:
//! `Ctrl+Z` is byte `26`, and so is `Ctrl+Shift+Z`. The shift is simply not in
//! the byte stream, so both arrive here as `Ctrl+Z` and no cleverness recovers
//! the difference. Under the kitty keyboard protocol the same key is
//! `ESC [ 122 ; 5 u` — base keycode plus a modifier bitmask — and the two are
//! distinct. The adapter therefore reports what it was given and never invents
//! a modifier: a binding on `Ctrl+Shift+Z` is unreachable on a terminal that
//! does not speak the kitty protocol, and that is a property of the terminal,
//! not a bug to be worked around here. The tests pin both halves of it.
//!
//! Three further collisions are inherent to the legacy encoding and are
//! resolved the same way — in favour of the named key, because that is what the
//! byte has meant since VT100:
//!
//! - `Ctrl+I` is byte `9`, which is `KeyCode::Tab`;
//! - `Ctrl+M` is byte `13`, which is `KeyCode::Enter`;
//! - `Ctrl+Backspace` is byte `8`, which arrives as `Ctrl+H`.
//!
//! # What the driver should ask the terminal for
//!
//! Not settled by the plan, and decided here because it is a property of this
//! conversion rather than of the driver: push
//! `DISAMBIGUATE_ESCAPE_CODES | REPORT_EVENT_TYPES | REPORT_ALTERNATE_KEYS`,
//! and **not** `REPORT_ALL_KEYS_AS_ESCAPE_CODES`.
//!
//! The first three are what make `Ctrl+Shift+Z`, key releases and shifted
//! characters reportable at all. The fourth puts *every* key into CSI-u form,
//! including plain text, and a CSI-u sequence names the key the user pressed
//! rather than the character it produced. The shifted glyph then depends on an
//! alternate keycode the terminal may not send, and on the associated-text
//! field `terminput` 0.5.15 does not parse — so `Shift+1` can arrive as `1`
//! and typing `!` inserts `1`. Leaving printable keys in their legacy form
//! avoids the whole class, and costs nothing this editor needs.
//!
//! # Capability replies are not user input
//!
//! `to_terminput` rejects raw DCS/CSI/OSC events. Those are the terminal
//! *answering a query* — device attributes, a keyboard-protocol report, a
//! colour query — and they belong to capability negotiation, not to the
//! kernel's input path. [`from_termina`](crate::input::from_termina) routes them
//! to [`TerminalEvent::Capability`](crate::input::TerminalEvent::Capability)
//! with the parsed value intact, rather than discarding them or flattening them
//! into an error string. Discarding them is how feature detection stops working
//! without anyone noticing.

use iridium_editor::{KeyEvent, KeyPress};
use termina::escape::{csi::Csi, dcs::Dcs, osc::Osc};
use terminput::{Event, KeyEventKind, MouseEvent, UnsupportedEvent};

mod keys;
#[cfg(test)]
mod tests;

/// One terminal event, sorted into the two channels it can belong to.
///
/// The split is the point: user input goes to the kernel, capability replies go
/// to terminal negotiation, and neither channel can silently swallow the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    /// Something the user did.
    Input(TerminalInput),

    /// Something the terminal said about itself, in reply to a query.
    ///
    /// Never routed to the kernel. See [`CapabilityReply`].
    Capability(CapabilityReply),
}

/// A reply the terminal sent about its own capabilities.
///
/// Carried as the parsed `termina` value rather than as text, so the
/// negotiation layer can match on it. Nothing in this module interprets one;
/// this type exists so that a reply is *routed* instead of dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityReply {
    /// A CSI response: device attributes, cursor position, a mode report, or a
    /// keyboard-protocol report.
    Csi(Csi),

    /// An OSC response, such as a dynamic colour query answer.
    Osc(Osc<'static>),

    /// A DCS response, such as a DECRQSS setting report.
    Dcs(Dcs),
}

/// One piece of user input, in the kernel's vocabulary where the kernel has one.
///
/// Every variant that the kernel can act on carries a kernel type. The two that
/// do not — [`Self::Mouse`] and [`Self::Unmapped`] — carry the `terminput`
/// value unchanged, because dropping an event the kernel cannot yet name would
/// make a key silently dead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalInput {
    /// A key press or auto-repeat, ready for
    /// [`Editor::handle_key`](iridium_editor::Editor::handle_key).
    ///
    /// [`KeyEvent::is_repeat`] is set for a kitty repeat report. No terminal
    /// reports repeats under the legacy encoding, where an auto-repeating key is
    /// indistinguishable from the same key pressed twice.
    Key(KeyEvent),

    /// A key *release*.
    ///
    /// Only ever produced under the kitty protocol with
    /// [`KittyFlags::REPORT_EVENT_TYPES`](terminput::KittyFlags::REPORT_EVENT_TYPES)
    /// enabled. The kernel binds presses, not releases, so this is reported as
    /// its own variant: a driver that fed releases into the keymap would run
    /// every binding twice.
    Release(KeyPress),

    /// Text delivered whole by bracketed paste.
    ///
    /// A paste is one event, not a stream of key presses — which is exactly why
    /// bracketed paste is worth enabling. The driver hands this to
    /// [`Editor::paste`](iridium_editor::Editor::paste), so the whole paste is
    /// one reversible command; replaying it as key presses would push one undo
    /// step per character and run auto-indent and auto-close pairs over pasted
    /// text.
    Paste(String),

    /// The terminal window gained focus.
    FocusGained,

    /// The terminal window lost focus.
    FocusLost,

    /// The terminal was resized. Dimensions are in cells.
    Resize {
        /// New height in cells.
        rows: u32,
        /// New width in cells.
        cols: u32,
    },

    /// A pointer event, carried verbatim.
    ///
    /// Deliberately *not* converted to the kernel's
    /// [`MouseEvent`](iridium_editor::MouseEvent): that conversion needs the
    /// viewport geometry which the frame layer owns (step 4 of the plan), and
    /// hit testing a cell grid is not a key mapping. Carried rather than
    /// dropped so the driver keeps the event when that layer exists.
    Mouse(MouseEvent),

    /// A key the kernel's [`KeyCode`](iridium_editor::KeyCode) cannot name.
    ///
    /// Insert, F13 and above, the lock keys, Print Screen, Pause, Menu, the
    /// keypad Begin key, media keys, and the ISO level-3/5 shift keys. Reported
    /// rather than dropped so a driver can log or ignore it deliberately; the
    /// kernel is the only place new key codes may be added.
    Unmapped(terminput::KeyEvent),
}

/// Sorts one `termina` event into input or a capability reply.
///
/// DCS, CSI and OSC events are recognised *before* `to_terminput` is consulted,
/// so a capability reply arrives as [`TerminalEvent::Capability`] with its
/// parsed value rather than as an error carrying a `Debug` string.
///
/// # Errors
///
/// Returns [`UnsupportedEvent`] only for a `termina` key event whose code is
/// `KeyCode::Null` — a key the VT parser could not name at all. Every other
/// `termina` event has an outcome here. The error is returned rather than
/// swallowed for the same reason capability replies are routed: an input event
/// that vanishes is a key that mysteriously does nothing.
pub fn from_termina(event: termina::Event) -> Result<TerminalEvent, UnsupportedEvent> {
    match event {
        termina::Event::Csi(csi) => Ok(TerminalEvent::Capability(CapabilityReply::Csi(csi))),
        termina::Event::Osc(osc) => Ok(TerminalEvent::Capability(CapabilityReply::Osc(osc))),
        termina::Event::Dcs(dcs) => Ok(TerminalEvent::Capability(CapabilityReply::Dcs(dcs))),
        input => terminput_termina::to_terminput(input)
            .map(|event| TerminalEvent::Input(translate(event))),
    }
}

/// Converts a `terminput` event into the kernel's vocabulary.
///
/// Total: every event has an outcome, and nothing is dropped. This is the only
/// conversion in the input path that Iridium owns.
///
/// # Case and the shift modifier
///
/// The event is passed through [`terminput::KeyEvent::normalize_case`] first,
/// which is what makes the two encodings agree. The legacy encoding reports a
/// capital as `Char('Z')` with shift; the kitty protocol reports either
/// `Char('z')` with shift, or — with
/// [`REPORT_ALTERNATE_KEYS`](terminput::KittyFlags::REPORT_ALTERNATE_KEYS) —
/// `Char('Z')` with the shift bit *cleared*, because the shifted codepoint
/// already says everything. Normalising makes all three `Char('Z')` with shift,
/// so one binding matches on every terminal and inserted text is the character
/// the user actually typed.
///
/// No further normalisation happens here. The kernel lowercases a
/// [`KeyPress`] when it *compares* it ([`KeyPress::normalized`]) and keeps the
/// original for insertion; lowercasing at this boundary would turn a typed `A`
/// into an `a`.
#[must_use]
pub fn translate(event: Event) -> TerminalInput {
    match event {
        Event::Key(key) => translate_key(key),
        Event::Mouse(mouse) => TerminalInput::Mouse(mouse),
        Event::Paste(text) => TerminalInput::Paste(text),
        Event::FocusGained => TerminalInput::FocusGained,
        Event::FocusLost => TerminalInput::FocusLost,
        Event::Resize { rows, cols } => TerminalInput::Resize { rows, cols },
    }
}

/// Converts one key event, reporting a key the kernel cannot name as
/// [`TerminalInput::Unmapped`].
fn translate_key(key: terminput::KeyEvent) -> TerminalInput {
    let key = key.normalize_case();
    let Some(code) = keys::key_code(key.code) else {
        return TerminalInput::Unmapped(key);
    };
    let modifiers = keys::modifiers(key.modifiers);

    match key.kind {
        KeyEventKind::Press | KeyEventKind::Repeat => TerminalInput::Key(KeyEvent {
            key: code,
            modifiers,
            is_repeat: matches!(key.kind, KeyEventKind::Repeat),
        }),
        KeyEventKind::Release => TerminalInput::Release(KeyPress::new(code, modifiers)),
    }
}
