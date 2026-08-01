//! Everything that is not a key press: release and repeat reports, paste,
//! focus, resize, mouse, and the `termina` boundary.

use iridium_editor::{KeyCode, KeyEvent, KeyPress, Modifiers};
use terminput::{
    Encoding, Event, KeyCode as Tc, KeyEvent as TerminalKey, KeyEventKind, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};

use super::{BOTH, KITTY, from_bytes, pressed, round_trip, sorted};
use crate::input::{CapabilityReply, TerminalEvent, TerminalInput, from_termina, translate};

// ===== event kinds =====

#[test]
fn a_key_release_is_its_own_outcome() {
    // Only the kitty protocol reports releases. They must not reach the keymap:
    // a driver that fed them in would run every binding twice.
    let release = Event::Key(TerminalKey::new(Tc::Char('z')).kind(KeyEventKind::Release));
    assert_eq!(
        round_trip(&release, KITTY),
        TerminalInput::Release(KeyPress::new(KeyCode::Char('z'), Modifiers::none()))
    );
}

#[test]
fn a_key_repeat_sets_the_repeat_flag() {
    let repeat = Event::Key(TerminalKey::new(Tc::Char('z')).kind(KeyEventKind::Repeat));
    assert_eq!(
        round_trip(&repeat, KITTY),
        TerminalInput::Key(KeyEvent {
            key: KeyCode::Char('z'),
            modifiers: Modifiers::none(),
            is_repeat: true,
        })
    );
}

// ===== paste, focus, resize, mouse =====

#[test]
fn a_paste_is_one_event_not_a_stream_of_key_presses() {
    // Bracketed paste is the whole reason the driver enables it: pasted text
    // arrives whole, so the editor can apply it as one reversible command
    // instead of one undo step per character with auto-indent running over it.
    let text = "fn main() {\n    println!(\"hi\");\n}\n";
    for encoding in BOTH {
        assert_eq!(
            round_trip(&Event::Paste(text.to_string()), encoding),
            TerminalInput::Paste(text.to_string()),
            "under {encoding:?}"
        );
    }
    // Including the newlines: pasting must not become a series of Enter keys,
    // which would fire auto-indent on every line.
    assert_eq!(
        from_bytes(b"\x1b[200~a\nb\x1b[201~"),
        TerminalInput::Paste("a\nb".to_string())
    );
}

#[test]
fn focus_and_resize_events_translate() {
    assert_eq!(translate(Event::FocusGained), TerminalInput::FocusGained);
    assert_eq!(translate(Event::FocusLost), TerminalInput::FocusLost);
    assert_eq!(
        translate(Event::Resize {
            rows: 40,
            cols: 120
        }),
        TerminalInput::Resize {
            rows: 40,
            cols: 120,
        }
    );
}

#[test]
fn a_mouse_event_is_carried_rather_than_dropped() {
    // Converting a pointer event needs the viewport geometry the frame layer
    // owns, so it is not converted here — but dropping it would lose the click
    // entirely once that layer exists.
    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 7,
        row: 3,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(
        round_trip(&Event::Mouse(mouse), Encoding::Xterm),
        TerminalInput::Mouse(mouse)
    );
}

// ===== the termina boundary =====

#[test]
fn capability_replies_are_routed_rather_than_swallowed() {
    use termina::escape::{
        csi::{Csi, Keyboard, KittyKeyboardFlags},
        dcs::{Dcs, DcsRequest},
        osc::Osc,
    };

    // These are the terminal answering a query, not the user typing. Losing
    // them is how capability negotiation stops working with nobody noticing,
    // so each must arrive on the capability channel with its parsed value
    // intact — not as an error string, and not at all on the input channel.
    let keyboard = Csi::Keyboard(Keyboard::ReportFlags(
        KittyKeyboardFlags::DISAMBIGUATE_ESCAPE_CODES,
    ));
    assert_eq!(
        sorted(termina::Event::Csi(keyboard.clone())),
        TerminalEvent::Capability(CapabilityReply::Csi(keyboard))
    );

    let osc = Osc::SetWindowTitle("iridium");
    assert_eq!(
        sorted(termina::Event::Osc(osc.clone())),
        TerminalEvent::Capability(CapabilityReply::Osc(osc))
    );

    let dcs = Dcs::Request(DcsRequest::GraphicRendition);
    assert_eq!(
        sorted(termina::Event::Dcs(dcs.clone())),
        TerminalEvent::Capability(CapabilityReply::Dcs(dcs))
    );
}

#[test]
fn termina_input_events_reach_the_kernel_channel() {
    use termina::event::{
        KeyCode as TerminaCode, KeyEvent as TerminaKey, Modifiers as TerminaMods,
    };

    assert_eq!(
        sorted(termina::Event::Key(TerminaKey::new(
            TerminaCode::Char('a'),
            TerminaMods::CONTROL,
        ))),
        TerminalEvent::Input(pressed(KeyCode::Char('a'), Modifiers::ctrl()))
    );
    assert_eq!(
        sorted(termina::Event::Paste("x".to_string())),
        TerminalEvent::Input(TerminalInput::Paste("x".to_string()))
    );
    assert_eq!(
        sorted(termina::Event::FocusIn),
        TerminalEvent::Input(TerminalInput::FocusGained)
    );
    assert_eq!(
        sorted(termina::Event::FocusOut),
        TerminalEvent::Input(TerminalInput::FocusLost)
    );
    assert_eq!(
        sorted(termina::Event::WindowResized(termina::WindowSize {
            cols: 80,
            rows: 24,
            pixel_width: None,
            pixel_height: None,
        })),
        TerminalEvent::Input(TerminalInput::Resize { rows: 24, cols: 80 })
    );
}

#[test]
fn an_unnameable_termina_key_is_an_error_not_a_silent_drop() {
    use termina::event::{
        KeyCode as TerminaCode, KeyEvent as TerminaKey, Modifiers as TerminaMods,
    };

    // `KeyCode::Null` is termina's "the parser could not name this key". It has
    // no terminput equivalent, so the conversion fails — and the failure is
    // returned rather than absorbed, because an input event that vanishes is a
    // key that mysteriously does nothing.
    assert!(
        from_termina(termina::Event::Key(TerminaKey::new(
            TerminaCode::Null,
            TerminaMods::NONE,
        )))
        .is_err()
    );
}
