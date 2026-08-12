//! The overlay driven exactly as a user drives it: keys in, cells out.
//!
//! Nothing here reaches into the panel's private state to arrange a result. A
//! test types the keys, renders a frame, and asserts on the kernel's own
//! counters and on the cells that came back — which is the only way to catch a
//! panel that is internally consistent and telling the user something false.

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
    let mut overlay = SearchOverlay::new();
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

// ==================== Driving the kernel ====================

#[test]
fn typing_a_query_searches_the_document_as_it_goes() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);

    assert_eq!(editor.search_match_count(), 0, "nothing typed yet");
    type_text(&mut overlay, &mut editor, "f");
    assert_eq!(editor.search_match_count(), 2, "`f` matches both `foo`s");
    type_text(&mut overlay, &mut editor, "oo b");
    assert_eq!(editor.search_match_count(), 1, "`foo b` matches once");
    assert!(editor.is_searching());
    assert_eq!(overlay.query(), "foo b");
}

#[test]
fn the_search_moves_the_document_to_the_match_it_found() {
    // A count that says `1 of 1` while the screen shows some other part of the
    // file is answering about a place the user cannot see.
    let mut editor = editor_with("alpha\nbeta\ngamma\nneedle\n");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "needle");
    assert_eq!(editor.cursor(), Position::new(3, 0));
}

#[test]
fn the_count_is_one_based_and_names_the_current_match() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 8);
    assert!(
        screen.find_row().contains("1 of 2"),
        "{}",
        screen.find_row()
    );
}

#[test]
fn enter_walks_forwards_through_the_matches_and_wraps() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert_eq!(editor.current_match_index(), Some(0));

    assert_eq!(
        press(&mut overlay, &mut editor, KeyCode::Enter),
        SearchOutcome::Handled
    );
    assert_eq!(editor.current_match_index(), Some(1));
    let screen = render(&editor, &overlay, 60, 8);
    assert!(
        screen.find_row().contains("2 of 2"),
        "{}",
        screen.find_row()
    );

    press(&mut overlay, &mut editor, KeyCode::Enter);
    assert_eq!(editor.current_match_index(), Some(0), "must wrap");
}

#[test]
fn up_walks_backwards_through_the_matches_and_wraps() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    press(&mut overlay, &mut editor, KeyCode::Up);
    assert_eq!(editor.current_match_index(), Some(1), "must wrap backwards");
    press(&mut overlay, &mut editor, KeyCode::Down);
    assert_eq!(editor.current_match_index(), Some(0));
}

#[test]
fn shift_enter_walks_backwards_where_the_terminal_can_say_so() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    chord(
        &mut overlay,
        &mut editor,
        KeyCode::Enter,
        Modifiers::shift(),
    );
    assert_eq!(editor.current_match_index(), Some(1));
}

#[test]
fn a_query_with_no_matches_says_so_rather_than_counting_to_zero() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "zzz");

    let row = render(&editor, &overlay, 60, 8).find_row();
    assert!(row.contains("No matches"), "{row}");
    assert!(!row.contains("0 of 0"), "{row}");
}

#[test]
fn an_empty_query_makes_no_claim_at_all() {
    // A panel that had just opened and said `No matches` would be answering a
    // question the user has not asked.
    let mut editor = editor_with("foo bar foo");
    let overlay = opened(&mut editor);

    let row = render(&editor, &overlay, 60, 8).find_row();
    assert!(!row.contains("No matches"), "{row}");
    assert!(!row.contains(" of "), "{row}");
}

#[test]
fn escape_closes_the_kernels_search() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert!(editor.is_searching());

    assert_eq!(
        press(&mut overlay, &mut editor, KeyCode::Escape),
        SearchOutcome::Closed
    );
    assert!(!editor.is_searching(), "the kernel's search must be closed");
    assert_eq!(editor.search_match_count(), 0);
    assert!(editor.search_state().all_matches().is_empty());
}

