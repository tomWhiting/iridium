//! The keys that edit the panel's rows, against real directories.
//!
//! [`super::mode_tests`] proves the state machine without a keyboard and
//! [`super::plan_tests`] the operations without a disk. This is the join: that
//! a key press reaches the row the user is looking at, that the four ruled
//! bindings do what they were ruled to do, and that the keys which would drop
//! a buffer full of work no longer can.
//!
//! Real reader thread throughout, like the panel's own suite. The thing most
//! worth catching here is a key going to the wrong place, and the wrong place
//! — the filter — is a live part of the panel that a mock would have removed.

use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_file::test_support::TempDir;

use super::mode::Mode;
use super::tests::support::{
    FIT, all_lines, chord, lines, meta, opened, press, select_row, settle,
};
use crate::explorer::{ExplorerOutcome, FileExplorer};

/// A key press under `Ctrl` alone.
fn ctrl(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    }
}

/// The same key press as the keyboard's auto-repeat sends it.
fn held(event: &KeyEvent) -> KeyEvent {
    KeyEvent {
        is_repeat: true,
        ..event.clone()
    }
}

/// A project whose root holds one open-able folder and one file, which is the
/// smallest shape that can tell "inside the folder" from "beside it" apart.
fn fixture(name: &str) -> TempDir {
    let directory = TempDir::new(name);
    std::fs::create_dir(directory.path().join("engine")).expect("the fixture directory was made");
    std::fs::write(directory.path().join("engine/mod.rs"), "m").expect("the fixture was written");
    std::fs::write(directory.path().join("README.md"), "#").expect("the fixture was written");
    directory
}

/// Selects the row containing `needle` and starts editing on it.
fn editing(explorer: &mut FileExplorer, needle: &str) {
    select_row(explorer, needle);
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Tab)),
        ExplorerOutcome::Handled,
        "tab must be accepted on a panel that has finished loading"
    );
    assert!(explorer.mode.is_editing(), "tab must start an edit session");
}

/// Types `text` into whatever the panel is taking characters for.
fn type_text(explorer: &mut FileExplorer, text: &str) {
    for character in text.chars() {
        explorer.handle_key(&press(KeyCode::Char(character)));
    }
}

/// Presses one key `count` times.
fn repeat(explorer: &mut FileExplorer, key: KeyCode, count: usize) {
    for _ in 0..count {
        explorer.handle_key(&press(key));
    }
}

/// The list row containing `needle`, or a panic naming what was on screen.
fn row_with(explorer: &mut FileExplorer, needle: &str) -> String {
    let rows = lines(explorer);
    rows.iter()
        .find(|row| row.contains(needle))
        .cloned()
        .unwrap_or_else(|| panic!("no row contains {needle:?}: {rows:?}"))
}

/// What `Failed` said, or a panic naming the outcome that came instead.
fn failure(outcome: &ExplorerOutcome) -> String {
    match outcome {
        ExplorerOutcome::Failed(message) => message.clone(),
        other => panic!("expected a refusal with something to say, got {other:?}"),
    }
}

// ------------------------------------------------------------ entering

#[test]
fn tab_replaces_the_query_field_with_a_label_and_draws_the_text() {
    let directory = fixture("edit-enter");
    let mut explorer = opened(&directory);

    let browsing = all_lines(&mut explorer);
    assert!(
        browsing[0].starts_with("> "),
        "the premise: browsing draws the query field: {browsing:?}"
    );

    editing(&mut explorer, "README.md");
    let rows = all_lines(&mut explorer);
    assert!(
        rows[0].starts_with("edit"),
        "the field must be replaced by something that says the keys have changed \
         meaning: {rows:?}"
    );
    assert!(
        rows[0].contains("ctrl+s to apply"),
        "and must say how to apply, with a chord every terminal can send: {rows:?}"
    );
    assert_eq!(
        rows[1..],
        ["  engine/", "  README.md"],
        "the rows are lines of text, folders marked, the root not among them"
    );
}

