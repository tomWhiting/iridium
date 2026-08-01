//! What each key becomes: characters, modifiers, the legacy ambiguity, and
//! the named keys.

use iridium_editor::{KeyCode, Modifiers};
use terminput::{Encoding, Event, KeyCode as Tc, KeyEvent as TerminalKey, KeyModifiers};

use super::{
    BOTH, KITTY, KITTY_RECOMMENDED, assert_both, encode, from_bytes, key, pressed, round_trip,
};
use crate::input::{TerminalInput, translate};

// ===== characters =====

#[test]
fn plain_character_survives_both_encodings() {
    assert_both(
        Tc::Char('a'),
        KeyModifiers::NONE,
        &pressed(KeyCode::Char('a'), Modifiers::none()),
    );
}

#[test]
fn capital_letter_keeps_its_case_and_carries_shift_under_both_encodings() {
    // The encodings disagree at the wire: xterm sends the byte `Z` and infers
    // shift from the case, while the kitty protocol sends the base keycode
    // `122` with the shifted codepoint `90` and *clears* the shift bit, because
    // the codepoint already says it. Both must reach the kernel as the
    // character the user typed, with shift held, or a `Shift+Z` binding would
    // match on one terminal and not the other and typing a capital would insert
    // a lowercase letter.
    assert_both(
        Tc::Char('Z'),
        KeyModifiers::SHIFT,
        &pressed(KeyCode::Char('Z'), Modifiers::shift()),
    );
}

#[test]
fn control_letters_round_trip_under_both_encodings() {
    for letter in 'a'..='z' {
        // `i` and `m` collide with Tab and Enter under the legacy encoding; see
        // `ctrl_i_and_ctrl_m_are_tab_and_enter_under_the_legacy_encoding`.
        let encodings: &[Encoding] = if matches!(letter, 'i' | 'm') {
            &[KITTY]
        } else {
            &BOTH
        };
        for &encoding in encodings {
            assert_eq!(
                round_trip(&key(Tc::Char(letter), KeyModifiers::CTRL), encoding),
                pressed(KeyCode::Char(letter), Modifiers::ctrl()),
                "Ctrl+{letter} under {encoding:?}"
            );
        }
    }
}

#[test]
fn ctrl_i_and_ctrl_m_are_tab_and_enter_under_the_legacy_encoding() {
    // Byte 9 and byte 13 have meant Tab and Enter since the VT100, and that is
    // all the legacy stream carries. The named key wins; the kitty protocol
    // tells the two apart and is checked alongside so the difference is visible.
    for (letter, named) in [('i', KeyCode::Tab), ('m', KeyCode::Enter)] {
        let event = key(Tc::Char(letter), KeyModifiers::CTRL);
        assert_eq!(
            round_trip(&event, Encoding::Xterm),
            pressed(named, Modifiers::none())
        );
        assert_eq!(
            round_trip(&event, KITTY),
            pressed(KeyCode::Char(letter), Modifiers::ctrl())
        );
    }
}

#[test]
fn ctrl_space_is_a_character_press_not_a_null_byte() {
    assert_both(
        Tc::Char(' '),
        KeyModifiers::CTRL,
        &pressed(KeyCode::Char(' '), Modifiers::ctrl()),
    );
}

// ===== modifiers =====

#[test]
fn each_modifier_reaches_its_own_bit() {
    // A shifted letter is also an upper-case letter — that is what shift *does*
    // — so the expected character is part of each case rather than assumed.
    let cases = [
        (KeyModifiers::SHIFT, 'P', Modifiers::shift()),
        (KeyModifiers::CTRL, 'p', Modifiers::ctrl()),
        (
            KeyModifiers::ALT,
            'p',
            Modifiers {
                alt: true,
                ..Modifiers::none()
            },
        ),
        (
            KeyModifiers::CTRL | KeyModifiers::SHIFT,
            'P',
            Modifiers::ctrl_shift(),
        ),
        (
            KeyModifiers::CTRL | KeyModifiers::ALT,
            'p',
            Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::none()
            },
        ),
    ];
    for (terminal, character, kernel) in cases {
        assert_eq!(
            round_trip(&key(Tc::Char('p'), terminal), KITTY),
            pressed(KeyCode::Char(character), kernel),
            "{terminal:?}"
        );
    }
}

#[test]
fn super_hyper_and_meta_all_land_on_the_kernel_meta_bit() {
    // The kernel has one meta bit for the Command/Windows key family. Super,
    // meta and hyper are merged onto it rather than any of them being dropped:
    // a dropped modifier would turn `Hyper+P` into a bare `p` and insert a
    // character nobody typed.
    let meta = Modifiers {
        meta: true,
        ..Modifiers::none()
    };
    for modifier in [KeyModifiers::SUPER, KeyModifiers::META, KeyModifiers::HYPER] {
        assert_eq!(
            round_trip(&key(Tc::Char('p'), modifier), KITTY),
            pressed(KeyCode::Char('p'), meta),
            "{modifier:?}"
        );
    }
}

