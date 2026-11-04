//! The panel's mode machine, and the one rule it exists to keep.
//!
//! Key-free, exactly as the module is: edits are made to the session's editor
//! directly and followed with [`Editing::sync`], which is what a key does.

use std::path::PathBuf;

use iridium_editor::commands::builtin::{
    LINES_DELETE, LINES_DUPLICATE_DOWN, LINES_JOIN, LINES_MOVE_DOWN, LINES_MOVE_UP,
    MULTI_CURSOR_ADD_CURSOR_BELOW, SELECTION_SELECT_ALL,
};
use iridium_editor::{CommandArgs, KeyCode, KeyEvent, Modifiers};

use super::buffer::SourceRow;
use super::mode::{Confirmation, Leaving, Mode};
use super::session::{EditLayers, Editing};

/// The panel's root, a folder inside it, and two files.
///
/// ⚠️ **Row 0 must be the folder the panel is showing**, and everything else
/// nests under it: the listing is the root's contents.
fn source() -> Vec<SourceRow> {
    let row = |depth, name: &str, path: &str, directory| SourceRow {
        depth,
        name: name.to_owned(),
        path: PathBuf::from(path),
        directory,
    };
    vec![
        row(0, "p", "/p", true),
        row(1, "src", "/p/src", true),
        row(2, "main.rs", "/p/src/main.rs", false),
        row(1, "README.md", "/p/README.md", false),
    ]
}

/// The text the fixture loads as.
const LOADED: &str = "src/\n  main.rs\nREADME.md";

/// A session over the fixture.
fn session() -> Editing {
    Editing::new(&source(), 0, &EditLayers::default())
        .expect("the fixture starts at its root")
        .0
}

/// Replaces the whole text being edited, as one edit, and follows it.
fn retype(editing: &mut Editing, text: &str) {
    editing
        .editor
        .run_command(SELECTION_SELECT_ALL.as_str(), CommandArgs::NONE)
        .expect("select all is a kernel command");
    editing.editor.paste(text);
    editing.sync();
}

/// [`retype`] on the session `mode` is editing.
fn retype_in(mode: &mut Mode, text: &str) {
    retype(mode.editing_mut().expect("editing holds a session"), text);
}

/// A mode in the middle of an edit that renames `main.rs`.
fn editing_with_a_rename() -> Mode {
    let mut mode = Mode::default();
    assert!(mode.begin_edit(session()), "the fixture must start editing");
    retype_in(&mut mode, "src/\n  lib.rs\nREADME.md");
    mode
}

/// The text the session holds, wherever the mode keeps it.
fn text(mode: &Mode) -> Option<String> {
    mode.session().map(|editing| editing.editor.content())
}

#[test]
fn a_panel_starts_in_browse_and_holds_no_listing() {
    let mode = Mode::default();
    assert!(matches!(mode, Mode::Browse));
    assert!(!mode.is_editing());
    assert!(mode.listing().is_none());
    assert!(mode.plan().is_none());
    assert!(mode.refusals().is_empty());
}

#[test]
fn beginning_an_edit_loads_the_rows_the_panel_was_drawing_as_text() {
    let mut mode = Mode::default();
    assert!(mode.begin_edit(session()));
    assert!(mode.is_editing());
    assert_eq!(text(&mode).as_deref(), Some(LOADED));
    let listing = mode.listing().expect("editing holds a listing");
    assert_eq!(listing.rows().rows.len(), 4);
    assert!(
        !listing.is_dirty(),
        "a freshly loaded listing holds no edits"
    );
}

/// ⭐ The reason `begin_edit` reports whether it did anything.
///
/// A second press of whatever key enters edit mode must not quietly reload
/// the listing from disk and take the work with it.
#[test]
fn asking_to_edit_again_does_not_reload_over_the_work() {
    let mut mode = editing_with_a_rename();
    assert!(
        !mode.begin_edit(session()),
        "a second begin_edit must report that it changed nothing"
    );
    assert_eq!(text(&mode).as_deref(), Some("src/\n  lib.rs\nREADME.md"));
    assert!(mode.listing().is_some_and(super::buffer::Listing::is_dirty));
}

#[test]
fn a_clean_edit_session_leaves_without_argument() {
    let mut mode = Mode::default();
    mode.begin_edit(session());
    assert_eq!(mode.leave(), Leaving::Left);
    assert!(matches!(mode, Mode::Browse));
}

