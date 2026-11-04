//! Tests for the listing: the rows on screen written as text, and the text
//! read back as the rows the planner takes.
//!
//! What can put a file somewhere nobody asked for here is the folder a line
//! is read as being in, so the oracle is deliberately not an index: a row's
//! parent is checked *by name*, and an edit is required to leave every other
//! row's parent name exactly as it was.

use std::path::PathBuf;

use super::buffer::{Listing, LoadError, SourceRow};
use super::plan::Operation;
use crate::outline::LineState;

/// The root every fixture hangs off.
const ROOT: &str = "/project";

/// One drawn row, named by its path's last component.
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
/// alpha/
///   one.rs
///   two.rs
/// beta/
///   three.rs
/// loose.rs
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

/// The fixture, loaded.
fn loaded() -> Listing {
    Listing::load(&fixture()).expect("the fixture starts at its root")
}

/// Follows the listing to `text`, as one edit.
fn edit(listing: &mut Listing, text: &str) {
    listing.track(text.split('\n').map(str::to_owned).collect());
}

/// Each row's parent, **named**.
fn parents(listing: &Listing) -> Vec<Option<String>> {
    let rows = listing.rows().rows;
    rows.iter()
        .map(|row| {
            row.parent
                .and_then(|parent| rows.get(parent))
                .map(|parent| parent.name.clone())
        })
        .collect()
}

/// The operations the listing now plans, described.
fn operations(listing: &Listing) -> Vec<String> {
    listing
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
fn the_listing_is_written_as_indented_lines_with_folders_marked() {
    assert_eq!(
        loaded().text(),
        "alpha/\n  one.rs\n  two.rs\nbeta/\n  three.rs\nloose.rs"
    );
}

#[test]
fn a_freshly_loaded_listing_is_clean_and_plans_nothing() {
    let listing = loaded();
    assert!(!listing.is_dirty());
    assert!(operations(&listing).is_empty());
    assert_eq!(listing.removed_count(), 0);
}

#[test]
fn a_folder_with_nothing_drawn_in_it_is_one_empty_line() {
    let listing = Listing::load(&[folder(0, ROOT)]).expect("a root alone loads");
    assert_eq!(listing.text(), "");
    assert_eq!(listing.rows().rows.len(), 1, "only the root");
    assert!(!listing.is_dirty());
}

#[test]
fn rows_that_do_not_start_at_a_folder_cannot_be_edited() {
    assert_eq!(Listing::load(&[]).err(), Some(LoadError::NoRoot));
    assert_eq!(
        Listing::load(&[file(0, "/project/loose.rs")]).err(),
        Some(LoadError::NoRoot)
    );
    assert_eq!(
        Listing::load(&[folder(0, ROOT), file(0, "/elsewhere.rs")]).err(),
        Some(LoadError::NoRoot),
        "a second row at the root's depth is not inside it"
    );
}

#[test]
fn a_name_no_line_can_hold_is_refused_before_editing_starts() {
    let source = vec![folder(0, ROOT), file(1, "/project/two\nlines")];
    assert!(matches!(
        Listing::load(&source),
        Err(LoadError::Projection(_))
    ));
}

#[test]
fn the_folder_being_shown_is_the_first_row_and_not_a_line() {
    let rows = loaded().rows();
    assert_eq!(rows.rows[0].name, "project");
    assert_eq!(rows.rows[0].parent, None);
    assert_eq!(rows.lines[0], None, "the root has no line to edit");
    assert_eq!(rows.lines[1], Some(0));
}

#[test]
fn each_row_remembers_the_name_and_path_it_loaded_with() {
    let rows = loaded().rows().rows;
    let origin = rows[2]
        .origin
        .as_ref()
        .expect("a loaded row carries its origin");
    assert_eq!(origin.name, "one.rs");
    assert_eq!(origin.path, PathBuf::from("/project/alpha/one.rs"));
}

#[test]
fn nesting_follows_the_indentation() {
    assert_eq!(
        parents(&loaded()),
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
fn a_row_that_dedents_partway_attaches_to_the_level_above_it() {
    let source = vec![
        folder(0, ROOT),
        folder(1, "/project/a"),
        folder(2, "/project/a/b"),
        file(3, "/project/a/b/deep.rs"),
        file(2, "/project/a/beside.rs"),
    ];
    let listing = Listing::load(&source).expect("the fixture starts at its root");
    assert_eq!(parents(&listing)[4], Some("a".to_owned()));
}

#[test]
fn a_depth_that_jumps_is_refused_rather_than_planned_into_the_wrong_folder() {
    // Neither row source produces a gap, so this is what happens if one ever
    // did: the row reads as a child of the root while living a level below
    // it, and the planner refuses rather than computing paths from there.
    let source = vec![folder(0, ROOT), file(2, "/project/mid/deep.rs")];
    let mut listing = Listing::load(&source).expect("the fixture starts at its root");
    edit(&mut listing, "  renamed.rs");
    let refusals = listing.plan().expect_err("a row drawn in the wrong folder");
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].row, 1);
}

// ---------------------------------------------------------------- editing

#[test]
fn a_renamed_line_is_planned_against_the_folder_it_is_in() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  one.rs\n  two.rs\nbeta/\n  renamed.rs\nloose.rs",
    );
    assert_eq!(
        operations(&listing),
        vec!["rename /project/beta/three.rs → /project/beta/renamed.rs"]
    );
}