#[test]
fn re_opening_offers_the_last_query_back_and_re_runs_it() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Escape);
    assert_eq!(overlay.query(), "foo", "the field keeps its text");

    overlay.open(&mut editor);
    assert!(editor.is_searching(), "a shown query must be a live one");
    assert_eq!(editor.search_match_count(), 2);
}

// ==================== Options ====================

#[test]
fn case_sensitivity_toggles_and_re_runs_the_search() {
    let mut editor = editor_with("Foo foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert_eq!(editor.search_match_count(), 2, "insensitive by default");

    assert_eq!(
        chord(&mut overlay, &mut editor, KeyCode::Char('c'), alt()),
        SearchOutcome::Handled
    );
    assert!(overlay.options().case_sensitive);
    assert_eq!(editor.search_match_count(), 1, "only the lower-case one");

    chord(&mut overlay, &mut editor, KeyCode::Char('c'), alt());
    assert!(!overlay.options().case_sensitive);
    assert_eq!(editor.search_match_count(), 2);
}

#[test]
fn whole_word_toggles_and_re_runs_the_search() {
    let mut editor = editor_with("foo foobar");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert_eq!(editor.search_match_count(), 2);

    chord(&mut overlay, &mut editor, KeyCode::Char('w'), alt());
    assert!(overlay.options().whole_word);
    assert_eq!(editor.search_match_count(), 1, "`foobar` is not the word");
}

#[test]
fn regex_toggles_and_re_runs_the_search() {
    let mut editor = editor_with("a1 b2");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "\\w\\d");
    assert_eq!(
        editor.search_match_count(),
        0,
        "a literal search must not interpret the pattern"
    );

    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    assert!(overlay.options().regex);
    assert_eq!(editor.search_match_count(), 2);
}

#[test]
fn an_active_toggle_is_bracketed_and_an_inactive_one_is_not() {
    // The style differs too, but a sixteen-colour terminal degrades colour and
    // not text, and the state has to survive that.
    let mut editor = editor_with("Foo foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let before = render(&editor, &overlay, 60, 8).find_row();
    assert!(before.contains(" Aa "), "{before}");
    assert!(!before.contains("[Aa]"), "{before}");

    chord(&mut overlay, &mut editor, KeyCode::Char('c'), alt());
    let after = render(&editor, &overlay, 60, 8).find_row();
    assert!(after.contains("[Aa]"), "{after}");
}

#[test]
fn each_toggle_has_its_own_marker() {
    let mut editor = editor_with("foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    for (key, marker) in [('c', "[Aa]"), ('w', "[\\b]"), ('r', "[.*]")] {
        chord(&mut overlay, &mut editor, KeyCode::Char(key), alt());
        let row = render(&editor, &overlay, 60, 8).find_row();
        assert!(row.contains(marker), "{key} did not light {marker}: {row}");
        chord(&mut overlay, &mut editor, KeyCode::Char(key), alt());
    }
}

// ==================== An invalid pattern ====================

#[test]
fn an_unfinished_regex_is_feedback_rather_than_a_crash() {
    let mut editor = editor_with("abc");
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    type_text(&mut overlay, &mut editor, "[a-z");

    let message = overlay
        .error()
        .expect("an unfinished character class must be reported");
    assert!(message.contains("Invalid regex"), "{message}");
    assert_eq!(editor.search_match_count(), 0);

    let screen = render(&editor, &overlay, 60, 8);
    assert!(
        screen.find_row().contains("Invalid regex"),
        "{}",
        screen.find_row()
    );
    let clipped: String = message.chars().take(24).collect();
    assert!(
        screen.message_row().contains(&clipped),
        "the engine's own words must reach the message row: {}",
        screen.message_row()
    );
}

#[test]
fn finishing_the_regex_clears_the_complaint() {
    let mut editor = editor_with("abc");
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    type_text(&mut overlay, &mut editor, "[a-z");
    assert!(overlay.error().is_some());

    type_text(&mut overlay, &mut editor, "]");
    assert!(overlay.error().is_none(), "{:?}", overlay.error());
    assert_eq!(editor.search_match_count(), 3);
}

#[test]
fn the_panel_keeps_working_while_the_pattern_is_broken() {
    let mut editor = editor_with("abc");
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    type_text(&mut overlay, &mut editor, "[a-z");

    // Navigation, focus and closing must all still answer.
    assert_eq!(
        press(&mut overlay, &mut editor, KeyCode::Enter),
        SearchOutcome::Handled
    );
    assert_eq!(
        press(&mut overlay, &mut editor, KeyCode::Tab),
        SearchOutcome::Handled
    );
    assert_eq!(
        press(&mut overlay, &mut editor, KeyCode::Escape),
        SearchOutcome::Closed
    );
    assert!(!editor.is_searching());
}

#[test]
fn a_pattern_is_literal_until_regex_is_switched_on() {
    // `[a-z` is not a valid regex, and is a perfectly ordinary piece of text.
    let mut editor = editor_with("the set [a-z] of letters");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "[a-z");
    assert!(overlay.error().is_none(), "{:?}", overlay.error());
    assert_eq!(editor.search_match_count(), 1);
}