/// ⚠️ The module's one rule.
#[test]
fn a_dirty_listing_refuses_to_be_left_and_keeps_everything() {
    let mut mode = editing_with_a_rename();
    assert_eq!(mode.leave(), Leaving::Unsaved);
    assert!(mode.is_editing(), "and must not change the mode");
    assert!(
        mode.listing().is_some_and(super::buffer::Listing::is_dirty),
        "and must not touch the listing"
    );
}

/// The refusal is repeatable, not a one-shot that a second press walks past.
#[test]
fn refusing_to_leave_does_not_wear_off() {
    let mut mode = editing_with_a_rename();
    for attempt in 0..3 {
        assert_eq!(
            mode.leave(),
            Leaving::Unsaved,
            "attempt {attempt} must be refused like the first"
        );
    }
}

#[test]
fn undoing_every_edit_makes_the_listing_clean_again() {
    let mut mode = editing_with_a_rename();
    let editing = mode.editing_mut().expect("editing");
    while editing.editor.undo() {}
    editing.sync();
    assert_eq!(text(&mode).as_deref(), Some(LOADED));
    assert_eq!(mode.leave(), Leaving::Left, "nothing left to lose");
}

#[test]
fn discarding_is_the_only_thing_that_throws_work_away() {
    let mut mode = editing_with_a_rename();
    mode.discard();
    assert!(matches!(mode, Mode::Browse));
    assert!(mode.listing().is_none());
}

/// Leaving from `Browse` is not an error, so no caller has to check first.
#[test]
fn leaving_when_not_editing_is_a_no_op_rather_than_a_refusal() {
    let mut mode = Mode::Browse;
    assert_eq!(mode.leave(), Leaving::Left);
    assert!(matches!(mode, Mode::Browse));
}

#[test]
fn confirming_an_edit_moves_to_the_confirmation_and_keeps_the_plan() {
    let mut mode = editing_with_a_rename();
    assert_eq!(mode.request_confirm(), Confirmation::Ready);
    assert!(matches!(mode, Mode::Confirm(_)));
    let plan = mode.plan().expect("a confirmation holds a plan");
    assert!(
        !plan.is_empty(),
        "the rename must have produced an operation"
    );
    assert!(
        mode.listing().is_some(),
        "and the listing must survive so going back returns to the edits"
    );
}

/// An unchanged listing has nothing to confirm, and says so rather than
/// showing an empty confirmation somebody has to dismiss.
#[test]
fn confirming_an_untouched_listing_reports_nothing_to_do() {
    let mut mode = Mode::default();
    mode.begin_edit(session());
    assert_eq!(mode.request_confirm(), Confirmation::NothingToDo);
    assert!(
        matches!(mode, Mode::Edit(_)),
        "and must leave the mode alone"
    );
}

#[test]
fn confirming_while_browsing_reports_that_there_was_nothing_to_confirm() {
    let mut mode = Mode::Browse;
    assert_eq!(mode.request_confirm(), Confirmation::NotEditing);
    assert!(matches!(mode, Mode::Browse));
}

/// A refused plan stays in `Edit` and keeps the reasons where the panel can
/// draw them.
#[test]
fn a_refused_plan_holds_its_reasons_and_does_not_advance() {
    let mut mode = Mode::default();
    mode.begin_edit(session());
    // `README.md` given the name its sibling folder already has.
    retype_in(&mut mode, "src/\n  main.rs\nsrc");

    assert_eq!(mode.request_confirm(), Confirmation::Refused);
    assert!(
        matches!(mode, Mode::Edit(_)),
        "a refusal must not advance the mode"
    );
    assert!(
        !mode.refusals().is_empty(),
        "and must leave something to show"
    );
}

/// ⭐ A refusal names a problem in the text; editing the text removes the
/// claim that the problem is still there.
#[test]
fn editing_after_a_refusal_clears_it() {
    let mut mode = Mode::default();
    mode.begin_edit(session());
    retype_in(&mut mode, "src/\n  main.rs\nsrc");
    assert_eq!(mode.request_confirm(), Confirmation::Refused);
    assert!(!mode.refusals().is_empty(), "the premise");

    retype_in(&mut mode, "src/\n  main.rs\nNOTES.md");
    assert!(
        mode.refusals().is_empty(),
        "a refusal that outlives its cause points at a problem that is gone"
    );
}