/// ⭐ The whole reason the edit branch is taken *before* the browse table.
///
/// `(Plain, Char(_))` in [`super::keys`] swallows every printable character
/// into the filter. In an editing session those characters are a filename.
#[test]
fn a_printable_character_reaches_the_row_rather_than_the_filter() {
    let directory = fixture("edit-typing");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");

    type_text(&mut explorer, "xy");

    assert!(
        row_with(&mut explorer, "README.mdxy").contains("README.mdxy"),
        "the characters must land in the row under the cursor"
    );
    assert_eq!(
        explorer.query.text(),
        "",
        "and must not have reached the filter, which is the row source the \
         buffer is a snapshot of"
    );
}

#[test]
fn backspace_takes_a_character_out_of_the_row_not_the_query() {
    let directory = fixture("edit-backspace");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");

    explorer.handle_key(&press(KeyCode::Backspace));

    assert!(row_with(&mut explorer, "README.").contains("README.m"));
    assert_eq!(explorer.query.text(), "");
}

/// A query that matched nothing produces no rows at all, and a buffer with no
/// rows refuses everything typed into it — the first row has to be the folder
/// the panel is showing.
#[test]
fn tab_is_refused_when_there_is_nothing_on_screen_to_edit() {
    let directory = fixture("edit-empty");
    let mut explorer = opened(&directory);
    type_text(&mut explorer, "zzzznothing");
    settle(&mut explorer);

    let outcome = explorer.handle_key(&press(KeyCode::Tab));
    assert!(
        failure(&outcome).contains("no rows"),
        "the refusal must say why: {outcome:?}"
    );
    assert!(!explorer.mode.is_editing(), "and must not start a session");
}

/// ⭐ A second press of the edit key must not replace a buffer full of work
/// with the rows as they are on disk.
#[test]
fn a_second_tab_does_not_reload_over_the_work() {
    let directory = fixture("edit-reenter");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");

    explorer.handle_key(&press(KeyCode::Tab));

    assert!(
        explorer.has_unapplied_edits(),
        "the edit must survive a second tab"
    );
    assert!(row_with(&mut explorer, "README.md!").contains("README.md!"));
}

// ------------------------------------------------------------ Ctrl+D

#[test]
fn emptying_a_line_marks_its_file_for_deletion_and_says_so() {
    let directory = fixture("edit-delete-line");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");

    repeat(&mut explorer, KeyCode::Backspace, "README.md".len());
    let rows = all_lines(&mut explorer);
    assert!(
        rows[0].contains("1 to delete"),
        "a file with no line left is counted on the label, in words: {rows:?}"
    );
    assert!(explorer.has_unapplied_edits());

    // Undo is the kernel's, so the line comes back as the file it was.
    explorer.handle_key(&ctrl(KeyCode::Char('z')));
    assert!(
        !explorer.has_unapplied_edits(),
        "undoing the deletion leaves the text as it loaded"
    );
}

/// The folder the panel shows is the parent of every line, not a line, so no
/// edit to the text can rename or delete it.
#[test]
fn the_folder_the_panel_is_showing_is_not_a_line_of_the_text() {
    let directory = fixture("edit-root");
    let mut explorer = opened(&directory);
    // The panel opens with the root selected; editing starts on the first line.
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Tab)),
        ExplorerOutcome::Handled
    );
    let root = directory
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .expect("the fixture has a name")
        .to_owned();
    let text = explorer
        .buffer_editor()
        .map(iridium_editor::Editor::content)
        .expect("editing");
    assert!(!text.contains(&root), "the root is a line: {text:?}");
    assert_eq!(text, "engine/\nREADME.md");
}