#[test]
fn backspacing_out_of_a_broken_pattern_recovers() {
    let mut editor = editor_with("abc");
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    type_text(&mut overlay, &mut editor, "a[");
    assert!(overlay.error().is_some());

    press(&mut overlay, &mut editor, KeyCode::Backspace);
    assert!(overlay.error().is_none(), "{:?}", overlay.error());
    assert_eq!(editor.search_match_count(), 1);
}

// ==================== Replace ====================

#[test]
fn replacing_the_current_match_edits_the_document_and_reports_it() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "baz");

    let outcome = chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('r'),
        Modifiers::ctrl(),
    );
    assert_eq!(outcome, SearchOutcome::Replaced(1));
    assert_eq!(editor.state().document.text(), "baz bar foo");

    let row = render(&editor, &overlay, 60, 8).message_row();
    assert!(row.contains("Replaced 1 match"), "{row}");
}

#[test]
fn replacing_every_match_is_one_step_and_reports_the_count() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "baz");

    let outcome = chord(&mut overlay, &mut editor, KeyCode::Char('r'), ctrl_alt());
    assert_eq!(outcome, SearchOutcome::Replaced(2));
    assert_eq!(editor.state().document.text(), "baz bar baz");

    let row = render(&editor, &overlay, 60, 8).message_row();
    assert!(row.contains("Replaced 2 matches"), "{row}");

    editor.undo();
    assert_eq!(
        editor.state().document.text(),
        "foo bar foo",
        "replace-all must be a single undoable step"
    );
}

#[test]
fn replacing_everything_leaves_the_panel_searching_for_what_it_shows() {
    // `replace_all_matches` closes the kernel's search. A panel still showing a
    // query while nothing is searching for it is a lie, and the highlighting
    // would vanish without a word.
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "baz");
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), ctrl_alt());

    assert!(
        editor.is_searching(),
        "the query is still on screen, so it must still be running"
    );
    assert_eq!(editor.search_state().query, "foo");
    assert_eq!(editor.search_match_count(), 0, "they are all gone now");
}

#[test]
fn replacing_a_match_with_nothing_deletes_it() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo ");
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), ctrl_alt());
    assert_eq!(editor.state().document.text(), "bar foo");
}

#[test]
fn a_replacement_with_nothing_to_replace_says_so() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "zzz");

    let outcome = chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('r'),
        Modifiers::ctrl(),
    );
    assert_eq!(outcome, SearchOutcome::Handled);
    assert_eq!(editor.state().document.text(), "foo bar foo");

    let row = render(&editor, &overlay, 60, 8).message_row();
    assert!(row.contains("No match to replace"), "{row}");
}

#[test]
fn a_read_only_document_refuses_a_replacement_and_says_why() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    editor.state_mut().read_only = true;

    for modifiers in [Modifiers::ctrl(), ctrl_alt()] {
        let outcome = chord(&mut overlay, &mut editor, KeyCode::Char('r'), modifiers);
        assert_eq!(outcome, SearchOutcome::Handled);
        assert_eq!(editor.state().document.text(), "foo bar foo");
        let row = render(&editor, &overlay, 60, 8).message_row();
        assert!(row.contains("Document is read-only"), "{row}");
    }
}

