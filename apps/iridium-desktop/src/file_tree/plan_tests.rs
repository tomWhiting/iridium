//! Tests for the plan builder.
//!
//! Pure, and deliberately so: this is the logic that can destroy a file, and
//! testing it through a worker thread and a real directory would make the
//! suite slow, timing-dependent, and reluctant to cover the cases that matter
//! — a rename cycle, a rename inside a renamed folder, two rows fighting over
//! one name.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::plan::{EditedRow, Operation, Plan, Refusal, RowOrigin, plan};

/// The root every fixture hangs off.
const ROOT: &str = "/project";

/// The root row: it exists, it is nobody's child, and it cannot change.
fn root() -> EditedRow {
    EditedRow {
        origin: Some(RowOrigin {
            name: "project".to_owned(),
            path: PathBuf::from(ROOT),
        }),
        name: "project".to_owned(),
        parent: None,
        deleted: false,
        directory: true,
    }
}

/// A row that came from the tree, under `parent`, still named what it was.
fn existing(name: &str, parent: usize, path: &str) -> EditedRow {
    EditedRow {
        origin: Some(RowOrigin {
            name: name.to_owned(),
            path: PathBuf::from(path),
        }),
        name: name.to_owned(),
        parent: Some(parent),
        deleted: false,
        directory: false,
    }
}

/// A row the user typed.
fn typed(name: &str, parent: usize, directory: bool) -> EditedRow {
    EditedRow {
        origin: None,
        name: name.to_owned(),
        parent: Some(parent),
        deleted: false,
        directory,
    }
}

/// The plan, or a panic naming the refusals — a test that meant to plan and
/// got refused should say which row and why, not just fail an unwrap.
fn planned(rows: &[EditedRow]) -> Plan {
    plan(rows).unwrap_or_else(|refusals| {
        panic!(
            "expected a plan, refused:\n{}",
            describe_refusals(&refusals)
        )
    })
}

/// The refusals, or a panic showing the plan that was produced instead.
fn refused(rows: &[EditedRow]) -> Vec<Refusal> {
    match plan(rows) {
        Ok(plan) => panic!(
            "expected a refusal, planned: {:?}",
            plan.operations
                .iter()
                .map(Operation::describe)
                .collect::<Vec<_>>()
        ),
        Err(refusals) => refusals,
    }
}

fn describe_refusals(refusals: &[Refusal]) -> String {
    refusals
        .iter()
        .map(|refusal| format!("  row {}: {}", refusal.row, refusal.reason))
        .collect::<Vec<_>>()
        .join("\n")
}

fn rename(from: &str, to: &str) -> Operation {
    Operation::Rename {
        from: PathBuf::from(from),
        to: PathBuf::from(to),
    }
}

#[test]
fn a_buffer_nobody_edited_plans_nothing() {
    let rows = [
        root(),
        existing("main.rs", 0, "/project/main.rs"),
        existing("lib.rs", 0, "/project/lib.rs"),
    ];
    assert!(
        planned(&rows).is_empty(),
        "an untouched buffer must touch no files at all"
    );
}

#[test]
fn a_renamed_row_plans_one_rename() {
    let mut rows = [root(), existing("main.rs", 0, "/project/main.rs")];
    rows[1].name = "app.rs".to_owned();

    assert_eq!(
        planned(&rows).operations,
        vec![rename("/project/main.rs", "/project/app.rs")]
    );
}

#[test]
fn a_struck_row_plans_a_delete_of_where_it_actually_is() {
    let mut rows = [root(), existing("main.rs", 0, "/project/main.rs")];
    rows[1].deleted = true;

    assert_eq!(
        planned(&rows).operations,
        vec![Operation::Delete {
            path: PathBuf::from("/project/main.rs")
        }]
    );
}

#[test]
fn a_typed_row_plans_a_create() {
    let rows = [root(), typed("notes.md", 0, false), typed("docs", 0, true)];

    assert_eq!(
        planned(&rows).operations,
        vec![
            Operation::Create {
                path: PathBuf::from("/project/notes.md"),
                directory: false,
            },
            Operation::Create {
                path: PathBuf::from("/project/docs"),
                directory: true,
            },
        ]
    );
}