#[test]
fn going_back_from_a_confirmation_returns_to_the_edits_not_to_the_load() {
    let mut mode = editing_with_a_rename();
    assert_eq!(mode.request_confirm(), Confirmation::Ready);

    assert!(mode.back_to_edit());
    assert!(matches!(mode, Mode::Edit(_)));
    assert_eq!(text(&mode).as_deref(), Some("src/\n  lib.rs\nREADME.md"));
    assert!(mode.listing().is_some_and(super::buffer::Listing::is_dirty));
}

#[test]
fn going_back_from_anywhere_else_reports_that_there_was_nothing_to_go_back_from() {
    let mut mode = Mode::Browse;
    assert!(!mode.back_to_edit());
    let mut mode = Mode::default();
    mode.begin_edit(session());
    assert!(!mode.back_to_edit());
    assert!(matches!(mode, Mode::Edit(_)), "and changes nothing");
}

/// ⚠️ The confirmation states what *will* happen, so the text behind it is
/// not editable — otherwise the plan shown would describe something the apply
/// no longer does.
#[test]
fn the_text_cannot_be_edited_from_behind_a_confirmation() {
    let mut mode = editing_with_a_rename();
    assert_eq!(mode.request_confirm(), Confirmation::Ready);
    assert!(
        mode.editing_mut().is_none(),
        "no editable session behind a plan"
    );
    assert!(
        mode.listing().is_some(),
        "though it still holds one, for going back"
    );
}

#[test]
fn applying_returns_to_browse_and_drops_the_listing() {
    let mut mode = editing_with_a_rename();
    assert_eq!(mode.request_confirm(), Confirmation::Ready);
    assert!(mode.applied());
    assert!(matches!(mode, Mode::Browse));
    assert!(mode.listing().is_none());
}

/// ⭐ `applied` and `discard` do the same thing to the mode and must not be
/// interchangeable: refusing to run from outside a confirmation is what stops
/// a caller recording an apply for a plan nobody confirmed.
#[test]
fn an_apply_cannot_be_recorded_for_a_plan_that_was_never_confirmed() {
    let mut mode = editing_with_a_rename();
    assert!(
        !mode.applied(),
        "editing is not a state an apply happened in"
    );
    assert!(mode.is_editing(), "and the mode is untouched");

    let mut mode = Mode::Browse;
    assert!(!mode.applied());
}

/// The whole path, once, in order — the sequence a user actually walks.
#[test]
fn the_full_round_trip_ends_where_it_started() {
    let mut mode = Mode::default();
    assert!(mode.begin_edit(session()));
    retype_in(&mut mode, "src/\n  lib.rs\nREADME.md");

    assert_eq!(mode.leave(), Leaving::Unsaved, "dirty, so it holds");
    assert_eq!(mode.request_confirm(), Confirmation::Ready);
    assert!(mode.back_to_edit(), "have another look");
    assert_eq!(mode.request_confirm(), Confirmation::Ready);
    assert!(mode.applied());

    assert!(matches!(mode, Mode::Browse));
    assert!(mode.listing().is_none());
    assert!(mode.refusals().is_empty());
}

#[test]
fn the_caret_starts_at_the_end_of_the_line_it_was_asked_for() {
    let (editing, problems) =
        Editing::new(&source(), 1, &EditLayers::default()).expect("the fixture loads");
    assert!(
        problems.is_empty(),
        "the default layers install cleanly: {problems:?}"
    );
    assert_eq!(editing.caret_line(), 1);
    assert_eq!(editing.editor.cursor().column, "  main.rs".len());
}

#[test]
fn escape_drops_extra_carets_before_anything_else() {
    let mut editing = session();
    assert!(!editing.collapse_carets(), "one caret: nothing to drop");
    editing
        .editor
        .run_command(MULTI_CURSOR_ADD_CURSOR_BELOW.as_str(), CommandArgs::NONE)
        .expect("adding a caret below is a kernel command");
    assert!(
        editing.editor.state().cursor.cursor_count() > 1,
        "the premise"
    );
    assert!(editing.collapse_carets());
    assert_eq!(editing.editor.state().cursor.cursor_count(), 1);
}

/// Eight folders of five files under the root, every name unique.
fn eight_folders() -> Vec<SourceRow> {
    let mut rows = vec![SourceRow {
        depth: 0,
        name: "p".to_owned(),
        path: PathBuf::from("/p"),
        directory: true,
    }];
    for folder in 0..8 {
        let folder_path = format!("/p/dir{folder}");
        rows.push(SourceRow {
            depth: 1,
            name: format!("dir{folder}"),
            path: PathBuf::from(&folder_path),
            directory: true,
        });
        for file in 0..5 {
            rows.push(SourceRow {
                depth: 2,
                name: format!("f{folder}_{file}.rs"),
                path: PathBuf::from(format!("{folder_path}/f{folder}_{file}.rs")),
                directory: false,
            });
        }
    }
    rows
}

