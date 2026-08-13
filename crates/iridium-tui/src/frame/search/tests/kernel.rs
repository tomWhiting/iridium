//! What the overlay asks the kernel for: the search itself, the option
//! toggles, and a pattern that will not compile.
//!
//! Split out of a 1,136-line `tests.rs` for #92, on the rules the file
//! already carried. The fixtures and the [`Screen`](super::Screen) reader are
//! in [`super`].

use super::*;

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