#[test]
fn a_struck_row_that_was_never_on_disk_plans_nothing() {
    // Typing a name and then changing your mind must not reach the disk at
    // all — there is nothing there to delete, and planning one would be an
    // operation against a path the user never created.
    let mut rows = [root(), typed("notes.md", 0, false)];
    rows[1].deleted = true;

    assert!(planned(&rows).is_empty());
}

#[test]
fn renaming_a_directory_does_not_also_rename_everything_inside_it() {
    // THE case this design exists for. Renaming `src` moves its contents as a
    // consequence; emitting a second rename for the child would look for a
    // file the first operation already moved, and fail.
    let mut rows = [
        root(),
        existing("src", 0, "/project/src"),
        existing("main.rs", 1, "/project/src/main.rs"),
    ];
    rows[1].name = "source".to_owned();

    assert_eq!(
        planned(&rows).operations,
        vec![rename("/project/src", "/project/source")],
        "the child moved with its folder and must not be renamed a second time"
    );
}

#[test]
fn a_child_renamed_inside_a_renamed_folder_uses_the_folders_new_path() {
    // Both changed in one apply. The child's rename has to be expressed
    // against where the folder will be by the time it runs, not where it was.
    let mut rows = [
        root(),
        existing("src", 0, "/project/src"),
        existing("main.rs", 1, "/project/src/main.rs"),
    ];
    rows[1].name = "source".to_owned();
    rows[2].name = "app.rs".to_owned();

    assert_eq!(
        planned(&rows).operations,
        vec![
            rename("/project/src", "/project/source"),
            rename("/project/source/main.rs", "/project/source/app.rs"),
        ],
        "the folder must move first, and the child's rename must name the new folder"
    );
}

#[test]
fn a_rename_into_a_name_another_row_is_vacating_happens_second() {
    // `a` to `b` and `b` to `c`. Applied as typed, the first would destroy the
    // original `b`.
    let mut rows = [
        root(),
        existing("a", 0, "/project/a"),
        existing("b", 0, "/project/b"),
    ];
    rows[1].name = "b".to_owned();
    rows[2].name = "c".to_owned();

    assert_eq!(
        planned(&rows).operations,
        vec![
            rename("/project/b", "/project/c"),
            rename("/project/a", "/project/b"),
        ],
        "b must vacate its name before a moves into it"
    );
}

#[test]
fn two_rows_swapping_names_go_through_a_temporary() {
    let mut rows = [
        root(),
        existing("a", 0, "/project/a"),
        existing("b", 0, "/project/b"),
    ];
    rows[1].name = "b".to_owned();
    rows[2].name = "a".to_owned();

    let operations = planned(&rows).operations;

    assert_eq!(
        operations.len(),
        3,
        "a two-way swap needs three moves: out, across, back — got {operations:?}"
    );

    // Whatever order the cycle is broken in, the invariant is the same: no
    // rename may target a path that a later rename still reads from.
    assert_nothing_is_overwritten(&operations, &["/project/a", "/project/b"]);

    // And the end state must be the swap that was asked for.
    assert_eq!(
        final_locations(&operations, &["/project/a", "/project/b"]),
        vec![PathBuf::from("/project/b"), PathBuf::from("/project/a")],
        "the two files did not end up in each other's places: {operations:?}"
    );
}

#[test]
fn a_three_way_rotation_also_resolves() {
    let mut rows = [
        root(),
        existing("a", 0, "/project/a"),
        existing("b", 0, "/project/b"),
        existing("c", 0, "/project/c"),
    ];
    rows[1].name = "b".to_owned();
    rows[2].name = "c".to_owned();
    rows[3].name = "a".to_owned();

    let operations = planned(&rows).operations;
    assert_nothing_is_overwritten(&operations, &["/project/a", "/project/b", "/project/c"]);
    assert_eq!(
        final_locations(&operations, &["/project/a", "/project/b", "/project/c"]),
        vec![
            PathBuf::from("/project/b"),
            PathBuf::from("/project/c"),
            PathBuf::from("/project/a")
        ],
        "the rotation did not land: {operations:?}"
    );
}