// ==================== The two fields ====================

#[test]
fn tab_moves_between_the_query_and_the_replacement() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "baz");

    assert_eq!(overlay.query(), "foo");
    assert_eq!(overlay.replacement(), "baz");

    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "t");
    assert_eq!(overlay.query(), "foot", "tab must come back to the query");
}

#[test]
fn typing_a_replacement_does_not_disturb_the_search() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    let before = editor.current_match_index();
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "zzzz");

    assert_eq!(editor.search_match_count(), 2);
    assert_eq!(editor.current_match_index(), before);
    assert_eq!(editor.search_state().query, "foo");
}

#[test]
fn backspacing_the_query_re_runs_the_search() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo b");
    assert_eq!(editor.search_match_count(), 1);

    press(&mut overlay, &mut editor, KeyCode::Backspace);
    press(&mut overlay, &mut editor, KeyCode::Backspace);
    assert_eq!(overlay.query(), "foo");
    assert_eq!(editor.search_match_count(), 2);
}

#[test]
fn a_caret_motion_in_the_query_costs_no_search() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Enter);
    assert_eq!(editor.current_match_index(), Some(1));

    let searches = count_searches(&mut editor);
    for code in [KeyCode::Left, KeyCode::Right, KeyCode::Home, KeyCode::End] {
        press(&mut overlay, &mut editor, code);
        assert_eq!(
            searches.load(Ordering::Relaxed),
            0,
            "{code:?} must not restart the search"
        );
        assert_eq!(editor.current_match_index(), Some(1));
    }
}

#[test]
fn typing_a_replacement_costs_no_search() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);

    let searches = count_searches(&mut editor);
    type_text(&mut overlay, &mut editor, "zzzz");
    assert_eq!(
        searches.load(Ordering::Relaxed),
        0,
        "the replacement text is not a query and must cost no pass over the document"
    );
}

#[test]
fn typing_a_query_costs_exactly_one_search_per_keystroke() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    let searches = count_searches(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert_eq!(searches.load(Ordering::Relaxed), 3);
}

#[test]
fn editing_inside_the_query_re_runs_it_from_the_middle() {
    let mut editor = editor_with("fXoo food");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert_eq!(editor.search_match_count(), 1);

    press(&mut overlay, &mut editor, KeyCode::Home);
    press(&mut overlay, &mut editor, KeyCode::Right);
    type_text(&mut overlay, &mut editor, "X");
    assert_eq!(overlay.query(), "fXoo");
    assert_eq!(editor.search_match_count(), 1);
    assert_eq!(editor.cursor(), Position::new(0, 0));
}

// ==================== A cell is not a character ====================

#[test]
fn a_double_width_query_puts_the_caret_two_cells_along_per_glyph() {
    let mut editor = editor_with("漢字 test");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "漢字");
    assert_eq!(editor.search_match_count(), 1);

    let screen = render(&editor, &overlay, 60, 8);
    let (first, _) = screen.layout.search_rows.expect("panel");
    let caret = screen.layout.caret().expect("the query field has a caret");
    assert_eq!(caret.row, first);
    // Nine cells of label, then four cells for two ideographs — not two.
    assert_eq!(
        caret.column,
        13,
        "the caret must count cells, not chars: {}",
        screen.find_row()
    );
    assert!(screen.find_row().contains("漢字"), "{}", screen.find_row());
}

#[test]
fn a_grapheme_cluster_is_one_backspace_however_many_chars_it_is() {
    let mut editor = editor_with("nothing");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, FAMILY);
    assert_eq!(overlay.query().chars().count(), 7);

    press(&mut overlay, &mut editor, KeyCode::Backspace);
    assert_eq!(
        overlay.query(),
        "",
        "one press must take the whole cluster, joiners and all"
    );
}

#[test]
fn an_emoji_query_is_two_cells_wide_in_the_caret_position() {
    let mut editor = editor_with("nothing");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, FAMILY);

    let caret = render(&editor, &overlay, 60, 8)
        .layout
        .caret()
        .expect("caret");
    assert_eq!(caret.column, 9 + 2, "seven chars, two cells");
}

