//! Tests for the editable buffer.
//!
//! Two things here can put a file somewhere nobody asked for, and both are
//! arithmetic rather than filesystem work, so both are tested without a disk:
//!
//! - **the parent derivation**, which decides what folder a row is in from
//!   nothing but the indentation it was drawn at, and
//! - **the index shifting** in [`Buffer::insert_below`] and
//!   [`Buffer::remove_typed`], where every parent index below the edit moves.
//!
//! The oracle for both is the same and is deliberately not an index: a row's
//! parent is checked *by name*, and an edit is required to leave every other
//! row's parent name exactly as it was. An off-by-one in the shifting reads as
//! a row that has silently changed folders, which is what it would be.

use std::path::PathBuf;

use super::buffer::{Buffer, SourceRow};
use super::plan::Operation;

/// The root every fixture hangs off.
const ROOT: &str = "/project";

/// One drawn row. The name is the path's last component, so a fixture reads
/// as a directory listing rather than as a pair that has to be kept in step.
fn node(depth: usize, path: &str, directory: bool) -> SourceRow {
    SourceRow {
        depth,
        name: path.rsplit('/').next().unwrap_or(path).to_owned(),
        path: PathBuf::from(path),
        directory,
    }
}

fn file(depth: usize, path: &str) -> SourceRow {
    node(depth, path, false)
}

fn folder(depth: usize, path: &str) -> SourceRow {
    node(depth, path, true)
}

/// A small project, drawn as the panel would draw it fully expanded.
///
/// ```text
/// project
///   alpha
///     one.rs
///     two.rs
///   beta
///     three.rs
///   loose.rs
/// ```
fn fixture() -> Vec<SourceRow> {
    vec![
        folder(0, ROOT),
        folder(1, "/project/alpha"),
        file(2, "/project/alpha/one.rs"),
        file(2, "/project/alpha/two.rs"),
        folder(1, "/project/beta"),
        file(2, "/project/beta/three.rs"),
        file(1, "/project/loose.rs"),
    ]
}

/// Each row's parent, **named**. The oracle for every shifting test: an index
/// that moved by one is unreadable, a row that changed folders is not.
fn parents(buffer: &Buffer) -> Vec<Option<String>> {
    let rows = buffer.rows();
    rows.iter()
        .map(|row| {
            row.parent
                .and_then(|parent| rows.get(parent))
                .map(|parent| parent.name.clone())
        })
        .collect()
}

/// Every row's name, so a fixture's shape is legible in a failure.
fn names(buffer: &Buffer) -> Vec<String> {
    buffer.rows().iter().map(|row| row.name.clone()).collect()
}