/// The keys the comparison below presses at random.
fn random_keys() -> [KeyEvent; 11] {
    let press = |key, modifiers| KeyEvent {
        key,
        modifiers,
        is_repeat: false,
    };
    [
        press(KeyCode::Char('x'), Modifiers::none()),
        press(KeyCode::Char('/'), Modifiers::none()),
        press(KeyCode::Enter, Modifiers::none()),
        press(KeyCode::Backspace, Modifiers::none()),
        press(KeyCode::Tab, Modifiers::none()),
        press(KeyCode::Up, Modifiers::none()),
        press(KeyCode::Down, Modifiers::none()),
        press(KeyCode::Down, Modifiers::shift()),
        press(KeyCode::Home, Modifiers::none()),
        press(KeyCode::End, Modifiers::none()),
        press(KeyCode::Char('z'), Modifiers::ctrl()),
    ]
}

/// Whether two non-blank lines are the same, or two are blank: then which
/// of the two an edit kept depends on where the comparison started, and a
/// span and the whole text can both be right and disagree.
fn has_twins(lines: &[String]) -> bool {
    let mut distinct = std::collections::HashSet::new();
    lines.iter().any(|line| {
        !distinct.insert(if line.trim().is_empty() {
            ""
        } else {
            line.as_str()
        })
    })
}

/// Many keystrokes, line commands, pastes and undos, each followed through
/// the span the document reports, read exactly as the same text reread whole
/// after each one.
#[test]
fn following_the_editor_by_span_matches_rereading_it_whole() {
    let (mut editing, _) = Editing::new(&eight_folders(), 20, &EditLayers::default())
        .expect("the rows start at their root");
    let mut whole = editing.listing.clone();
    let keys = random_keys();
    let commands = [
        LINES_DELETE,
        LINES_MOVE_UP,
        LINES_MOVE_DOWN,
        LINES_DUPLICATE_DOWN,
        LINES_JOIN,
    ];
    let mut seed: u64 = 0x0dd_ba11_cafe_f00d;
    let mut next = |bound: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        usize::try_from(seed % u64::try_from(bound.max(1)).unwrap_or(1)).unwrap_or(0)
    };
    let mut clipboard = String::new();
    let mut compared = 0;
    for step in 0..3_000 {
        let before = editing.editor.state().document.revision();
        match next(20) {
            // A line taken out, to be pasted somewhere else later.
            0 => {
                let line = next(editing.editor.state().document.line_count());
                clipboard = editing
                    .editor
                    .state()
                    .document
                    .line(line)
                    .unwrap_or_default();
                editing.place_caret(line);
                let _ = editing
                    .editor
                    .run_command(LINES_DELETE.as_str(), CommandArgs::NONE);
            },
            1 => {
                let line = next(editing.editor.state().document.line_count());
                editing.place_caret(line);
                editing.editor.paste(&format!("\n{clipboard}"));
            },
            2 => {
                editing.editor.redo();
            },
            3 => {
                let command = &commands[next(commands.len())];
                let _ = editing
                    .editor
                    .run_command(command.as_str(), CommandArgs::NONE);
            },
            _ => {
                editing.editor.handle_key(&keys[next(keys.len())]);
            },
        }
        let document = &editing.editor.state().document;
        assert!(
            document.lines_changed_since(before).is_some(),
            "step {step}: the document must say which lines changed"
        );
        editing.sync();
        let document = &editing.editor.state().document;
        let all: Vec<String> = (0..document.line_count())
            .map(|line| document.line(line).unwrap_or_default())
            .collect();
        assert_eq!(editing.listing.line_count(), all.len(), "step {step}");
        whole.track(all.clone());
        if has_twins(&all) {
            whole = editing.listing.clone();
        } else {
            assert_eq!(editing.listing.rows(), whole.rows(), "step {step}");
            assert_eq!(
                editing.listing.line_states(),
                whole.line_states(),
                "step {step}"
            );
            assert_eq!(editing.listing.is_dirty(), whole.is_dirty(), "step {step}");
            compared += 1;
        }
    }
    assert!(
        compared > 300,
        "only {compared} steps compared with a whole reading"
    );
}