#[test]
fn a_query_wider_than_its_box_scrolls_to_keep_the_caret_visible() {
    let mut editor = editor_with("nothing");
    let mut overlay = opened(&mut editor);
    // Thirty cells of query in a panel about twenty cells wide.
    type_text(&mut overlay, &mut editor, "abcdefghijklmnopqrstuvwxyz0123");

    let screen = render(&editor, &overlay, 30, 8);
    let caret = screen.layout.caret().expect("caret");
    assert!(
        caret.column < 30,
        "the caret must stay on screen: {caret:?} in {}",
        screen.find_row()
    );
    assert!(
        screen.find_row().contains("0123"),
        "the end of the query must be what is shown: {}",
        screen.find_row()
    );
}

// ==================== Match highlighting ====================

/// The background of the first cell of the text area on `row`.
fn text_background(screen: &Screen, row: usize, offset: usize) -> Color {
    screen
        .background(screen.layout.text.origin + offset, row)
        .expect("the text area must be painted")
}

#[test]
fn every_match_is_marked_and_the_current_one_is_marked_apart() {
    // Marking only the current match makes the rest invisible; marking them all
    // the same makes navigation unreadable. Both are silent: the editor still
    // renders and the counts are still right.
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    // Sampled one cell into each match: `find` parks the caret on a match's
    // first cell, and a caret paints its own background over whatever it sits
    // on — so cell zero reports the caret rather than the match.
    let screen = render(&editor, &overlay, 60, 8);
    let current = text_background(&screen, 0, 1);
    let other = text_background(&screen, 0, 9);
    let plain = text_background(&screen, 0, 4);

    assert_ne!(current, plain, "the current match must be marked");
    assert_ne!(other, plain, "every other match must be marked too");
    assert_ne!(current, other, "and the current one must stand apart");
}

#[test]
fn the_current_match_marking_follows_navigation() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    let before = render(&editor, &overlay, 60, 8);
    // One cell in, away from the caret the search parks on the match start.
    let current_style = text_background(&before, 0, 1);
    assert_ne!(
        text_background(&before, 0, 9),
        current_style,
        "the second match must not start out as the current one"
    );

    press(&mut overlay, &mut editor, KeyCode::Enter);
    let after = render(&editor, &overlay, 60, 8);
    assert_eq!(
        text_background(&after, 0, 9),
        current_style,
        "the second match must now carry the current-match background"
    );
    assert_ne!(
        text_background(&after, 0, 1),
        current_style,
        "and the first must have given it up"
    );
}

#[test]
fn a_match_covers_exactly_its_own_cells() {
    let mut editor = editor_with("xfoox");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 8);
    let outside = text_background(&screen, 0, 4);
    for offset in 1..4 {
        assert_ne!(
            text_background(&screen, 0, offset),
            outside,
            "cell {offset} is inside the match"
        );
    }
    assert_eq!(
        text_background(&screen, 0, 0),
        outside,
        "the cell before the match must be untouched"
    );
}

#[test]
fn a_double_width_match_is_marked_across_both_of_its_cells() {
    let mut editor = editor_with("a漢b");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "漢");

    let screen = render(&editor, &overlay, 60, 8);
    let marked = text_background(&screen, 0, 1);
    assert_eq!(
        text_background(&screen, 0, 2),
        marked,
        "the continuation half must carry its glyph's background"
    );
    assert_ne!(text_background(&screen, 0, 3), marked, "the `b` must not");
}

#[test]
fn a_match_after_a_double_width_glyph_is_measured_in_cells() {
    // `字` is the second *char* of the line and its third *cell*. Resolving a
    // match's document columns as cells marks the ideograph before it instead,
    // which is the whole reason ranges go through `LineLayout`.
    let mut editor = editor_with("漢字x");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "字");
    assert_eq!(editor.search_match_count(), 1);

    let screen = render(&editor, &overlay, 60, 8);
    let plain = text_background(&screen, 0, 4);
    assert_ne!(
        text_background(&screen, 0, 2),
        plain,
        "`字` is cells 2 and 3"
    );
    assert_ne!(text_background(&screen, 0, 3), plain);
    assert_eq!(
        text_background(&screen, 0, 0),
        plain,
        "`漢` is not the match and must not be marked"
    );
    assert_eq!(text_background(&screen, 0, 1), plain, "nor its second half");
}