/// The kernel's multi-cursor keys work on the text as they do in a document:
/// `Ctrl+D` adds the next match, and typing goes to every caret.
#[test]
fn ctrl_d_adds_a_caret_at_the_next_match_and_typing_reaches_both() {
    let directory = fixture("edit-multi");
    std::fs::write(directory.path().join("engine.md"), "e").expect("the fixture was written");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "engine/");

    // Select the word `engine` on the first line, then add its next match.
    explorer.handle_key(&press(KeyCode::Home));
    explorer.handle_key(&chord(
        KeyCode::End,
        Modifiers {
            shift: true,
            ..Modifiers::none()
        },
    ));
    explorer.handle_key(&chord(
        KeyCode::Left,
        Modifiers {
            shift: true,
            ..Modifiers::none()
        },
    ));
    explorer.handle_key(&ctrl(KeyCode::Char('d')));
    type_text(&mut explorer, "motor");

    let text = explorer
        .buffer_editor()
        .map(iridium_editor::Editor::content)
        .expect("editing");
    assert_eq!(text, "motor/\nmotor.md\nREADME.md", "both carets typed");

    // Escape drops the extra caret first and stays in the text.
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Escape)),
        ExplorerOutcome::Handled
    );
    assert!(
        explorer.mode.is_editing(),
        "one escape only collapsed the carets"
    );
}

// ------------------------------------------------------------ Ctrl+Enter

/// A new line keeps the indentation of the line it was opened from, so a line
/// opened after a top-level folder is beside it.
#[test]
fn enter_opens_a_line_beside_a_folder_at_its_indentation() {
    let directory = fixture("edit-new-beside");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "engine");

    assert_eq!(
        explorer.handle_key(&press(KeyCode::Enter)),
        ExplorerOutcome::Handled
    );
    type_text(&mut explorer, "new.rs");

    assert_eq!(row_with(&mut explorer, "new.rs"), "+ new.rs");
    let plan = explorer
        .mode
        .listing()
        .expect("editing")
        .plan()
        .expect("a typed file plans");
    assert_eq!(plan.operations.len(), 1);
    assert_eq!(
        plan.operations[0].describe(),
        format!("create {}", directory.path().join("new.rs").display())
    );
}

/// Tab nests the new line one level under the folder above it.
#[test]
fn tab_on_a_new_line_puts_it_inside_the_folder_above() {
    let directory = fixture("edit-new-inside");
    let mut explorer = opened(&directory);
    select_row(&mut explorer, "engine");
    explorer.handle_key(&press(KeyCode::Right));
    settle(&mut explorer);
    editing(&mut explorer, "engine");

    explorer.handle_key(&press(KeyCode::Enter));
    explorer.handle_key(&press(KeyCode::Tab));
    type_text(&mut explorer, "new.rs");

    assert_eq!(row_with(&mut explorer, "new.rs"), "+   new.rs");
    let plan = explorer
        .mode
        .listing()
        .expect("editing")
        .plan()
        .expect("a typed file plans");
    assert_eq!(
        plan.operations[0].describe(),
        format!(
            "create {}",
            directory.path().join("engine").join("new.rs").display()
        )
    );
}

#[test]
fn a_typed_line_deleted_again_leaves_the_text_as_it_loaded() {
    let directory = fixture("edit-new-undo");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    let before = lines(&mut explorer).len();

    explorer.handle_key(&press(KeyCode::Enter));
    type_text(&mut explorer, "ab");
    assert_eq!(lines(&mut explorer).len(), before + 1);

    repeat(&mut explorer, KeyCode::Backspace, 3);

    assert_eq!(lines(&mut explorer).len(), before);
    assert!(
        !explorer.has_unapplied_edits(),
        "taking a typed line back leaves the text indistinguishable from the load"
    );
}

/// Joining a line onto a folder's makes one name with a separator in it,
/// which is refused: a move between folders is not something typing does.
#[test]
fn joining_a_line_onto_a_folders_is_refused_as_a_path() {
    let directory = fixture("edit-join");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    explorer.handle_key(&press(KeyCode::Home));
    explorer.handle_key(&press(KeyCode::Backspace));

    let text = explorer
        .buffer_editor()
        .map(iridium_editor::Editor::content)
        .expect("editing");
    assert_eq!(text, "engine/README.md");
    let refusals = explorer
        .mode
        .listing()
        .expect("editing")
        .plan()
        .expect_err("a name with a separator in it is refused");
    assert!(
        refusals
            .iter()
            .any(|refusal| refusal.reason.contains("path rather than a name")),
        "{refusals:?}"
    );
}