#[test]
fn a_rename_after_a_dedent_is_planned_against_the_root() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  one.rs\n  two.rs\nbeta/\n  three.rs\nrenamed.rs",
    );
    assert_eq!(
        operations(&listing),
        vec!["rename /project/loose.rs → /project/renamed.rs"]
    );
}

#[test]
fn renaming_a_line_back_by_hand_leaves_the_listing_clean() {
    let mut listing = loaded();
    let original = listing.text();
    edit(&mut listing, &original.replace("one.rs", "uno.rs"));
    assert!(listing.is_dirty());
    edit(&mut listing, &original);
    assert!(!listing.is_dirty());
    assert!(operations(&listing).is_empty());
}

#[test]
fn a_deleted_line_plans_a_delete_of_where_the_file_actually_is() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  two.rs\nbeta/\n  three.rs\nloose.rs",
    );
    assert_eq!(listing.removed_count(), 1);
    assert_eq!(operations(&listing), vec!["delete /project/alpha/one.rs"]);
}

#[test]
fn deleting_a_folder_and_its_lines_deletes_the_folder_once() {
    let mut listing = loaded();
    edit(&mut listing, "beta/\n  three.rs\nloose.rs");
    assert_eq!(listing.removed_count(), 3);
    assert_eq!(operations(&listing), vec!["delete /project/alpha"]);
}

#[test]
fn a_trailing_slash_makes_a_typed_line_a_folder() {
    let mut listing = loaded();
    let text = format!("{}\ndocs/", listing.text());
    edit(&mut listing, &text);
    assert_eq!(operations(&listing), vec!["create /project/docs/"]);
    let rows = listing.rows().rows;
    assert_eq!(rows.last().map(|row| row.name.as_str()), Some("docs"));
}

#[test]
fn a_typed_line_without_a_slash_is_a_file() {
    let mut listing = loaded();
    let text = format!("{}\ndocs", listing.text());
    edit(&mut listing, &text);
    assert_eq!(operations(&listing), vec!["create /project/docs"]);
}

#[test]
fn taking_the_slash_off_a_loaded_folder_is_not_a_request_to_change_what_it_is() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha\n  one.rs\n  two.rs\nbeta/\n  three.rs\nloose.rs",
    );
    assert!(operations(&listing).is_empty());

    edit(
        &mut listing,
        "renamed/\n  one.rs\n  two.rs\nbeta/\n  three.rs\nloose.rs",
    );
    assert_eq!(
        operations(&listing),
        vec!["rename /project/alpha → /project/renamed"]
    );
}

#[test]
fn a_line_typed_indented_under_a_folder_belongs_to_that_folder() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  new.rs\n  one.rs\n  two.rs\nbeta/\n  three.rs\nloose.rs",
    );
    assert_eq!(operations(&listing), vec!["create /project/alpha/new.rs"]);
}