#[test]
fn a_match_ending_after_a_double_width_glyph_covers_its_last_cell() {
    // `漢a` is two document columns and three cells. Treating the match's end
    // column as a cell stops the mark one glyph short and leaves the `a`
    // unmarked — visible only because the ideograph before it is two cells wide,
    // which is exactly the case an ASCII test cannot reach.
    let mut editor = editor_with("漢ab x");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "漢a");
    assert_eq!(editor.search_match_count(), 1);

    let screen = render(&editor, &overlay, 60, 8);
    let plain = text_background(&screen, 0, 3);
    assert_ne!(
        text_background(&screen, 0, 2),
        plain,
        "the `a` is the match's last cell and must be marked"
    );
    assert_eq!(
        text_background(&screen, 0, 4),
        plain,
        "and the space after `b` must not be"
    );
}

#[test]
fn matches_on_lines_below_the_first_are_marked_too() {
    let mut editor = editor_with("foo\nbar\nfoo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 8);
    let plain = text_background(&screen, 1, 0);
    assert_ne!(
        text_background(&screen, 2, 0),
        plain,
        "the match on line 2 must be marked"
    );
}

#[test]
fn closing_the_search_takes_the_marks_with_it() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    let marked = render(&editor, &overlay, 60, 8);
    assert_ne!(
        text_background(&marked, 0, 8),
        text_background(&marked, 0, 4)
    );

    press(&mut overlay, &mut editor, KeyCode::Escape);
    let cleared = render(&editor, &overlay, 60, 8);
    assert_eq!(
        text_background(&cleared, 0, 8),
        text_background(&cleared, 0, 4),
        "a closed search must leave no marks behind"
    );
}

// ==================== Geometry ====================

#[test]
fn the_panel_takes_its_rows_from_the_document() {
    let editor = editor_with("one\ntwo\nthree");
    let overlay = SearchOverlay::new();

    let without = Frame::layout(&editor, 60, 8, Chrome::default());
    assert_eq!(without.text_rows, 7);
    assert_eq!(without.search_rows, None);

    let with = Frame::layout(
        &editor,
        60,
        8,
        Chrome {
            status: crate::frame::Status::default(),
            search: Some(&overlay),
            sidebar_columns: 0,
        },
    );
    assert_eq!(with.text_rows, 5, "two rows go to the panel");
    assert_eq!(with.search_rows, Some((5, 2)));
    assert_eq!(with.status_row, Some(7));
}

#[test]
fn the_kernels_viewport_shrinks_with_the_panel() {
    // A viewport that still claimed the panel's rows would let the kernel
    // scroll a match underneath the panel that found it.
    let mut editor = editor_with("one\ntwo\nthree");
    let overlay = SearchOverlay::new();
    let chrome = Chrome {
        status: crate::frame::Status::default(),
        search: Some(&overlay),
        sidebar_columns: 0,
    };

    Frame::sync_viewport(&mut editor, 60, 8, Chrome::default());
    assert_eq!(editor.state().viewport.visible_lines, 7);

    Frame::sync_viewport(&mut editor, 60, 8, chrome);
    assert_eq!(editor.state().viewport.visible_lines, 5);
}

#[test]
fn the_caret_belongs_to_the_focused_field_while_the_panel_is_open() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 8);
    let (first, _) = screen.layout.search_rows.expect("panel");
    let caret = screen.layout.caret().expect("caret");
    assert_eq!(caret.row, first, "not the document's caret");
    assert_eq!(screen.layout.search_caret, Some(caret));

    press(&mut overlay, &mut editor, KeyCode::Tab);
    let switched = render(&editor, &overlay, 60, 8);
    assert_eq!(
        switched.layout.caret().map(|position| position.row),
        Some(first + 1),
        "the caret must follow the focus to the replacement field"
    );
}

