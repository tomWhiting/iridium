//! The editable buffer against a real project.
//!
//! [`super::super::buffer_tests`] proves the arithmetic without a disk. This
//! proves the join: that the rows the panel is actually drawing — from either
//! source, over a real directory read on the real worker thread — produce a
//! buffer whose nesting is the nesting on the filesystem.
//!
//! # The oracle is the plan itself, and it has a hole worth naming
//!
//! A row's folder is derived from the indentation it was drawn at, and every
//! path is computed by joining that derived chain. [`super::super::plan`]
//! refuses any row whose drawn folder is not the folder its origin actually
//! lives in — so a plan that comes back `Ok` says the derived nesting agreed
//! with the filesystem. That does not have to be rewritten when the fixture
//! grows, which is why it is the oracle here.
//!
//! **It does not catch a row wrongly given no folder at all.** Such a row is
//! its own target, so its subtree still resolves and the plan still says
//! `Ok`. Flattening every depth by one was tried against these tests and
//! `a_real_project_is_nested_the_way_it_really_is` passed it. What catches it
//! is [`the_root_is_the_only_row_with_no_folder_above_it`], which is here for
//! that reason and not for tidiness.

use iridium_editor::KeyCode;

use super::support::{lines, open_directory, opened, press, project, settle, type_query};
use crate::file_tree::plan::Operation;

#[test]
fn the_buffer_holds_every_row_the_panel_is_drawing() {
    let directory = project();
    let mut explorer = opened(&directory);
    open_directory(&mut explorer, "engine");

    let drawn = lines(&mut explorer);
    let buffer = explorer.buffer();

    assert_eq!(
        buffer.rows().len(),
        drawn.len(),
        "the buffer and the screen disagree about how many rows there are:\n{drawn:?}"
    );
    for (row, line) in buffer.rows().iter().zip(&drawn) {
        assert!(
            line.contains(&row.name),
            "the buffer has {:?} where the screen draws {line:?}",
            row.name
        );
    }
}

#[test]
fn a_real_project_is_nested_the_way_it_really_is() {
    // The load-bearing test of the whole join. `plan` refuses a row whose
    // drawn folder is not the folder it lives in, so an untouched buffer
    // planning `Ok` proves the derivation agreed with the filesystem for
    // every row — including after a subdirectory was opened, which is where a
    // depth is first read from a listing rather than from the root.
    let directory = project();
    let mut explorer = opened(&directory);
    open_directory(&mut explorer, "engine");
    open_directory(&mut explorer, "widgets");

    let buffer = explorer.buffer();
    let plan = buffer
        .plan()
        .unwrap_or_else(|refusals| panic!("a real project was refused: {refusals:?}"));

    assert!(
        plan.is_empty(),
        "an untouched buffer planned work: {:?}",
        plan.operations
    );
}

#[test]
fn renaming_a_row_plans_a_rename_of_the_file_that_row_draws() {
    // End to end, against a path that really exists: the operation must name
    // the file the row was drawn from, not a path reconstructed from a name.
    let directory = project();
    let mut explorer = opened(&directory);
    open_directory(&mut explorer, "engine");

    let mut buffer = explorer.buffer();
    let index = buffer
        .rows()
        .iter()
        .position(|row| row.name == "render.rs")
        .expect("the fixture drew engine/render.rs");
    buffer.rename(index, "renderer.rs");

    let plan = buffer
        .plan()
        .unwrap_or_else(|refusals| panic!("the rename was refused: {refusals:?}"));

    assert_eq!(
        plan.operations,
        vec![Operation::Rename {
            from: directory.path().join("engine/render.rs"),
            to: directory.path().join("engine/renderer.rs"),
        }]
    );
}

#[test]
fn striking_a_row_plans_a_delete_of_the_path_on_disk() {
    let directory = project();
    let explorer = opened(&directory);

    let mut buffer = explorer.buffer();
    let index = buffer
        .rows()
        .iter()
        .position(|row| row.name == "README.md")
        .expect("the fixture drew README.md");
    buffer.toggle_deleted(index);

    let plan = buffer
        .plan()
        .unwrap_or_else(|refusals| panic!("the delete was refused: {refusals:?}"));

    assert_eq!(
        plan.operations,
        vec![Operation::Delete {
            path: directory.path().join("README.md"),
        }]
    );
}

#[test]
fn a_filtered_buffer_holds_only_what_the_query_left_on_screen() {
    // Ruling 4, and it is not implemented anywhere — it falls out of the
    // buffer reading whatever is drawn. A file the query filtered away is not
    // in the buffer, so it cannot be renamed, struck through, or planned
    // against by anything downstream.
    let directory = project();
    let mut explorer = opened(&directory);
    type_query(&mut explorer, "button");
    settle(&mut explorer);

    let buffer = explorer.buffer();
    let names: Vec<&str> = buffer.rows().iter().map(|row| row.name.as_str()).collect();

    assert!(
        names.contains(&"button.rs"),
        "the match itself is missing: {names:?}"
    );
    assert!(
        !names.contains(&"README.md") && !names.contains(&"render.rs"),
        "the buffer holds a file the query took off the screen: {names:?}"
    );
}

#[test]
fn a_filtered_buffer_keeps_the_folders_the_matches_live_in() {
    // The other half of ruling 8: the filter keeps the hierarchy, so a match
    // drawn under its folder is *nested* under it in the buffer too — and the
    // rename is therefore planned inside that folder rather than beside it.
    let directory = project();
    let mut explorer = opened(&directory);
    type_query(&mut explorer, "button");
    settle(&mut explorer);

    let mut buffer = explorer.buffer();
    let index = buffer
        .rows()
        .iter()
        .position(|row| row.name == "button.rs")
        .expect("the query matched widgets/button.rs");
    buffer.rename(index, "pressable.rs");

    let plan = buffer
        .plan()
        .unwrap_or_else(|refusals| panic!("a filtered rename was refused: {refusals:?}"));

    assert_eq!(
        plan.operations,
        vec![Operation::Rename {
            from: directory.path().join("widgets/button.rs"),
            to: directory.path().join("widgets/pressable.rs"),
        }]
    );
}

#[test]
fn the_buffer_follows_the_panel_back_out_of_a_filter() {
    // Clearing the query restores the tree that was there, and the buffer is
    // rebuilt from whatever is showing rather than remembering a view.
    let directory = project();
    let mut explorer = opened(&directory);
    let before = explorer.buffer().rows().len();

    type_query(&mut explorer, "button");
    settle(&mut explorer);
    assert_ne!(
        explorer.buffer().rows().len(),
        before,
        "the filter did not change what is on screen, so this proves nothing"
    );

    for _ in 0.."button".len() {
        explorer.handle_key(&press(KeyCode::Backspace));
    }
    settle(&mut explorer);

    assert_eq!(explorer.buffer().rows().len(), before);
}

#[test]
fn the_root_is_the_only_row_with_no_folder_above_it() {
    // What makes the buffer plannable at all: `plan` treats a row with no
    // parent as the mount point and refuses to rename or delete it, so
    // exactly one row must be in that position.
    let directory = project();
    let mut explorer = opened(&directory);
    open_directory(&mut explorer, "engine");

    let buffer = explorer.buffer();
    let rootless: Vec<&str> = buffer
        .rows()
        .iter()
        .filter(|row| row.parent.is_none())
        .map(|row| row.name.as_str())
        .collect();

    assert_eq!(
        rootless.len(),
        1,
        "more than one row has no folder above it: {rootless:?}"
    );
    assert_eq!(
        buffer.rows().first().map(|row| row.parent),
        Some(None),
        "the row with no folder above it is not the first one"
    );
}