// ------------------------------------------------------------ ⌘S and apply

/// ⛔ `⌘S` reaches the confirmation and stops. Two keys, because the gap
/// between them is the whole of ruling 7.
#[test]
fn cmd_s_reaches_the_confirmation_and_writes_nothing() {
    let directory = fixture("edit-confirm");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");

    assert_eq!(
        explorer.handle_key(&meta(KeyCode::Char('s'))),
        ExplorerOutcome::Handled
    );
    assert!(matches!(explorer.mode, Mode::Confirm(_)));

    assert!(
        directory.path().join("README.md").exists(),
        "⌘S must not touch the disk"
    );
    assert!(!directory.path().join("README.md!").exists());

    let rows = all_lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("rename")),
        "the confirmation lists the operations by name: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("y to apply")),
        "and says how to answer: {rows:?}"
    );
}

/// ⚠️ The confirmation replaces the row list outright, field and all. `y`
/// applies and `y` is a plain character; a query field left on screen would be
/// an invitation to type one.
#[test]
fn the_confirmation_leaves_nothing_on_screen_to_type_into() {
    let directory = fixture("edit-confirm-modal");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");
    explorer.handle_key(&meta(KeyCode::Char('s')));

    let rows = all_lines(&mut explorer);
    assert!(
        !rows[0].starts_with("> "),
        "no query field on the confirmation: {rows:?}"
    );
    assert!(
        explorer.body(&Theme::dark(), FIT).caret.is_none(),
        "and no caret, because there is nothing here to edit"
    );

    // A stray character must not reach a row, and must not answer anything.
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Char('q'))),
        ExplorerOutcome::Handled
    );
    assert!(matches!(explorer.mode, Mode::Confirm(_)));
    assert_eq!(explorer.query.text(), "");
}

#[test]
fn y_on_the_confirmation_carries_the_plan_out_and_returns_to_browsing() {
    let directory = fixture("edit-apply");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    repeat(&mut explorer, KeyCode::Backspace, "README.md".len());
    type_text(&mut explorer, "NOTES.md");
    explorer.handle_key(&meta(KeyCode::Char('s')));

    assert_eq!(
        explorer.handle_key(&press(KeyCode::Char('y'))),
        ExplorerOutcome::Handled
    );

    assert!(directory.path().join("NOTES.md").exists(), "the rename ran");
    assert!(!directory.path().join("README.md").exists());
    assert!(
        matches!(explorer.mode, Mode::Browse),
        "and the session is over"
    );
    assert!(!explorer.has_unapplied_edits());

    // ⚠️ The rows must be re-read, or a second session would carry origins
    // naming a file that has moved.
    settle(&mut explorer);
    let rows = lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("NOTES.md")),
        "the panel re-reads what it changed: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("README.md")),
        "{rows:?}"
    );
}

/// A held `y` must not apply twice, and `⌘S` on the confirmation must be
/// inert — a held apply key that confirmed and then applied would answer a
/// confirmation by an accident of timing.
#[test]
fn the_confirmation_cannot_be_answered_by_a_held_key() {
    let directory = fixture("edit-apply-held");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");
    explorer.handle_key(&meta(KeyCode::Char('s')));

    assert_eq!(
        explorer.handle_key(&held(&meta(KeyCode::Char('s')))),
        ExplorerOutcome::Handled
    );
    assert_eq!(
        explorer.handle_key(&meta(KeyCode::Char('s'))),
        ExplorerOutcome::Handled
    );
    assert!(
        matches!(explorer.mode, Mode::Confirm(_)),
        "⌘S is what got here; it cannot also be what leaves"
    );
    assert!(directory.path().join("README.md").exists());

    assert_eq!(
        explorer.handle_key(&held(&press(KeyCode::Char('y')))),
        ExplorerOutcome::Handled
    );
    assert!(
        matches!(explorer.mode, Mode::Confirm(_)),
        "a repeat must not apply"
    );
    assert!(directory.path().join("README.md").exists());
}

