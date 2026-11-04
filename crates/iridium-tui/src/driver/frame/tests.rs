//! The bytes a frame becomes, checked against a `Vec<u8>`.

use super::*;
use crate::cell::{CellBuffer, ColorDepth};

const TERMINAL: Capabilities = Capabilities {
    color_depth: ColorDepth::TrueColor,
    keyboard_enhancement: false,
    synchronized_output: false,
};

/// `rows` rows of numbered text starting at `first`, and a statusline.
fn screen(first: usize, rows: usize) -> CellBuffer {
    let mut buffer = CellBuffer::new(20, rows + 1);
    for row in 0..rows {
        buffer.set_str(0, row, &format!("line {}", first + row), Style::DEFAULT);
    }
    buffer.set_str(0, rows, "status", Style::DEFAULT);
    buffer
}

/// The frame that turns `front` into `back`, as text.
fn bytes(front: &CellBuffer, back: &CellBuffer) -> String {
    let damage = Damage::compute(front, back, ColorDepth::TrueColor);
    let frame = Frame {
        damage: &damage,
        capabilities: TERMINAL,
        cursor: CursorState::Hidden,
    };
    let state = TerminalState::new();
    state.set_cursor_hidden(true);
    let mut out = Vec::new();
    write_frame(&mut out, &frame, &state).expect("a `Vec` cannot fail");
    String::from_utf8(out).expect("frames are UTF-8")
}

/// The reset every shift starts with, as termina spells it.
fn reset() -> String {
    Csi::Sgr(Sgr::Reset).to_string()
}

#[test]
fn a_scroll_is_written_as_margins_a_scroll_and_the_new_row() {
    let written = bytes(&screen(10, 20), &screen(11, 20));
    let shift = format!("{}\u{1b}[1;20r\u{1b}[S\u{1b}[r", reset());
    assert!(written.starts_with(&shift), "{written:?}");
    let rest = &written[shift.len()..];
    assert!(
        rest.contains("line 30"),
        "the uncovered row is painted: {rest:?}"
    );
    assert!(
        !rest.contains("line 29"),
        "rows the terminal moved are not: {rest:?}"
    );
}

#[test]
fn scrolling_back_scrolls_down() {
    let written = bytes(&screen(10, 20), &screen(8, 20));
    let shift = format!("{}\u{1b}[1;20r\u{1b}[2T\u{1b}[r", reset());
    assert!(written.starts_with(&shift), "{written:?}");
}

#[test]
fn a_frame_without_a_shift_touches_no_margins() {
    let front = screen(10, 20);
    let mut back = front.clone();
    back.set_str(0, 4, "edited", Style::DEFAULT);
    let written = bytes(&front, &back);
    assert!(!written.contains("\u{1b}[r"), "{written:?}");
    assert!(!written.contains(";20r"), "{written:?}");
    assert!(written.contains("edited"));
}

#[test]
fn a_shift_is_much_cheaper_than_the_repaint_it_replaces() {
    let front = screen(10, 40);
    let back = screen(11, 40);
    let shifted = bytes(&front, &back).len();
    let repainted = {
        let damage = Damage::full(&back, ColorDepth::TrueColor);
        let frame = Frame {
            damage: &damage,
            capabilities: TERMINAL,
            cursor: CursorState::Hidden,
        };
        let state = TerminalState::new();
        state.set_cursor_hidden(true);
        let mut out = Vec::new();
        write_frame(&mut out, &frame, &state).expect("a `Vec` cannot fail");
        out.len()
    };
    assert!(
        shifted * 10 < repainted,
        "a one-line scroll wrote {shifted} bytes against {repainted} for a repaint"
    );
}
