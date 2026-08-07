//! The panel's mode machine, and the one rule it exists to keep.
//!
//! Every test here is key-free, exactly as the module is. The three bindings
//! still waiting on a ruling cannot change any of these answers.

use std::path::PathBuf;

use super::buffer::SourceRow;
use super::mode::{Confirmation, Leaving, Mode};

/// The panel's root, a folder inside it, and two files.
///
/// ⚠️ **Row 0 must be the folder the panel is showing**, and everything else
/// nests under it. A first draft of this fixture had two rows at depth 0 and
/// every planning test came back `Refused` — the planner reads the drawn
/// indentation as the tree, so a second depth-0 row is a sibling of the root
/// with nowhere to live. The failure was in the fixture, not the module, and
/// it is worth a comment because the shape is not obvious from `SourceRow`.
fn source() -> Vec<SourceRow> {
    vec![
        SourceRow {
            depth: 0,
            name: "p".to_owned(),
            path: PathBuf::from("/p"),
            directory: true,
        },
        SourceRow {
            depth: 1,
            name: "src".to_owned(),
            path: PathBuf::from("/p/src"),
            directory: true,
        },
        SourceRow {
            depth: 2,
            name: "main.rs".to_owned(),
            path: PathBuf::from("/p/src/main.rs"),
            directory: false,
        },
        SourceRow {
            depth: 1,
            name: "README.md".to_owned(),
            path: PathBuf::from("/p/README.md"),
            directory: false,
        },
    ]
}

/// A mode in the middle of an edit that renames `main.rs`.
fn editing_with_a_rename() -> Mode {
    let mut mode = Mode::default();
    assert!(mode.begin_edit(&source()), "the fixture must start editing");
    let buffer = mode.buffer_mut().expect("editing holds a buffer");
    buffer.rename(2, "lib.rs");
    mode.note_edit();
    mode
}

#[test]
fn a_panel_starts_in_browse_and_holds_no_buffer() {
    let mode = Mode::default();
    assert_eq!(mode, Mode::Browse);
    assert!(!mode.is_editing());
    assert!(mode.buffer().is_none());
    assert!(mode.plan().is_none());
    assert!(mode.refusals().is_empty());
}

#[test]
fn beginning_an_edit_loads_the_rows_the_panel_was_drawing() {
    let mut mode = Mode::default();
    assert!(mode.begin_edit(&source()));
    assert!(mode.is_editing());
    let buffer = mode.buffer().expect("editing holds a buffer");
    assert_eq!(buffer.rows().len(), 4);
    assert!(!buffer.is_dirty(), "a freshly loaded buffer holds no edits");
}

/// ⭐ The reason `begin_edit` reports whether it did anything.
///
/// A second press of whatever key enters edit mode must not quietly reload the
/// buffer from disk and take the work with it. This is the test that would
/// fail if `begin_edit` were ever simplified to an unconditional assignment.
#[test]
fn asking_to_edit_again_does_not_reload_over_the_work() {
    let mut mode = editing_with_a_rename();
    let before = mode.buffer().cloned().expect("editing holds a buffer");

    assert!(
        !mode.begin_edit(&source()),
        "a second begin_edit must report that it changed nothing"
    );

    let after = mode.buffer().expect("still editing");
    assert_eq!(
        &before, after,
        "the buffer must survive a second begin_edit untouched"
    );
    assert!(after.is_dirty(), "and must still hold the rename");
}

#[test]
fn a_clean_edit_session_leaves_without_argument() {
    let mut mode = Mode::default();
    mode.begin_edit(&source());
    assert_eq!(mode.leave(), Leaving::Left);
    assert_eq!(mode, Mode::Browse);
}