#[test]
fn the_legacy_encoding_carries_no_super_hyper_or_meta_at_all() {
    // Not a mapping choice: xterm has no form for these, so the byte on the
    // wire is a bare `p`. The adapter must report the bare `p` rather than
    // inventing a modifier that was never transmitted.
    for modifier in [KeyModifiers::SUPER, KeyModifiers::META, KeyModifiers::HYPER] {
        assert_eq!(
            round_trip(&key(Tc::Char('p'), modifier), Encoding::Xterm),
            pressed(KeyCode::Char('p'), Modifiers::none()),
            "{modifier:?}"
        );
    }
}

#[test]
fn alt_graph_is_never_inferred_from_ctrl_plus_alt() {
    // The kernel uses Ctrl+Alt for add-cursor-above/below and has an
    // `alt_graph` bit so a browser can exclude an AltGr composition from it. No
    // terminal protocol reports AltGr, so guessing it here would silently kill
    // those two chords.
    let TerminalInput::Key(event) = round_trip(
        &key(Tc::Char('p'), KeyModifiers::CTRL | KeyModifiers::ALT),
        KITTY,
    ) else {
        panic!("expected a key press");
    };
    assert!(!event.modifiers.alt_graph);
}

// ===== the load-bearing legacy ambiguity =====

#[test]
fn ctrl_shift_z_is_indistinguishable_from_ctrl_z_under_xterm_only() {
    let ctrl_z = key(Tc::Char('z'), KeyModifiers::CTRL);
    let ctrl_shift_z = key(Tc::Char('z'), KeyModifiers::CTRL | KeyModifiers::SHIFT);

    // Legacy: Ctrl+Z is the single byte 26 and there is no other byte to send,
    // so `terminput`'s encoder refuses Ctrl+Shift+Z outright. The byte a real
    // terminal does send for both is 26, and it reaches the kernel as Ctrl+Z
    // with shift absent. A `Ctrl+Shift+Z` binding is therefore unreachable on a
    // terminal without the kitty protocol — a fact about the terminal, not a
    // gap to paper over.
    assert_eq!(encode(&ctrl_z, Encoding::Xterm), vec![26]);
    let mut buf = [0u8; 64];
    assert!(
        ctrl_shift_z.encode(&mut buf, Encoding::Xterm).is_err(),
        "the legacy encoding has no form for Ctrl+Shift+Z"
    );
    assert_eq!(
        from_bytes(&[26]),
        pressed(KeyCode::Char('z'), Modifiers::ctrl())
    );

    // Kitty: base keycode plus a modifier bitmask, so the two are distinct
    // bytes and stay distinct all the way into the kernel.
    assert_ne!(
        encode(&ctrl_z, KITTY),
        encode(&ctrl_shift_z, KITTY),
        "the kitty protocol must distinguish the two on the wire"
    );
    assert_eq!(
        round_trip(&ctrl_z, KITTY),
        pressed(KeyCode::Char('z'), Modifiers::ctrl())
    );
    assert_eq!(
        round_trip(&ctrl_shift_z, KITTY),
        pressed(KeyCode::Char('Z'), Modifiers::ctrl_shift())
    );
}

#[test]
fn report_all_keys_costs_the_shifted_character() {
    // Typing `!` sends the byte `!` under xterm, and under the kitty protocol
    // with `REPORT_ALTERNATE_KEYS` the terminal sends the shifted codepoint
    // too. But `REPORT_ALL_KEYS_AS_ESCAPE_CODES` puts every printable key into
    // CSI-u form, and `terminput` 0.5.15 only emits an alternate codepoint for
    // ASCII *letters* — so `Shift+1` arrives as the base key `1` with shift,
    // and the shifted glyph is gone. This is why the driver should push
    // `KITTY_RECOMMENDED` and leave plain text in its legacy form.
    assert_eq!(
        round_trip(&key(Tc::Char('!'), KeyModifiers::NONE), Encoding::Xterm),
        pressed(KeyCode::Char('!'), Modifiers::none())
    );
    assert_eq!(
        round_trip(&key(Tc::Char('!'), KeyModifiers::NONE), KITTY_RECOMMENDED),
        pressed(KeyCode::Char('!'), Modifiers::none())
    );
    assert_eq!(
        round_trip(&key(Tc::Char('1'), KeyModifiers::SHIFT), KITTY),
        pressed(KeyCode::Char('1'), Modifiers::shift()),
        "with every kitty flag on, the shifted glyph is not recoverable"
    );
}

// ===== named keys =====

#[test]
fn navigation_keys_round_trip_under_both_encodings() {
    let cases = [
        (Tc::Left, KeyCode::Left),
        (Tc::Right, KeyCode::Right),
        (Tc::Up, KeyCode::Up),
        (Tc::Down, KeyCode::Down),
        (Tc::Home, KeyCode::Home),
        (Tc::End, KeyCode::End),
    ];
    for (terminal, kernel) in cases {
        assert_both(
            terminal,
            KeyModifiers::NONE,
            &pressed(kernel, Modifiers::none()),
        );
        assert_both(
            terminal,
            KeyModifiers::CTRL,
            &pressed(kernel, Modifiers::ctrl()),
        );
    }
}