/// Walks the renames the way the filesystem would and reports where each
/// starting path ended up.
///
/// This is the oracle that matters: asserting an exact operation list would
/// pin one particular way of breaking a cycle, and any correct ordering should
/// pass. Simulating is what checks the *outcome*.
fn final_locations(operations: &[Operation], starts: &[&str]) -> Vec<PathBuf> {
    let mut where_now: Vec<PathBuf> = starts.iter().map(PathBuf::from).collect();
    for operation in operations {
        let Operation::Rename { from, to } = operation else {
            continue;
        };
        for location in &mut where_now {
            if location == from {
                location.clone_from(to);
            }
        }
    }
    where_now
}

/// Replays the plan against a set of live paths and asserts it never
/// overwrites anything, and never reads from nothing.
///
/// An earlier version of this asserted that no rename targets a path a later
/// rename reads from. That was wrong, and wrong in a way worth recording: a
/// temporary is *exactly* a path written and then read back, so the check
/// forbade the very mechanism it was meant to verify. Simulating occupancy
/// asks the real question — at the moment each operation runs, is its source
/// there, and is its destination free?
fn assert_nothing_is_overwritten(operations: &[Operation], initial: &[&str]) {
    let mut live: BTreeSet<PathBuf> = initial.iter().map(PathBuf::from).collect();
    for operation in operations {
        match operation {
            Operation::Rename { from, to } => {
                assert!(
                    live.contains(from),
                    "renames from {} which nothing occupies: {operations:?}",
                    from.display()
                );
                assert!(
                    !live.contains(to),
                    "renames onto {}, destroying what is there: {operations:?}",
                    to.display()
                );
                live.remove(from);
                live.insert(to.clone());
            },
            Operation::Create { path, .. } => {
                assert!(
                    !live.contains(path),
                    "creates {}, which already exists: {operations:?}",
                    path.display()
                );
                live.insert(path.clone());
            },
            Operation::Delete { path } => {
                live.remove(path);
            },
        }
    }
}

#[test]
fn deletes_run_after_every_rename() {
    // A rename out of a directory that is being deleted must still find its
    // source.
    let mut rows = [
        root(),
        existing("old", 0, "/project/old"),
        existing("keep.rs", 1, "/project/old/keep.rs"),
    ];
    rows[1].deleted = true;
    rows[2].name = "kept.rs".to_owned();

    let operations = planned(&rows).operations;
    let delete_at = operations
        .iter()
        .position(|operation| matches!(operation, Operation::Delete { .. }))
        .expect("the delete was planned");
    let rename_at = operations
        .iter()
        .position(|operation| matches!(operation, Operation::Rename { .. }))
        .expect("the rename was planned");

    assert!(
        rename_at < delete_at,
        "the delete ran before the rename it would have destroyed: {operations:?}"
    );
}

#[test]
fn a_row_drawn_inside_a_folder_it_does_not_live_in_is_refused() {
    // The check that makes the whole target computation safe. A target is
    // built by joining the *drawn* parent chain, while an existing row also
    // knows where it *really* is. Those two agree for every row either row
    // source produces — and if they ever stopped agreeing, the rename would
    // be computed entirely from the drawn chain and would name a path the row
    // has nothing to do with.
    //
    // Without this refusal the rows below plan
    // `rename /project/src/main.rs → /project/src/renamed.rs`, while the row
    // is really `/project/src/deep/main.rs`. That is not a move: it is an
    // operation against **a different file**, which renames whatever happens
    // to be sitting at that path and leaves the row's own file untouched.
    let mut rows = [
        root(),
        existing("src", 0, "/project/src"),
        existing("main.rs", 1, "/project/src/deep/main.rs"),
    ];
    rows[2].name = "renamed.rs".to_owned();

    let refusals = refused(&rows);
    assert_eq!(refusals.len(), 1, "{}", describe_refusals(&refusals));
    assert_eq!(refusals[0].row, 2);
}

