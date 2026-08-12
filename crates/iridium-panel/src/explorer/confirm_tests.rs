//! Tests for the confirmation.
//!
//! The one that earns its keep is
//! [`a_plan_too_long_for_the_panel_says_how_much_it_is_not_showing`]. Every
//! other test here checks that something reads well; that one checks that a
//! list which *cannot* be complete never looks complete. A confirmation is
//! read once, quickly, and then trusted — so a delete quietly dropped off the
//! bottom is worse than showing no confirmation at all.

use std::path::{Path, PathBuf};

use iridium_editor::theme::Theme;

use super::confirm::{plan_rows, refusal_rows};
use super::plan::{EditedRow, Operation, Plan, Refusal, RowOrigin};
use crate::row::PanelRow;

/// The folder the panel is showing.
const ROOT: &str = "/project";

fn root() -> &'static Path {
    Path::new(ROOT)
}

/// Wide enough that nothing in these fixtures truncates unless a test means
/// it to.
const WIDTH: usize = 60;

/// Room for a comfortable panel: four rows of chrome and eight of list.
const ROOM: usize = 12;

fn rename(from: &str, to: &str) -> Operation {
    Operation::Rename {
        from: PathBuf::from(from),
        to: PathBuf::from(to),
    }
}

fn plan_of(operations: Vec<Operation>) -> Plan {
    Plan { operations }
}

/// The rows as plain text.
fn text(rows: &[PanelRow]) -> Vec<String> {
    rows.iter().map(PanelRow::text).collect()
}

/// The confirmation for a plan, as plain text.
fn shown(plan: &Plan, room: usize) -> Vec<String> {
    text(&plan_rows(plan, root(), &Theme::dark(), WIDTH, room))
}

#[test]
fn an_empty_plan_says_there_is_nothing_to_apply() {
    let rows = shown(&Plan::default(), ROOM);
    assert!(
        rows.iter().any(|row| row.contains("Nothing to apply")),
        "{rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("y to apply")),
        "an empty plan offered to apply itself: {rows:?}"
    );
}

#[test]
fn every_operation_is_listed_by_name_and_not_merely_counted() {
    // Ruling 7. A count is what the summary line is for; the list is the
    // thing being agreed to.
    let plan = plan_of(vec![
        rename("/project/engine/render.rs", "/project/engine/renderer.rs"),
        Operation::Create {
            path: PathBuf::from("/project/docs"),
            directory: true,
        },
        Operation::Delete {
            path: PathBuf::from("/project/README.md"),
        },
    ]);

    let rows = shown(&plan, ROOM);
    let listed = rows.join("\n");

    assert!(listed.contains("1 rename, 1 delete, 1 create"), "{rows:?}");
    assert!(
        listed.contains("rename  engine/render.rs → engine/renderer.rs"),
        "{rows:?}"
    );
    assert!(listed.contains("create  docs/"), "{rows:?}");
    assert!(listed.contains("delete  README.md"), "{rows:?}");
}

#[test]
fn paths_are_read_against_the_folder_on_screen() {
    let plan = plan_of(vec![Operation::Delete {
        path: PathBuf::from("/project/engine/render.rs"),
    }]);

    let rows = shown(&plan, ROOM);
    assert!(
        rows.iter().any(|row| row.contains("engine/render.rs")),
        "{rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("/project/engine")),
        "the panel is already showing /project; repeating it on every row \
         crowds out the part that differs: {rows:?}"
    );
}

#[test]
fn a_path_outside_the_folder_on_screen_is_shown_whole() {
    // It should not arise — every operation is built from a row the panel
    // drew — but a path silently rendered as a fragment of itself is how
    // somebody agrees to delete the wrong thing.
    let plan = plan_of(vec![Operation::Delete {
        path: PathBuf::from("/elsewhere/important"),
    }]);

    let rows = shown(&plan, ROOM);
    assert!(
        rows.iter().any(|row| row.contains("/elsewhere/important")),
        "{rows:?}"
    );
}

#[test]
fn a_plan_too_long_for_the_panel_says_how_much_it_is_not_showing() {
    // THE test this module exists for. A list that cannot be complete must
    // never look complete.
    let operations: Vec<Operation> = (0..40)
        .map(|index| Operation::Delete {
            path: PathBuf::from(format!("/project/file-{index}.rs")),
        })
        .collect();
    let plan = plan_of(operations);

    let rows = shown(&plan, ROOM);
    assert!(
        rows.len() <= ROOM,
        "the confirmation drew more rows than the panel has: {}",
        rows.len()
    );

    let listed = rows.iter().filter(|row| row.contains("delete ")).count();
    let hidden = 40 - listed;
    assert!(
        rows.iter()
            .any(|row| row.contains(&format!("and {hidden} more"))),
        "{listed} of 40 operations were shown and the panel did not say the \
         other {hidden} were missing: {rows:?}"
    );
}

#[test]
fn a_plan_that_exactly_fits_shows_every_operation_and_claims_nothing_is_missing() {
    // The boundary either side of the cap: one operation fewer than the
    // budget must not spend a row on an apology.
    let budget = ROOM - 4;
    let operations: Vec<Operation> = (0..budget)
        .map(|index| Operation::Delete {
            path: PathBuf::from(format!("/project/file-{index}.rs")),
        })
        .collect();
    let plan = plan_of(operations);

    let rows = shown(&plan, ROOM);
    assert_eq!(
        rows.iter().filter(|row| row.contains("delete ")).count(),
        budget
    );
    assert!(
        !rows.iter().any(|row| row.contains("more not shown")),
        "a plan that fits was reported as truncated: {rows:?}"
    );
}

