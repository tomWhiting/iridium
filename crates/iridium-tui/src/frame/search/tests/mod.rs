//! The overlay driven exactly as a user drives it: keys in, cells out.
//!
//! Nothing here reaches into the panel's private state to arrange a result. A
//! test types the keys, renders a frame, and asserts on the kernel's own
//! counters and on the cells that came back — which is the only way to catch a
//! panel that is internally consistent and telling the user something false.
//!
//! # One file per subject
//!
//! ⚠️ This was a single 1,136-line file against a 1,000-line hard limit, and it
//! was already over before #117 converted this panel's keys. Split for #92 on
//! the `// ==================== … ====================` rules it already
//! carried, grouped into four files by subject; no test moved past a
//! neighbour.
//!
//! What stays here is what every file below needs: the two editor fixtures,
//! the key helpers, the search counter, and [`Screen`] — the rendered-cell
//! reader that makes "what the user saw" assertable.
//!
//! # ⛔ A file missing from the list below is silently ignored
//!
//! `cargo` reads this list, not the directory: an orphaned `.rs` beside it
//! compiles clean, takes its tests with it, and leaves the suite green — none
//! of the ten gates reports it. Measured, with numbers, in
//! `docs/CODING_STANDARDS.md`. Take a test count either side of any add,
//! remove or rename here.

mod kernel;
mod layout;
mod replace;
mod text;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use iridium_editor::{Editor, EditorEvent, KeyCode, KeyEvent, Modifiers, Position};

use super::{SearchOutcome, SearchOverlay};
use crate::cell::{CellBuffer, CellContent, Color};
use crate::frame::{Chrome, Frame, FrameLayout};

/// A family emoji: seven `char`s, one cluster, two cells.
const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";

/// An editor holding `content`, with the caret at the start.
fn editor_with(content: &str) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(content);
    editor.set_cursor(Position::new(0, 0));
    editor
}

/// An open panel over `editor`.
fn opened(editor: &mut Editor) -> SearchOverlay {
    let mut overlay = SearchOverlay::new(&iridium_editor::Keymap::new("empty"));
    overlay.open(editor);
    overlay
}

/// Alt alone.
const fn alt() -> Modifiers {
    Modifiers {
        shift: false,
        ctrl: false,
        alt: true,
        meta: false,
        alt_graph: false,
    }
}

/// Control and Alt together.
const fn ctrl_alt() -> Modifiers {
    Modifiers {
        shift: false,
        ctrl: true,
        alt: true,
        meta: false,
        alt_graph: false,
    }
}

/// Presses one unmodified key.
fn press(overlay: &mut SearchOverlay, editor: &mut Editor, code: KeyCode) -> SearchOutcome {
    overlay.handle_key(&KeyEvent::simple(code), editor)
}

/// Presses one chord.
fn chord(
    overlay: &mut SearchOverlay,
    editor: &mut Editor,
    code: KeyCode,
    modifiers: Modifiers,
) -> SearchOutcome {
    overlay.handle_key(&KeyEvent::new(code, modifiers), editor)
}

/// Types every character of `text` into the focused field.
fn type_text(overlay: &mut SearchOverlay, editor: &mut Editor, text: &str) {
    for character in text.chars() {
        let outcome = press(overlay, editor, KeyCode::Char(character));
        assert_eq!(outcome, SearchOutcome::Handled, "typing {character:?}");
    }
}

/// Counts every search the kernel runs from now on.
///
/// `SearchUpdated` is emitted by every kernel search verb and by nothing else,
/// so this is the only way to see a redundant re-run: repeating a search over
/// an unchanged query lands on the same match and is invisible in the state it
/// leaves behind. It is a full pass over the document all the same, and this
/// panel is on the keystroke path.
fn count_searches(editor: &mut Editor) -> Arc<AtomicUsize> {
    let searches = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&searches);
    editor.add_listener(move |event| {
        if matches!(event, EditorEvent::SearchUpdated { .. }) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    });
    searches
}

/// One rendered frame.
struct Screen {
    /// The cells.
    buffer: CellBuffer,
    /// Where everything went.
    layout: FrameLayout,
}

impl Screen {
    /// The text of a row, with continuation cells contributing nothing.
    fn row(&self, row: usize) -> String {
        let mut out = String::new();
        let Some(cells) = self.buffer.row(row) else {
            return out;
        };
        for cell in cells {
            match cell.content() {
                CellContent::Grapheme(grapheme) => grapheme.push_to(&mut out),
                CellContent::Continuation => {},
            }
        }
        out
    }

    /// The panel's first row, which carries the query.
    fn find_row(&self) -> String {
        let (first, _) = self.layout.search_rows.expect("the panel must be laid out");
        self.row(first)
    }

    /// The panel's second row, which carries the replacement and the message.
    fn message_row(&self) -> String {
        let (first, count) = self.layout.search_rows.expect("the panel must be laid out");
        assert!(count > 1, "this screen has no message row");
        self.row(first + 1)
    }

    /// The background of one cell.
    fn background(&self, column: usize, row: usize) -> Option<Color> {
        self.buffer
            .get(column, row)
            .map(|cell| cell.style().background)
    }
}

/// Renders one frame with the panel open.
fn render(editor: &Editor, overlay: &SearchOverlay, width: usize, height: usize) -> Screen {
    let mut buffer = CellBuffer::new(width, height);
    let mut frame = Frame::new();
    let chrome = Chrome {
        status: crate::frame::Status::default(),
        search: Some(overlay),
        sidebar_columns: 0,
    };
    let layout = frame.render(editor, chrome, &mut buffer);
    Screen { buffer, layout }
}