#[test]
fn an_existing_row_cannot_be_nested_under_one_the_user_typed() {
    // The same check from the other side: a file that is already on disk
    // cannot live inside a folder that does not exist yet, so a buffer saying
    // it does is describing something impossible rather than something to do.
    let rows = [
        root(),
        typed("new-folder", 0, true),
        existing("main.rs", 1, "/project/main.rs"),
    ];

    let refusals = refused(&rows);
    assert_eq!(refusals.len(), 1, "{}", describe_refusals(&refusals));
    assert_eq!(refusals[0].row, 2);
}

#[test]
fn two_rows_claiming_one_name_are_refused() {
    let mut rows = [
        root(),
        existing("a.rs", 0, "/project/a.rs"),
        existing("b.rs", 0, "/project/b.rs"),
    ];
    rows[2].name = "a.rs".to_owned();

    let refusals = refused(&rows);
    assert_eq!(refusals.len(), 1, "{}", describe_refusals(&refusals));
    assert_eq!(refusals[0].row, 2);
}

#[test]
fn a_name_that_is_really_a_path_is_refused() {
    // Without this a row could write outside the folder it is drawn in, which
    // is the difference between renaming a file and moving one somewhere the
    // user cannot see.
    for attempt in ["../escape", "sub/file.rs", "/absolute", "..", "."] {
        let mut rows = [root(), existing("a.rs", 0, "/project/a.rs")];
        rows[1].name = attempt.to_owned();
        let refusals = refused(&rows);
        assert_eq!(
            refusals.len(),
            1,
            "{attempt:?} should be refused exactly once: {}",
            describe_refusals(&refusals)
        );
    }
}

#[test]
fn an_empty_or_padded_name_is_refused() {
    for attempt in ["", "   ", " a.rs", "a.rs "] {
        let mut rows = [root(), existing("a.rs", 0, "/project/a.rs")];
        rows[1].name = attempt.to_owned();
        assert_eq!(refused(&rows).len(), 1, "{attempt:?} should be refused");
    }
}

#[test]
fn the_root_cannot_be_renamed_or_deleted_from_inside_itself() {
    let mut rows = [root()];
    rows[0].name = "elsewhere".to_owned();
    assert_eq!(refused(&rows).len(), 1);

    let mut rows = [root()];
    rows[0].deleted = true;
    assert_eq!(refused(&rows).len(), 1);
}

#[test]
fn a_row_after_the_first_with_no_folder_above_it_is_refused() {
    // Exactly one row is the mount point, and it is the first one. A later
    // row with nothing above it is a row whose nesting could not be worked
    // out — and accepting it as a *second* root quietly makes it unrenamable
    // and undeletable, under a message calling it "the root". The user is
    // looking straight at a row that is plainly not the root.
    let mut rows = [
        root(),
        existing("a.rs", 0, "/project/a.rs"),
        existing("b.rs", 0, "/project/b.rs"),
    ];
    rows[2].parent = None;
    rows[2].name = "renamed.rs".to_owned();

    let refusals = refused(&rows);
    assert_eq!(refusals.len(), 1, "{}", describe_refusals(&refusals));
    assert_eq!(refusals[0].row, 2);
    assert!(
        !refusals[0].reason.contains("root"),
        "a row that is plainly not the root was refused as one: {}",
        refusals[0].reason
    );
}

#[test]
fn a_row_nested_under_one_that_comes_after_it_is_refused() {
    // The panel derives `parent` from indentation, so a bug there must be a
    // refusal rather than a path built from a target that does not exist yet.
    let rows = [root(), existing("a.rs", 2, "/project/a.rs"), root()];
    let refusals = refused(&rows);
    assert!(
        refusals.iter().any(|refusal| refusal.row == 1),
        "{}",
        describe_refusals(&refusals)
    );
}