#[test]
fn page_keys_round_trip_from_the_bytes_terminals_actually_send() {
    // Page Up and Page Down keep their legacy `CSI 5 ~` form under the kitty
    // protocol as well, so one set of bytes covers both encodings. They are
    // written literally because `terminput` 0.5.15's kitty encoder appends its
    // `u` terminator to the legacy form and emits `ESC [ 5 ~ u`, which its own
    // parser rejects — an upstream encoder bug that cannot affect us, since
    // Iridium only ever parses.
    assert_eq!(
        from_bytes(b"\x1b[5~"),
        pressed(KeyCode::PageUp, Modifiers::none())
    );
    assert_eq!(
        from_bytes(b"\x1b[6~"),
        pressed(KeyCode::PageDown, Modifiers::none())
    );
    assert_eq!(
        from_bytes(b"\x1b[5;5~"),
        pressed(KeyCode::PageUp, Modifiers::ctrl())
    );
    assert_eq!(
        from_bytes(b"\x1b[6;2~"),
        pressed(KeyCode::PageDown, Modifiers::shift())
    );
}

#[test]
fn editing_keys_round_trip_under_both_encodings() {
    let cases = [
        (Tc::Enter, KeyCode::Enter),
        (Tc::Tab, KeyCode::Tab),
        (Tc::Backspace, KeyCode::Backspace),
        (Tc::Delete, KeyCode::Delete),
        (Tc::Esc, KeyCode::Escape),
    ];
    for (terminal, kernel) in cases {
        assert_both(
            terminal,
            KeyModifiers::NONE,
            &pressed(kernel, Modifiers::none()),
        );
    }
}

#[test]
fn a_bare_escape_byte_is_the_escape_key() {
    // The one byte that matters most for a modal keymap, and the one the
    // legacy stream is most ambiguous about: it is also the introducer of every
    // escape sequence. Termina resolves that ambiguity before this module sees
    // it; what must hold here is that a lone ESC is the Escape key.
    assert_eq!(
        from_bytes(b"\x1b"),
        pressed(KeyCode::Escape, Modifiers::none())
    );
}

#[test]
fn shift_tab_keeps_its_shift() {
    assert_both(
        Tc::Tab,
        KeyModifiers::SHIFT,
        &pressed(KeyCode::Tab, Modifiers::shift()),
    );
}

#[test]
fn function_keys_f1_to_f12_round_trip_under_both_encodings() {
    let kernel = [
        KeyCode::F1,
        KeyCode::F2,
        KeyCode::F3,
        KeyCode::F4,
        KeyCode::F5,
        KeyCode::F6,
        KeyCode::F7,
        KeyCode::F8,
        KeyCode::F9,
        KeyCode::F10,
        KeyCode::F11,
        KeyCode::F12,
    ];
    for (index, code) in kernel.into_iter().enumerate() {
        let number = u8::try_from(index + 1).expect("twelve fits in a u8");
        assert_both(
            Tc::F(number),
            KeyModifiers::NONE,
            &pressed(code, Modifiers::none()),
        );
    }
}

#[test]
fn keys_the_kernel_cannot_name_are_reported_not_dropped() {
    // Each of these is a real key with no kernel `KeyCode`. Reporting them as
    // `Unmapped` keeps the driver able to log or ignore them deliberately;
    // folding them onto a nearby code would fire the wrong binding.
    let unmapped = [
        key(Tc::Insert, KeyModifiers::NONE),
        key(Tc::F(13), KeyModifiers::NONE),
        key(Tc::CapsLock, KeyModifiers::NONE),
        key(Tc::PrintScreen, KeyModifiers::NONE),
        key(Tc::Menu, KeyModifiers::NONE),
        key(Tc::KeypadBegin, KeyModifiers::NONE),
        key(Tc::Media(terminput::MediaKeyCode::Play), KeyModifiers::NONE),
        key(
            Tc::Modifier(
                terminput::ModifierKeyCode::IsoLevel3Shift,
                terminput::ModifierDirection::Unknown,
            ),
            KeyModifiers::NONE,
        ),
    ];
    for event in unmapped {
        assert!(
            matches!(translate(event.clone()), TerminalInput::Unmapped(_)),
            "{event:?} should be reported as unmapped"
        );
    }
}

#[test]
fn bare_modifier_keys_map_to_the_kernel_modifier_codes() {
    use terminput::{ModifierDirection, ModifierKeyCode};

    let cases = [
        (ModifierKeyCode::Shift, KeyCode::Shift),
        (ModifierKeyCode::Control, KeyCode::Control),
        (ModifierKeyCode::Alt, KeyCode::Alt),
        (ModifierKeyCode::Super, KeyCode::Meta),
        (ModifierKeyCode::Meta, KeyCode::Meta),
        (ModifierKeyCode::Hyper, KeyCode::Meta),
    ];
    for (terminal, kernel) in cases {
        let event = Event::Key(TerminalKey::new(Tc::Modifier(
            terminal,
            ModifierDirection::Left,
        )));
        let TerminalInput::Key(press) = translate(event) else {
            panic!("{terminal:?} should be a key press");
        };
        assert_eq!(press.key, kernel);
    }
}