#[test]
fn a_panel_with_no_room_at_all_still_shows_an_operation_and_the_prompt() {
    // `room` comes from a window that can be resized to anything, so the
    // arithmetic has to hold at zero rather than underflow into a panel
    // showing only chrome.
    let plan = plan_of(vec![Operation::Delete {
        path: PathBuf::from("/project/a.rs"),
    }]);

    let rows = shown(&plan, 0);
    let listed = rows.join("\n");
    assert!(listed.contains("delete  a.rs"), "{rows:?}");
    assert!(listed.contains("y to apply"), "{rows:?}");
}

#[test]
fn the_three_verbs_do_not_all_read_the_same() {
    // A delete that looks like a rename is the one distinction in this panel
    // worth a colour, and the theme has already drawn it.
    let plan = plan_of(vec![
        Operation::Create {
            path: PathBuf::from("/project/a"),
            directory: false,
        },
        rename("/project/b", "/project/c"),
        Operation::Delete {
            path: PathBuf::from("/project/d"),
        },
    ]);

    let rows = plan_rows(&plan, root(), &Theme::dark(), WIDTH, ROOM);
    let verb_colors: Vec<_> = rows
        .iter()
        .filter_map(|row| row.spans.first())
        .filter(|span| matches!(span.text.as_str(), "create" | "rename" | "delete"))
        .map(|span| format!("{:?}", span.color))
        .collect();

    assert_eq!(verb_colors.len(), 3, "{:?}", text(&rows));
    assert_ne!(verb_colors[0], verb_colors[1]);
    assert_ne!(verb_colors[1], verb_colors[2]);
    assert_ne!(verb_colors[0], verb_colors[2]);
}

#[test]
fn a_name_too_long_for_the_panel_is_visibly_cut() {
    let plan = plan_of(vec![Operation::Delete {
        path: PathBuf::from(format!("/project/{}", "n".repeat(200))),
    }]);

    let rows = shown(&plan, ROOM);
    // The operation row, not the summary — "1 delete" contains "delete" too.
    let listed = rows
        .iter()
        .find(|row| row.starts_with("delete"))
        .cloned()
        .unwrap_or_default();

    assert!(
        listed.chars().count() <= WIDTH,
        "a row ran past the panel: {} characters",
        listed.chars().count()
    );
    assert!(
        listed.ends_with('…'),
        "a cut name did not say it was cut: {listed:?}"
    );
}

// ------------------------------------------------------------- refusals

fn existing(name: &str) -> EditedRow {
    EditedRow {
        origin: Some(RowOrigin {
            name: name.to_owned(),
            path: PathBuf::from(format!("{ROOT}/{name}")),
        }),
        name: name.to_owned(),
        parent: Some(0),
        deleted: false,
        directory: false,
    }
}

#[test]
fn a_refusal_names_the_row_rather_than_numbering_it() {
    // "row 3" is a number somebody has to count to. The row they need is the
    // one they can read.
    let edited = [existing("project"), existing("main.rs")];
    let refusals = [Refusal {
        row: 1,
        reason: "a name with leading or trailing whitespace".to_owned(),
    }];

    let rows = text(&refusal_rows(
        &edited,
        &refusals,
        &Theme::dark(),
        WIDTH,
        ROOM,
    ));
    let listed = rows.join("\n");

    assert!(listed.contains("1 row cannot be applied"), "{rows:?}");
    assert!(listed.contains("main.rs"), "{rows:?}");
    assert!(listed.contains("whitespace"), "{rows:?}");
    assert!(listed.contains("esc to go back"), "{rows:?}");
}

#[test]
fn a_refusal_of_a_row_with_no_name_still_says_which_row() {
    // The commonest refusal of all is an empty name, and a blank line
    // followed by a reason reads as a reason with no row.
    let mut edited = [existing("project"), existing("main.rs")];
    edited[1].name = String::new();
    let refusals = [Refusal {
        row: 1,
        reason: "a row with no name".to_owned(),
    }];

    let rows = text(&refusal_rows(
        &edited,
        &refusals,
        &Theme::dark(),
        WIDTH,
        ROOM,
    ));
    assert!(
        rows.iter().any(|row| row.contains("(a row with no name)")),
        "{rows:?}"
    );
}

#[test]
fn many_refusals_are_capped_and_say_so_too() {
    let edited: Vec<EditedRow> = (0..40)
        .map(|index| existing(&format!("file-{index}.rs")))
        .collect();
    let refusals: Vec<Refusal> = (0..40)
        .map(|row| Refusal {
            row,
            reason: "a name with leading or trailing whitespace".to_owned(),
        })
        .collect();

    let rows = text(&refusal_rows(
        &edited,
        &refusals,
        &Theme::dark(),
        WIDTH,
        ROOM,
    ));
    assert!(rows.len() <= ROOM, "{}", rows.len());
    assert!(
        rows.iter().any(|row| row.contains("more not shown")),
        "{rows:?}"
    );
}

#[test]
fn one_refusal_and_several_are_worded_differently() {
    let edited = [existing("project"), existing("a"), existing("b")];
    let one = [Refusal {
        row: 1,
        reason: "x".to_owned(),
    }];
    let two = [
        Refusal {
            row: 1,
            reason: "x".to_owned(),
        },
        Refusal {
            row: 2,
            reason: "y".to_owned(),
        },
    ];

    let theme = Theme::dark();
    let single = text(&refusal_rows(&edited, &one, &theme, WIDTH, ROOM));
    let plural = text(&refusal_rows(&edited, &two, &theme, WIDTH, ROOM));

    assert!(single[0].contains("1 row cannot"), "{single:?}");
    assert!(plural[0].contains("2 rows cannot"), "{plural:?}");
}