#[test]
fn one_bad_row_refuses_the_whole_buffer() {
    // A half-applied rename of a directory tree is worse than none of it.
    let mut rows = [
        root(),
        existing("a.rs", 0, "/project/a.rs"),
        existing("b.rs", 0, "/project/b.rs"),
    ];
    rows[1].name = "fine.rs".to_owned();
    rows[2].name = "../bad".to_owned();

    let refusals = refused(&rows);
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].row, 2);
}

#[test]
fn every_refusal_is_reported_not_just_the_first() {
    let mut rows = [
        root(),
        existing("a.rs", 0, "/project/a.rs"),
        existing("b.rs", 0, "/project/b.rs"),
    ];
    rows[1].name = String::new();
    rows[2].name = "../bad".to_owned();

    let refusals = refused(&rows);
    assert_eq!(
        refusals
            .iter()
            .map(|refusal| refusal.row)
            .collect::<Vec<_>>(),
        vec![1, 2],
        "someone who mistyped two names must see both: {}",
        describe_refusals(&refusals)
    );
}

#[test]
fn the_summary_counts_and_pluralises_what_will_happen() {
    let mut rows = [
        root(),
        existing("a.rs", 0, "/project/a.rs"),
        existing("b.rs", 0, "/project/b.rs"),
        existing("gone.rs", 0, "/project/gone.rs"),
        typed("new.rs", 0, false),
    ];
    rows[1].name = "one.rs".to_owned();
    rows[2].name = "two.rs".to_owned();
    rows[3].deleted = true;

    assert_eq!(planned(&rows).summary(), "2 renames, 1 delete, 1 create");
    assert_eq!(Plan::default().summary(), "nothing to do");
}

#[test]
fn an_operation_describes_itself_by_name() {
    // The confirmation is the only thing standing between a typo and the
    // filesystem, so it has to read as what will actually happen.
    assert_eq!(
        rename("/project/a.rs", "/project/b.rs").describe(),
        "rename /project/a.rs → /project/b.rs"
    );
    assert_eq!(
        Operation::Delete {
            path: PathBuf::from("/project/gone.rs")
        }
        .describe(),
        "delete /project/gone.rs"
    );
    assert_eq!(
        Operation::Create {
            path: PathBuf::from("/project/docs"),
            directory: true,
        }
        .describe(),
        "create /project/docs/"
    );
}

#[test]
fn a_deleted_row_does_not_collide_with_a_new_one_taking_its_name() {
    // Striking a file through and typing a fresh one with the same name is a
    // legitimate way to empty a file, and must not read as two rows fighting.
    let mut rows = [
        root(),
        existing("data.json", 0, "/project/data.json"),
        typed("data.json", 0, false),
    ];
    rows[1].deleted = true;

    let operations = planned(&rows).operations;
    assert!(
        operations
            .iter()
            .any(|operation| matches!(operation, Operation::Create { .. })),
        "{operations:?}"
    );
    assert!(
        operations
            .iter()
            .any(|operation| matches!(operation, Operation::Delete { .. })),
        "{operations:?}"
    );
}

#[test]
fn the_temporary_a_cycle_uses_is_not_a_name_any_row_claims() {
    // If the swap name collided with a row's own name the cycle would resolve
    // by destroying it.
    let mut rows = [
        root(),
        existing("a", 0, "/project/a"),
        existing("b", 0, "/project/b"),
    ];
    rows[1].name = "b".to_owned();
    rows[2].name = "a".to_owned();

    let claimed: Vec<&Path> = [Path::new("/project/a"), Path::new("/project/b")].to_vec();
    let plan = planned(&rows);
    let temporaries: Vec<&PathBuf> = plan
        .operations
        .iter()
        .filter_map(|operation| match operation {
            Operation::Rename { to, .. } if !claimed.contains(&to.as_path()) => Some(to),
            _ => None,
        })
        .collect();

    assert_eq!(
        temporaries.len(),
        1,
        "expected exactly one temporary: {temporaries:?}"
    );
}