#[test]
fn a_screen_with_room_for_one_panel_row_shows_the_focused_field() {
    // Keystrokes must never go into a field the user cannot see.
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 2);
    assert_eq!(screen.layout.search_rows, Some((0, 1)));
    assert!(
        screen.find_row().starts_with(" Find"),
        "{}",
        screen.find_row()
    );

    press(&mut overlay, &mut editor, KeyCode::Tab);
    let switched = render(&editor, &overlay, 60, 2);
    assert!(
        switched.find_row().starts_with(" Replace"),
        "{}",
        switched.find_row()
    );
}

#[test]
fn a_screen_with_no_room_at_all_still_renders() {
    let mut editor = editor_with("foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 1);
    assert_eq!(screen.layout.search_rows, None);
    assert_eq!(screen.layout.text_rows, 0);
    assert_eq!(screen.layout.caret(), None);
}

#[test]
fn a_narrow_panel_drops_the_toggles_before_the_count() {
    // The count answers the question being asked; the toggles only say how it
    // was asked. Losing the query text to either of them would be worse still.
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let wide = render(&editor, &overlay, 60, 8).find_row();
    assert!(wide.contains("Aa") && wide.contains("1 of 2"), "{wide}");

    // Both widths leave room for the count and not for the toggles beside it,
    // and 36 is the width where the toggles would fit *instead* of the count if
    // the priority between them were the other way round.
    for width in [30, 36] {
        let narrow = render(&editor, &overlay, width, 8).find_row();
        assert!(narrow.contains("1 of 2"), "at {width}: {narrow}");
        assert!(!narrow.contains("Aa"), "at {width}: {narrow}");
        assert!(
            narrow.contains("foo"),
            "the query itself must survive at {width}: {narrow}"
        );
    }
}

#[test]
fn a_panel_too_narrow_for_the_count_keeps_the_query_whole() {
    // The field's floor is what stops the chrome eating the query. Without it
    // the count is drawn and the query scrolls away behind it — the panel then
    // answers a question whose text the user can no longer read.
    let mut editor = editor_with("foobarbaz");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foobarbaz");

    let row = render(&editor, &overlay, 20, 8).find_row();
    assert!(row.contains("foobarbaz"), "{row}");
    assert!(!row.contains("1 of 1"), "{row}");
}

#[test]
fn a_long_engine_message_is_clipped_rather_than_dropped() {
    let mut editor = editor_with("abc");
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    type_text(&mut overlay, &mut editor, "[a-z");

    let row = render(&editor, &overlay, 40, 8).message_row();
    assert!(
        row.contains("Invalid"),
        "the beginning of the complaint is worth more than nothing: {row}"
    );
}

// ==================== What the panel does not bind ====================

#[test]
fn a_key_the_panel_does_not_bind_is_left_to_the_host() {
    let mut editor = editor_with("foo");
    let mut overlay = opened(&mut editor);

    let unbound = [
        (KeyCode::Char('s'), Modifiers::ctrl()),
        (KeyCode::Char('q'), alt()),
        (KeyCode::F1, Modifiers::none()),
        (KeyCode::PageDown, Modifiers::none()),
        (
            KeyCode::Char('a'),
            Modifiers {
                shift: false,
                ctrl: false,
                alt: false,
                meta: true,
                alt_graph: false,
            },
        ),
    ];
    for (code, modifiers) in unbound {
        assert_eq!(
            chord(&mut overlay, &mut editor, code, modifiers),
            SearchOutcome::Ignored,
            "{code:?} with {modifiers:?} must stay the host's"
        );
    }
    assert!(
        overlay.query().is_empty(),
        "and none of them typed anything"
    );
}

#[test]
fn a_shifted_letter_types_the_letter_it_is() {
    let mut editor = editor_with("Foo");
    let mut overlay = opened(&mut editor);
    chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('F'),
        Modifiers::shift(),
    );
    assert_eq!(overlay.query(), "F");
}