#[test]
fn typing_a_line_does_not_move_any_other_line_into_another_folder() {
    let mut listing = loaded();
    let before = parents(&listing);
    edit(
        &mut listing,
        "alpha/\n  new.rs\n  one.rs\n  two.rs\nbeta/\n  three.rs\nloose.rs",
    );
    let mut after = parents(&listing);
    assert_eq!(after[2], Some("alpha".to_owned()));
    after.remove(2);
    assert_eq!(
        after, before,
        "a typed line moved a row into another folder"
    );
}

#[test]
fn line_states_say_what_each_line_is_against_what_loaded() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  uno.rs\n  two.rs\n\nbeta/\n  three.rs\nloose.rs\nnew.rs",
    );
    assert_eq!(
        listing.line_states(),
        vec![
            LineState::Loaded,
            LineState::Changed,
            LineState::Loaded,
            LineState::Blank,
            LineState::Loaded,
            LineState::Loaded,
            LineState::Loaded,
            LineState::Typed,
        ]
    );
}

#[test]
fn blank_lines_are_not_rows() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  one.rs\n\n  two.rs\nbeta/\n  three.rs\nloose.rs\n",
    );
    assert!(!listing.is_dirty(), "a blank line was read as a change");
    assert!(operations(&listing).is_empty());
}

#[test]
fn a_typed_line_and_its_removal_leave_the_listing_as_it_loaded() {
    let mut listing = loaded();
    let original = listing.text();
    edit(&mut listing, &format!("{original}\nscratch.rs"));
    assert!(listing.is_dirty());
    edit(&mut listing, &original);
    assert!(!listing.is_dirty());
    assert!(operations(&listing).is_empty());
}

#[test]
fn a_line_cut_and_pasted_elsewhere_in_its_folder_is_the_same_file() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  two.rs\nbeta/\n  three.rs\nloose.rs",
    );
    edit(
        &mut listing,
        "alpha/\n  two.rs\n  one.rs\nbeta/\n  three.rs\nloose.rs",
    );
    assert!(
        !listing.is_dirty(),
        "a moved line was read as a delete and a create"
    );
}

// ---------------------------------------------------------------- moving

#[test]
fn a_line_cut_from_one_folder_and_pasted_under_another_moves_the_file() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  two.rs\nbeta/\n  three.rs\nloose.rs",
    );
    edit(
        &mut listing,
        "alpha/\n  two.rs\nbeta/\n  three.rs\n  one.rs\nloose.rs",
    );
    assert_eq!(
        operations(&listing),
        vec!["move /project/alpha/one.rs → /project/beta/one.rs"],
        "the paste is the same file, moved, not a delete and a create"
    );
}

#[test]
fn a_line_indented_under_the_folder_above_it_moves_into_that_folder() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  one.rs\n  two.rs\nbeta/\n  three.rs\n  loose.rs",
    );
    assert_eq!(
        operations(&listing),
        vec!["move /project/loose.rs → /project/beta/loose.rs"]
    );
}

#[test]
fn a_line_moved_and_renamed_is_one_move() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  one.rs\n  two.rs\nbeta/\n  three.rs\n  renamed.rs",
    );
    assert_eq!(
        operations(&listing),
        vec!["move /project/loose.rs → /project/beta/renamed.rs"]
    );
}

#[test]
fn a_line_moved_into_a_typed_folder_moves_after_the_folder_is_made() {
    let mut listing = loaded();
    edit(
        &mut listing,
        "alpha/\n  one.rs\n  two.rs\nbeta/\n  three.rs\nnew/\n  loose.rs",
    );
    assert_eq!(
        operations(&listing),
        vec![
            "create /project/new/",
            "move /project/loose.rs → /project/new/loose.rs",
        ]
    );
}

#[test]
fn a_moved_line_undone_by_hand_plans_nothing_again() {
    let mut listing = loaded();
    let original = listing.text();
    edit(
        &mut listing,
        "alpha/\n  one.rs\n  two.rs\nbeta/\n  three.rs\n  loose.rs",
    );
    assert!(listing.is_dirty());
    edit(&mut listing, &original);
    assert!(!listing.is_dirty());
    assert!(operations(&listing).is_empty());
}