/// ⚠️ The module's one rule.
#[test]
fn a_dirty_buffer_refuses_to_be_left_and_keeps_everything() {
    let mut mode = editing_with_a_rename();

    assert_eq!(
        mode.leave(),
        Leaving::Unsaved,
        "leaving with unapplied edits must be refused"
    );
    assert!(mode.is_editing(), "and must not change the mode");
    assert!(
        mode.buffer().is_some_and(super::buffer::Buffer::is_dirty),
        "and must not touch the buffer"
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
fn discarding_is_the_only_thing_that_throws_work_away() {
    let mut mode = editing_with_a_rename();
    mode.discard();
    assert_eq!(mode, Mode::Browse);
    assert!(mode.buffer().is_none());
}

/// Leaving from `Browse` is not an error, so no caller has to check first.
#[test]
fn leaving_when_not_editing_is_a_no_op_rather_than_a_refusal() {
    let mut mode = Mode::Browse;
    assert_eq!(mode.leave(), Leaving::Left);
    assert_eq!(mode, Mode::Browse);
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
        mode.buffer().is_some(),
        "and the buffer must survive so going back returns to the edits"
    );
}

/// An unchanged buffer has nothing to confirm, and says so rather than showing
/// an empty confirmation somebody has to dismiss.
#[test]
fn confirming_an_untouched_buffer_reports_nothing_to_do() {
    let mut mode = Mode::default();
    mode.begin_edit(&source());
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
    assert_eq!(mode, Mode::Browse);
}

/// A refused plan stays in `Edit` and keeps the reasons where the panel can
/// draw them.
#[test]
fn a_refused_plan_holds_its_reasons_and_does_not_advance() {
    let mut mode = Mode::default();
    mode.begin_edit(&source());
    // `README.md` given the name the sibling folder already has — a collision
    // under one parent, which the planner refuses rather than resolves.
    let buffer = mode.buffer_mut().expect("editing holds a buffer");
    buffer.rename(3, "src");
    mode.note_edit();

    assert_eq!(mode.request_confirm(), Confirmation::Refused);
    assert!(
        matches!(mode, Mode::Edit(_)),
        "a refusal must not advance the mode"
    );
    assert!(
        !mode.refusals().is_empty(),
        "and must leave something to show the user"
    );
}

/// ⭐ A refusal names a problem in the rows; editing the rows removes the
/// claim that the problem is still there.
#[test]
fn editing_after_a_refusal_clears_it() {
    let mut mode = Mode::default();
    mode.begin_edit(&source());
    let buffer = mode.buffer_mut().expect("editing holds a buffer");
    buffer.rename(3, "src");
    mode.note_edit();
    assert_eq!(mode.request_confirm(), Confirmation::Refused);
    assert!(!mode.refusals().is_empty(), "the premise");

    let buffer = mode.buffer_mut().expect("still editing");
    buffer.rename(3, "NOTES.md");
    mode.note_edit();

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
    let buffer = mode.buffer().expect("editing again");
    assert!(
        buffer.is_dirty(),
        "the rename must survive the trip through the confirmation"
    );
    assert_eq!(buffer.rows()[2].name, "lib.rs");
}

#[test]
fn going_back_from_anywhere_else_reports_that_there_was_nothing_to_go_back_from() {
    let mut mode = Mode::Browse;
    assert!(!mode.back_to_edit());
    let mut mode = Mode::default();
    mode.begin_edit(&source());
    assert!(!mode.back_to_edit());
    assert!(matches!(mode, Mode::Edit(_)), "and changes nothing");
}

/// ⚠️ The confirmation screen states what *will* happen, so the buffer behind
/// it is not editable — otherwise the displayed plan would describe something
/// the apply no longer does.
#[test]
fn the_buffer_cannot_be_edited_from_behind_a_confirmation() {
    let mut mode = editing_with_a_rename();
    assert_eq!(mode.request_confirm(), Confirmation::Ready);

    assert!(
        mode.buffer_mut().is_none(),
        "confirming must not hand out an editable buffer"
    );
    assert!(
        mode.buffer().is_some(),
        "though it still holds one, for going back"
    );
}

#[test]
fn applying_returns_to_browse_and_drops_the_buffer() {
    let mut mode = editing_with_a_rename();
    assert_eq!(mode.request_confirm(), Confirmation::Ready);
    assert!(mode.applied());
    assert_eq!(mode, Mode::Browse);
    assert!(mode.buffer().is_none());
}

/// ⭐ `applied` and `discard` do the same thing to the mode and must not be
/// interchangeable: one records that the work happened, the other that it was
/// thrown away. Refusing to run from outside a confirmation is what stops a
/// caller recording an apply for a plan nobody confirmed.
#[test]
fn an_apply_cannot_be_recorded_for_a_plan_that_was_never_confirmed() {
    let mut mode = editing_with_a_rename();
    assert!(
        !mode.applied(),
        "editing is not a state an apply can have happened in"
    );
    assert!(mode.is_editing(), "and the mode is untouched");

    let mut mode = Mode::Browse;
    assert!(!mode.applied());
}

/// The whole path, once, in order — the sequence a user actually walks.
#[test]
fn the_full_round_trip_ends_where_it_started() {
    let mut mode = Mode::default();
    assert!(mode.begin_edit(&source()));

    let buffer = mode.buffer_mut().expect("editing");
    buffer.rename(2, "lib.rs");
    mode.note_edit();

    assert_eq!(mode.leave(), Leaving::Unsaved, "dirty, so it holds");
    assert_eq!(mode.request_confirm(), Confirmation::Ready);
    assert!(mode.back_to_edit(), "have another look");
    assert_eq!(mode.request_confirm(), Confirmation::Ready);
    assert!(mode.applied());

    assert_eq!(mode, Mode::Browse);
    assert!(mode.buffer().is_none());
    assert!(mode.refusals().is_empty());
}
