//! Tests for the input adapter.
//!
//! Every test here drives the **production** parse. `terminput` ships an
//! encoder as well as a parser, so a test synthesises the exact bytes a real
//! terminal would send, feeds them through [`Event::parse_from`], and asserts on
//! the kernel key press that comes out. There is no pty, no timing and no
//! sleep, which is why the input adapter could be built before the driver
//! exists.
//!
//! Where the encoder cannot produce the bytes a real terminal sends — the
//! legacy encoding has no form for `Ctrl+Shift+Z`, and `terminput` 0.5.15
//! mis-encodes Page Up under the kitty protocol — the test writes the bytes
//! literally and says why.
//!
//! This file holds the shared fixtures; the assertions live in [`key_tests`]
//! (what each key becomes) and [`event_tests`] (everything that is not a key,
//! and the `termina` boundary).

use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use terminput::{
    Encoding, Event, KeyCode as Tc, KeyEvent as TerminalKey, KeyModifiers, KittyFlags,
};

use super::{TerminalEvent, TerminalInput, from_termina, translate};

mod event_tests;
mod key_tests;

/// Every kitty flag, as the task requires the adapter to be pinned under.
const KITTY: Encoding = Encoding::Kitty(KittyFlags::all());

/// The flag set the driver should actually push, and the reason it differs from
/// [`KITTY`] is recorded in
/// [`report_all_keys_costs_the_shifted_character`].
const KITTY_RECOMMENDED: Encoding = Encoding::Kitty(
    KittyFlags::DISAMBIGUATE_ESCAPE_CODES
        .union(KittyFlags::REPORT_EVENT_TYPES)
        .union(KittyFlags::REPORT_ALTERNATE_KEYS),
);

/// Both encodings the adapter must behave predictably under.
const BOTH: [Encoding; 2] = [Encoding::Xterm, KITTY];

// ===== helpers =====

/// Encodes an event to the bytes a terminal speaking `encoding` would send.
fn encode(event: &Event, encoding: Encoding) -> Vec<u8> {
    let mut buf = [0u8; 64];
    let written = event
        .encode(&mut buf, encoding)
        .unwrap_or_else(|e| panic!("{event:?} is not encodable under {encoding:?}: {e}"));
    buf[..written].to_vec()
}

/// Parses terminal bytes exactly as the driver will.
fn parse(bytes: &[u8]) -> Event {
    Event::parse_from(bytes)
        .unwrap_or_else(|e| panic!("{bytes:?} did not parse: {e}"))
        .unwrap_or_else(|| panic!("{bytes:?} parsed as an incomplete sequence"))
}

/// Bytes in, kernel input out — the whole production path above the terminal.
fn from_bytes(bytes: &[u8]) -> TerminalInput {
    translate(parse(bytes))
}

/// Sorts a `termina` event, failing the test if it could not be named at all.
#[track_caller]
fn sorted(event: termina::Event) -> TerminalEvent {
    from_termina(event).unwrap_or_else(|e| panic!("termina event was not convertible: {e}"))
}

/// Encodes, parses and translates, the round trip every key test uses.
fn round_trip(event: &Event, encoding: Encoding) -> TerminalInput {
    from_bytes(&encode(event, encoding))
}

/// A `terminput` key event with modifiers.
fn key(code: Tc, modifiers: KeyModifiers) -> Event {
    Event::Key(TerminalKey::new(code).modifiers(modifiers))
}

/// The expected kernel outcome for a press.
fn pressed(code: KeyCode, modifiers: Modifiers) -> TerminalInput {
    TerminalInput::Key(KeyEvent {
        key: code,
        modifiers,
        is_repeat: false,
    })
}

/// Asserts a key produces `expected` under both encodings.
#[track_caller]
fn assert_both(code: Tc, modifiers: KeyModifiers, expected: &TerminalInput) {
    for encoding in BOTH {
        assert_eq!(
            round_trip(&key(code, modifiers), encoding),
            *expected,
            "{code:?} with {modifiers:?} under {encoding:?}"
        );
    }
}
