//! A minimal modal keymap: two modes, seven bindings, nothing shipped on.
//!
//! # What this is for
//!
//! This is the smallest keymap that exercises the whole modal path at once —
//! the resolver's mode, mode-scoped bindings, a mode switch, the typing gate,
//! and the statusline that renders the mode name. It exists so that path can be
//! proven end to end **before** a grammar is chosen, and it is deliberately too
//! small to be anyone's daily driver.
//!
//! ⚠️ **Nothing pushes it.** No face installs this keymap, and the setting that
//! would choose one does not exist yet. It is reached by name, from tests and
//! from whatever installs it later.
//!
//! # What it deliberately is not
//!
//! Not a grammar. `d`, `y`, `c`, `w`, `v`, counts, operators and text objects
//! are all absent, because whether they compose the Vim way (an operator waits
//! for a motion) or the Helix way (a motion selects and a verb acts on the
//! selection) is an open question — see `docs/IN-FLIGHT-113-modality.md`. Every
//! binding below is one both answers agree on, so none of them has to be
//! unpicked when that question is settled.
//!
//! # The modes
//!
//! | mode | what it is |
//! | --- | --- |
//! | [`NORMAL`] | keys are verbs; **typing is off** ([`Keymap::silence_typing_in`]) |
//! | [`INSERT`] | keys are text, exactly as the non-modal keymap behaves |
//!
//! A session starts in [`NORMAL`], declared on the keymap itself rather than
//! left for a face to remember — see [`Keymap::set_initial_mode`].
//!
//! # Why a bare letter forbids every modifier
//!
//! `h` means *left* only when it is `h` alone. `Ctrl+H`, `Alt+H` and `Cmd+H`
//! belong to whatever binds them, and `AltGraph` is forbidden for the reason the
//! non-modal keymap forbids it on its add-cursor chord: on many layouts `AltGr`
//! is reported as `Ctrl+Alt` held while a character is being composed, and a
//! composed character must not fire a motion.
//!
//! `Shift` is forbidden rather than ignored because keymap resolution works on
//! the *normalized* keypress, where `J` and `j` are the same key with the shift
//! bit set. Ignoring `Shift` would make `J` mean *down*, and would silently
//! claim every capital letter a richer grammar wants for itself.

use super::builtin::{CURSOR_CHAR_LEFT, CURSOR_CHAR_RIGHT, CURSOR_LINE_DOWN, CURSOR_LINE_UP};
use super::{KeyBinding, Keymap, ModeName, ModifierPattern, ModifierState, StrokePattern};
use crate::input::KeyCode;

use ModifierState::{Any, Forbidden};

/// Keys are verbs, and typing is off.
pub const NORMAL: ModeName = ModeName::from_static("normal");
/// Keys are text.
pub const INSERT: ModeName = ModeName::from_static("insert");

/// The number of bindings in the minimal modal keymap.
///
/// Asserted in the module tests so the documented count cannot drift.
pub const MODAL_KEYMAP_BINDING_COUNT: usize = 7;

/// A bare key: every modifier absent, including `AltGraph`. See the module docs.
const BARE: ModifierPattern =
    ModifierPattern::new(Forbidden, Forbidden, Forbidden, Forbidden, Forbidden);

/// `Escape`, whatever is held with it — leaving a mode must not depend on which
/// modifiers a terminal managed to report.
const ESCAPE_ANY: ModifierPattern = ModifierPattern::new(Any, Any, Any, Any, Any);

/// A bare-letter stroke.
const fn letter(key: char) -> StrokePattern {
    StrokePattern::new(KeyCode::Char(key), BARE)
}

/// Builds the minimal modal keymap.
///
/// Seven bindings across two modes; see the module docs for what is in it and
/// what is deliberately left out.
#[must_use]
pub fn minimal_modal_keymap() -> Keymap {
    let mut keymap = Keymap::new("modal-minimal");

    // Normal mode: the four motions that already exist in every grammar.
    keymap.push(KeyBinding::new(letter('h'), &[], CURSOR_CHAR_LEFT).in_mode(NORMAL));
    keymap.push(KeyBinding::new(letter('j'), &[], CURSOR_LINE_DOWN).in_mode(NORMAL));
    keymap.push(KeyBinding::new(letter('k'), &[], CURSOR_LINE_UP).in_mode(NORMAL));
    keymap.push(KeyBinding::new(letter('l'), &[], CURSOR_CHAR_RIGHT).in_mode(NORMAL));

    // The two ways in, and the one way out.
    keymap.push(KeyBinding::enter_mode(letter('i'), &[], INSERT).in_mode(NORMAL));
    // `a` is "insert after the caret", which is a motion *and* a mode switch —
    // the case `then_enter_mode` exists for.
    keymap.push(
        KeyBinding::new(letter('a'), &[], CURSOR_CHAR_RIGHT)
            .in_mode(NORMAL)
            .then_enter_mode(INSERT),
    );
    keymap.push(
        KeyBinding::enter_mode(StrokePattern::new(KeyCode::Escape, ESCAPE_ANY), &[], NORMAL)
            .in_mode(INSERT),
    );

    // The two declarations that make the bindings above mean anything. Without
    // the first, every unbound letter would type into the document; without the
    // second, a session would begin in no mode at all and none of the bindings
    // would match.
    keymap.silence_typing_in(NORMAL);
    keymap.set_initial_mode(Some(NORMAL));

    keymap
}