#[test]
fn n_on_the_confirmation_goes_back_to_the_edits() {
    let directory = fixture("edit-back");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");
    explorer.handle_key(&meta(KeyCode::Char('s')));

    assert_eq!(
        explorer.handle_key(&press(KeyCode::Char('n'))),
        ExplorerOutcome::Handled
    );

    assert!(matches!(explorer.mode, Mode::Edit(_)));
    assert!(
        row_with(&mut explorer, "README.md!").contains("README.md!"),
        "going back returns to the edits, not to the rows as they loaded"
    );
    assert!(directory.path().join("README.md").exists());
}

#[test]
fn cmd_s_on_an_untouched_buffer_says_there_is_nothing_to_apply() {
    let directory = fixture("edit-nothing");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");

    let outcome = explorer.handle_key(&meta(KeyCode::Char('s')));
    assert!(
        failure(&outcome).contains("nothing to apply"),
        "{outcome:?}"
    );
    assert!(
        matches!(explorer.mode, Mode::Edit(_)),
        "an empty confirmation costs a keystroke to dismiss and says nothing"
    );
}

#[test]
fn a_refused_plan_shows_its_reasons_and_esc_returns_to_the_text() {
    let directory = fixture("edit-refused");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "/x");

    assert_eq!(
        explorer.handle_key(&meta(KeyCode::Char('s'))),
        ExplorerOutcome::Handled
    );
    let rows = all_lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("cannot be applied")),
        "the refusals replace the text: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("is a path")),
        "and name what is wrong: {rows:?}"
    );

    // Everything but escape is swallowed while they are showing: the text is
    // not on screen, so there is nothing for a character to land in.
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Char('q'))),
        ExplorerOutcome::Handled
    );
    assert!(
        all_lines(&mut explorer)
            .iter()
            .any(|row| row.contains("cannot be applied"))
    );

    assert_eq!(
        explorer.handle_key(&press(KeyCode::Escape)),
        ExplorerOutcome::Handled
    );
    assert!(
        all_lines(&mut explorer)[0].starts_with("edit"),
        "esc goes back to the text that needs fixing, not out of the session"
    );
    assert!(explorer.has_unapplied_edits(), "and keeps the work");
    let caret = explorer
        .buffer_editor()
        .map(iridium_editor::Editor::cursor)
        .expect("editing");
    assert_eq!(caret.line, 1, "the caret is on the refused line");
}

// ------------------------------------------------------------ leaving

/// ⚠️ The module's one rule, answered. A dirty buffer is never dropped without
/// being asked about; the first escape asks and the second answers.
#[test]
fn escape_asks_before_it_throws_unapplied_edits_away() {
    let directory = fixture("edit-escape");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");

    let outcome = explorer.handle_key(&press(KeyCode::Escape));
    assert!(failure(&outcome).contains("esc again"), "{outcome:?}");
    assert!(
        explorer.mode.is_editing(),
        "the first press changes nothing"
    );
    assert!(explorer.has_unapplied_edits());

    assert_eq!(
        explorer.handle_key(&press(KeyCode::Escape)),
        ExplorerOutcome::Handled
    );
    assert!(matches!(explorer.mode, Mode::Browse));
    assert!(!explorer.has_unapplied_edits());
}

/// The question does not stay open. A key in between means the user carried
/// on, and an escape a minute later must ask again rather than answer a
/// question they have forgotten about.
#[test]
fn a_key_in_between_makes_the_next_escape_ask_again() {
    let directory = fixture("edit-escape-rearm");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");

    let first = explorer.handle_key(&press(KeyCode::Escape));
    assert!(failure(&first).contains("esc again"));
    explorer.handle_key(&press(KeyCode::Down));

    let second = explorer.handle_key(&press(KeyCode::Escape));
    assert!(failure(&second).contains("esc again"), "{second:?}");
    assert!(explorer.mode.is_editing(), "and the work is still there");
}