/// The operations the buffer now plans, described.
fn operations(buffer: &Buffer) -> Vec<String> {
    buffer
        .plan()
        .unwrap_or_else(|refusals| {
            panic!(
                "expected a plan, refused:\n{}",
                refusals
                    .iter()
                    .map(|refusal| format!("  row {}: {}", refusal.row, refusal.reason))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
        .operations
        .iter()
        .map(Operation::describe)
        .collect()
}

// ---------------------------------------------------------------- loading

#[test]
fn a_freshly_loaded_buffer_is_clean_and_plans_nothing() {
    let buffer = Buffer::load(&fixture());
    assert!(!buffer.is_dirty());
    assert!(operations(&buffer).is_empty());
}

#[test]
fn an_empty_listing_makes_an_empty_buffer() {
    let buffer = Buffer::load(&[]);
    assert!(buffer.rows().is_empty());
    assert!(!buffer.is_dirty());
}

#[test]
fn each_row_remembers_the_name_and_path_it_loaded_with() {
    // The by-id rule, kept as data: the pairing between a row and what it was
    // is captured here and never re-derived by comparing names later.
    let buffer = Buffer::load(&fixture());
    let row = &buffer.rows()[2];
    let origin = row
        .origin
        .as_ref()
        .expect("a loaded row carries its origin");
    assert_eq!(origin.name, "one.rs");
    assert_eq!(origin.path, PathBuf::from("/project/alpha/one.rs"));
}

#[test]
fn nesting_follows_the_indentation() {
    let buffer = Buffer::load(&fixture());
    assert_eq!(
        parents(&buffer),
        vec![
            None,
            Some("project".to_owned()),
            Some("alpha".to_owned()),
            Some("alpha".to_owned()),
            Some("project".to_owned()),
            Some("beta".to_owned()),
            Some("project".to_owned()),
        ]
    );
}

#[test]
fn a_row_that_dedents_all_the_way_attaches_to_the_root() {
    // `loose.rs` is drawn after a two-level subtree and belongs beside the
    // folders, not inside the last one the walk happened to be in.
    let buffer = Buffer::load(&fixture());
    assert_eq!(parents(&buffer)[6], Some("project".to_owned()));
}

#[test]
fn a_row_that_dedents_partway_attaches_to_the_level_above_it() {
    let source = vec![
        folder(0, ROOT),
        folder(1, "/project/a"),
        folder(2, "/project/a/b"),
        file(3, "/project/a/b/deep.rs"),
        file(2, "/project/a/beside.rs"),
    ];
    let buffer = Buffer::load(&source);
    assert_eq!(
        parents(&buffer)[4],
        Some("a".to_owned()),
        "a row dedenting to depth two must land in the depth-one folder"
    );
}

#[test]
fn a_rename_after_a_dedent_is_planned_against_the_folder_the_row_is_in() {
    // The parent derivation, end to end. A wrong answer above shows up here
    // as a rename into a directory the file was never in.
    let mut buffer = Buffer::load(&fixture());
    buffer.rename(6, "renamed.rs");

    assert_eq!(
        operations(&buffer),
        vec!["rename /project/loose.rs → /project/renamed.rs"]
    );
}

#[test]
fn a_rename_deep_in_the_tree_is_planned_against_its_own_folder() {
    let mut buffer = Buffer::load(&fixture());
    buffer.rename(5, "renamed.rs");

    assert_eq!(
        operations(&buffer),
        vec!["rename /project/beta/three.rs → /project/beta/renamed.rs"]
    );
}

#[test]
fn a_depth_that_jumps_is_refused_rather_than_planned_into_the_wrong_folder() {
    // Neither row source produces a gap — the tree draws an expansion and the
    // filter keeps every ancestor of a hit — so this is what happens if one
    // ever did. The row is drawn as a child of the root while living a level
    // below it, so every path computed for it comes out of the wrong folder:
    // without the refusal these rows plan
    // `rename /project/deep.rs → /project/renamed.rs`, naming a source the
    // row has nothing to do with. It refuses.
    let source = vec![folder(0, ROOT), file(2, "/project/mid/deep.rs")];
    let mut buffer = Buffer::load(&source);
    buffer.rename(1, "renamed.rs");

    let refusals = buffer.plan().expect_err("a row drawn in the wrong folder");
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].row, 1);
}

// ---------------------------------------------------------------- editing

#[test]
fn renaming_a_row_makes_the_buffer_dirty_and_discarding_restores_it() {
    let mut buffer = Buffer::load(&fixture());
    buffer.rename(2, "renamed.rs");
    assert!(buffer.is_dirty());

    buffer.discard();
    assert!(!buffer.is_dirty());
    assert!(operations(&buffer).is_empty());
}

#[test]
fn striking_a_row_and_striking_it_again_leaves_the_buffer_clean() {
    // "Dirty" is a comparison against what loaded, not a flag somebody sets —
    // so an edit that was undone by hand is genuinely undone.
    let mut buffer = Buffer::load(&fixture());
    buffer.toggle_deleted(3);
    assert!(buffer.is_dirty());

    buffer.toggle_deleted(3);
    assert!(!buffer.is_dirty());
}

#[test]
fn a_struck_row_plans_a_delete_of_where_it_actually_is() {
    let mut buffer = Buffer::load(&fixture());
    buffer.toggle_deleted(2);

    assert_eq!(operations(&buffer), vec!["delete /project/alpha/one.rs"]);
}

#[test]
fn renaming_a_row_that_is_not_there_does_nothing() {
    let mut buffer = Buffer::load(&fixture());
    buffer.rename(99, "nowhere.rs");
    assert!(!buffer.is_dirty());
}

#[test]
fn striking_a_row_that_is_not_there_does_nothing() {
    let mut buffer = Buffer::load(&fixture());
    buffer.toggle_deleted(99);
    assert!(!buffer.is_dirty());
}

// ------------------------------------------------------- the folder sigil

#[test]
fn a_trailing_slash_makes_a_typed_row_a_folder() {
    let mut buffer = Buffer::load(&fixture());
    let at = buffer.insert_below(6, false);
    buffer.rename(at, "docs/");

    assert_eq!(buffer.rows()[at].name, "docs", "the slash reached the plan");
    assert_eq!(operations(&buffer), vec!["create /project/docs/"]);
}

#[test]
fn taking_the_slash_off_again_makes_it_a_file() {
    let mut buffer = Buffer::load(&fixture());
    let at = buffer.insert_below(6, false);
    buffer.rename(at, "docs/");
    buffer.rename(at, "docs");

    assert_eq!(operations(&buffer), vec!["create /project/docs"]);
}

#[test]
fn a_trailing_slash_on_an_existing_row_is_not_a_request_to_change_what_it_is() {
    // What a path is on disk is not the buffer's to decide, so the slash is
    // read off and the rename is just a rename.
    let mut buffer = Buffer::load(&fixture());
    buffer.rename(1, "renamed/");

    assert_eq!(
        operations(&buffer),
        vec!["rename /project/alpha → /project/renamed"]
    );
}

// -------------------------------------------------------------- inserting

#[test]
fn a_new_row_under_a_folder_belongs_to_that_folder() {
    let mut buffer = Buffer::load(&fixture());
    let at = buffer.insert_below(1, true);
    buffer.rename(at, "new.rs");

    assert_eq!(at, 2, "the new row is drawn immediately below the folder");
    assert_eq!(operations(&buffer), vec!["create /project/alpha/new.rs"]);
}

#[test]
fn a_new_row_under_a_file_belongs_beside_it() {
    let mut buffer = Buffer::load(&fixture());
    let at = buffer.insert_below(2, false);
    buffer.rename(at, "new.rs");

    assert_eq!(operations(&buffer), vec!["create /project/alpha/new.rs"]);
}

#[test]
fn inserting_a_row_does_not_reparent_the_rows_below_it() {
    // The shifting arithmetic. Every parent index at or after the insertion
    // point names a row that has moved down one, and getting that wrong puts
    // a file in a folder it was never drawn in.
    let mut buffer = Buffer::load(&fixture());
    let before = parents(&buffer);

    let at = buffer.insert_below(1, true);

    let mut after = parents(&buffer);
    assert_eq!(after[at], Some("alpha".to_owned()));
    after.remove(at);
    assert_eq!(
        after, before,
        "an insertion moved a row into another folder"
    );
}

#[test]
fn inserting_deep_in_the_tree_leaves_every_other_rows_plan_alone() {
    // The same claim through the planner rather than through the parent
    // names: with the new row named, every other row still plans nothing.
    let mut buffer = Buffer::load(&fixture());
    let at = buffer.insert_below(5, false);
    buffer.rename(at, "four.rs");

    assert_eq!(operations(&buffer), vec!["create /project/beta/four.rs"]);
}

#[test]
fn inserting_into_an_empty_buffer_appends_a_row_with_no_parent() {
    let mut buffer = Buffer::default();
    let at = buffer.insert_below(0, true);

    assert_eq!(at, 0);
    assert_eq!(buffer.rows().len(), 1);
    assert_eq!(buffer.rows()[0].parent, None);
}

#[test]
fn inserting_below_the_last_row_lands_at_the_end() {
    let mut buffer = Buffer::load(&fixture());
    let last = buffer.rows().len() - 1;
    let at = buffer.insert_below(last, false);

    assert_eq!(at, last + 1);
    assert_eq!(parents(&buffer)[at], Some("project".to_owned()));
}

// --------------------------------------------------------------- removing

#[test]
fn a_row_that_came_from_the_tree_is_not_removable() {
    // An existing file is struck through, never removed: the buffer has to
    // keep saying what is on disk.
    let mut buffer = Buffer::load(&fixture());
    let before = names(&buffer);

    assert!(!buffer.remove_typed(2));
    assert_eq!(names(&buffer), before);
    assert!(!buffer.is_dirty());
}

#[test]
fn removing_a_row_that_is_not_there_does_nothing() {
    let mut buffer = Buffer::load(&fixture());
    assert!(!buffer.remove_typed(99));
    assert!(!buffer.is_dirty());
}

#[test]
fn typing_a_row_and_removing_it_again_leaves_the_buffer_exactly_as_it_loaded() {
    // The shifting arithmetic in the other direction, with the strongest
    // oracle available: not "the indices look right" but "the buffer is
    // indistinguishable from the one that loaded".
    let mut buffer = Buffer::load(&fixture());
    let at = buffer.insert_below(1, true);
    buffer.rename(at, "new.rs");

    assert!(buffer.remove_typed(at));
    assert!(
        !buffer.is_dirty(),
        "removing a typed row left the buffer changed: {:?}",
        parents(&buffer)
    );
}

#[test]
fn removing_a_typed_folder_takes_the_rows_typed_inside_it() {
    // Removing the row alone would leave its children behind with a parent
    // index naming whichever row landed in that slot — a folder they were
    // never drawn in.
    let mut buffer = Buffer::load(&fixture());
    let outer = buffer.insert_below(1, true);
    buffer.rename(outer, "docs/");
    let inner = buffer.insert_below(outer, true);
    buffer.rename(inner, "inside.md");

    assert_eq!(
        operations(&buffer),
        vec![
            "create /project/alpha/docs/",
            "create /project/alpha/docs/inside.md"
        ]
    );

    assert!(buffer.remove_typed(outer));
    assert!(
        !buffer.is_dirty(),
        "the row typed inside survived its folder: {:?}",
        names(&buffer)
    );
}

#[test]
fn removing_a_typed_row_leaves_the_rows_below_it_in_their_own_folders() {
    let mut buffer = Buffer::load(&fixture());
    let at = buffer.insert_below(1, true);
    let expected = {
        let mut without = parents(&buffer);
        without.remove(at);
        without
    };

    assert!(buffer.remove_typed(at));
    assert_eq!(parents(&buffer), expected);
}

#[test]
fn discarding_undoes_an_insertion_as_well_as_an_edit() {
    let mut buffer = Buffer::load(&fixture());
    let at = buffer.insert_below(4, true);
    buffer.rename(at, "new.rs");
    buffer.rename(2, "renamed.rs");
    buffer.toggle_deleted(3);
    assert!(buffer.is_dirty());

    buffer.discard();
    assert!(!buffer.is_dirty());
    assert_eq!(buffer.rows().len(), fixture().len());
    assert!(operations(&buffer).is_empty());
}