#[test]
fn escape_on_a_clean_buffer_goes_back_to_browsing_without_asking() {
    let directory = fixture("edit-escape-clean");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");

    assert_eq!(
        explorer.handle_key(&press(KeyCode::Escape)),
        ExplorerOutcome::Handled
    );
    assert!(matches!(explorer.mode, Mode::Browse));
    assert!(
        all_lines(&mut explorer)[0].starts_with("> "),
        "and the query field is back"
    );
}

#[test]
fn the_close_chord_refuses_while_there_are_unapplied_edits() {
    let directory = fixture("edit-close");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");

    let toggle = KeyEvent {
        key: KeyCode::Char('e'),
        modifiers: Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        },
        is_repeat: false,
    };
    let outcome = explorer.handle_key(&toggle);
    assert!(failure(&outcome).contains("unapplied edits"), "{outcome:?}");
    assert!(explorer.mode.is_editing());

    // Clean, and the same chord closes as it always did.
    explorer.handle_key(&press(KeyCode::Backspace));
    assert_eq!(explorer.handle_key(&toggle), ExplorerOutcome::Closed);
}

/// ⛔ `Enter` in the browse table returns `Open`, and the host answers that by
/// dropping the panel — which would take the buffer with it and no keystroke
/// would have meant it.
#[test]
fn enter_cannot_open_a_file_from_inside_a_buffer() {
    let directory = fixture("edit-enter-file");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");

    assert_eq!(
        explorer.handle_key(&press(KeyCode::Enter)),
        ExplorerOutcome::Handled,
        "Enter must not name a file for the host to open while a buffer is up"
    );
    assert!(explorer.mode.is_editing());
    assert!(explorer.has_unapplied_edits());
}

/// Re-rooting replaces the whole panel, mode and all — a silent drop of
/// everything typed.
#[test]
fn the_rerooting_pair_is_swallowed_while_editing() {
    let directory = fixture("edit-reroot");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");
    type_text(&mut explorer, "!");

    for key in [KeyCode::Up, KeyCode::Down] {
        assert_eq!(explorer.handle_key(&meta(key)), ExplorerOutcome::Handled);
    }

    assert!(explorer.mode.is_editing(), "the session survives");
    assert!(explorer.has_unapplied_edits(), "and so does the work");
    assert_eq!(
        explorer
            .files
            .info(explorer.files.root())
            .map(|info| info.path),
        Some(directory.path()),
        "…and the panel is still rooted where it was"
    );
}

// ------------------------------------------------------------ the rest

/// The host intercepts the paste chord before `handle_key` ever sees it, so
/// the pre-match branch cannot cover this path and `paste` has to know the
/// mode itself.
#[test]
fn pasting_reaches_the_row_and_leaves_the_query_alone() {
    let directory = fixture("edit-paste");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "README.md");

    explorer.paste("-copy");

    assert!(row_with(&mut explorer, "README.md-copy").contains("README.md-copy"));
    assert_eq!(
        explorer.query.text(),
        "",
        "a paste into the filter would rebuild the row source under a live buffer"
    );
}

#[test]
fn the_arrows_move_the_caret_as_they_do_in_a_document() {
    let directory = fixture("edit-motion");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "engine");

    // Down keeps the column, as it does in a document; End then reaches the
    // end of the name.
    explorer.handle_key(&press(KeyCode::Down));
    explorer.handle_key(&press(KeyCode::End));
    type_text(&mut explorer, "!");
    assert!(row_with(&mut explorer, "README.md!").contains("README.md!"));

    explorer.handle_key(&press(KeyCode::Left));
    explorer.handle_key(&press(KeyCode::Left));
    type_text(&mut explorer, "-");
    assert!(row_with(&mut explorer, "README.m-d!").contains("README.m-d!"));

    explorer.handle_key(&press(KeyCode::Home));
    type_text(&mut explorer, "z");
    assert!(row_with(&mut explorer, "zREADME").contains("zREADME.m-d!"));
}

#[test]
fn the_caret_sits_at_the_end_of_the_line_editing_started_on() {
    let directory = fixture("edit-caret");
    let mut explorer = opened(&directory);
    editing(&mut explorer, "engine");

    let caret = explorer
        .body(&Theme::dark(), FIT)
        .caret
        .expect("an editing session draws a caret");
    assert_eq!(caret.row, 1, "the label, then `engine/`");
    assert_eq!(
        caret.column,
        "  engine/".chars().count(),
        "past the marker column and the whole line"
    );
}

/// The host reaches straight for the `Option` holding the panel on a click
/// outside it and on a second press of the toggle command, and never asks the
/// panel anything. This is what it asks now.
#[test]
fn the_panel_reports_unapplied_edits_so_the_host_can_refuse_to_drop_it() {
    let directory = fixture("edit-unapplied");
    let mut explorer = opened(&directory);
    assert!(!explorer.has_unapplied_edits(), "browsing holds no work");

    editing(&mut explorer, "README.md");
    assert!(
        !explorer.has_unapplied_edits(),
        "nor does a session nobody has typed into"
    );

    type_text(&mut explorer, "!");
    assert!(explorer.has_unapplied_edits());

    explorer.handle_key(&press(KeyCode::Backspace));
    assert!(
        !explorer.has_unapplied_edits(),
        "dirtiness is a comparison against what loaded, not a flag"
    );
}

/// ⭐ **The user's `[keys]` reach the text.** A binding to a kernel command
/// runs in the listing exactly as it does in a document.
#[test]
fn a_user_binding_to_a_kernel_command_runs_in_the_text() {
    let directory = fixture("edit-user-binding");
    let (first, rest) = iridium_editor::KeyBinding::parse_sequence("ctrl+j")
        .expect("the fixture is a key sequence");
    let mut layer = iridium_editor::Keymap::new("user");
    layer.push(iridium_editor::KeyBinding::new(
        first,
        &rest,
        iridium_editor::commands::builtin::EDIT_INSERT_NEWLINE,
    ));

    let mut explorer = opened(&directory);
    explorer.set_user_keymap(&layer);
    editing(&mut explorer, "README.md");
    explorer.handle_key(&ctrl(KeyCode::Char('j')));

    let text = explorer
        .buffer_editor()
        .map(iridium_editor::Editor::content)
        .expect("editing");
    assert_eq!(
        text, "engine/\nREADME.md\n",
        "the user's chord opened a line"
    );
}

/// A face's own command — saving a file — means nothing in a listing, so the
/// face's layer reaches the text without it rather than being refused whole.
#[test]
fn a_faces_layer_reaches_the_text_without_its_own_commands() {
    let directory = fixture("edit-face-binding");
    let (first, rest) =
        iridium_editor::KeyBinding::parse_sequence("ctrl+j").expect("a key sequence");
    let (save, save_rest) =
        iridium_editor::KeyBinding::parse_sequence("ctrl+s").expect("a key sequence");
    let mut layer = iridium_editor::Keymap::new("face");
    layer.push(iridium_editor::KeyBinding::new(
        first,
        &rest,
        iridium_editor::commands::builtin::EDIT_INSERT_NEWLINE,
    ));
    layer.push(iridium_editor::KeyBinding::new(
        save,
        &save_rest,
        iridium_editor::CommandId::from_static("face.file.save"),
    ));

    let mut explorer = opened(&directory);
    explorer.set_face_keymap(&layer);
    editing(&mut explorer, "README.md");
    explorer.handle_key(&ctrl(KeyCode::Char('j')));
    type_text(&mut explorer, "x");
    assert_eq!(
        explorer.handle_key(&ctrl(KeyCode::Char('s'))),
        ExplorerOutcome::Handled
    );
    assert!(
        matches!(explorer.mode, Mode::Confirm(_)),
        "ctrl+s still asks to apply rather than naming the face's save"
    );
}
