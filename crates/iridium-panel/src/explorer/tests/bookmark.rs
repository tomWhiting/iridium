//! Reopening uses live paths, never saved directory entries or node ids.
use super::support::*;
use crate::explorer::{ExplorerOutcome, FileExplorer};
use iridium_editor::KeyCode;
use std::fs;

fn reopen(explorer: &FileExplorer) -> FileExplorer {
    let mut next = FileExplorer::open_bookmark(&explorer.bookmark()).expect("reader");
    settle(&mut next);
    assert!(!next.is_waiting());
    next
}

#[test]
fn nested_selection_expansions_and_query_survive_fresh_reads() {
    let directory = project();
    let mut explorer = opened(&directory);
    open_directory(&mut explorer, "engine/");
    open_directory(&mut explorer, "widgets/");
    explorer.paste("mod.rs");
    settle(&mut explorer);
    explorer.handle_key(&press(KeyCode::End));
    let selected = explorer.selected_path().expect("selected").to_path_buf();
    let mut next = reopen(&explorer);
    assert_eq!(next.query.text(), "mod.rs");
    assert_eq!(next.selected_path(), Some(selected.as_path()));
    next.handle_key(&press(KeyCode::Escape));
    let screen = all_lines(&mut next).join("\n");
    assert!(screen.contains("render.rs"), "{screen}");
    assert!(screen.contains("button.rs"), "{screen}");
}

#[test]
fn deleted_selection_and_new_files_are_reflected_after_reopen() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.paste(".rs");
    settle(&mut explorer);
    explorer.handle_key(&press(KeyCode::End));
    let selected = explorer.selected_path().expect("selected").to_path_buf();
    fs::remove_file(&selected).expect("delete");
    fs::write(directory.path().join("fresh.rs"), "new").expect("create");
    let mut next = reopen(&explorer);
    assert_ne!(next.selected_path(), Some(selected.as_path()));
    assert!(next.files.node_at(&selected).is_none());
    assert!(all_lines(&mut next).join("\n").contains("fresh.rs"));
}

#[test]
fn unfiltered_nested_focus_is_restored_without_crawling() {
    let directory = project();
    let mut explorer = opened_with_crawl(&directory, false);
    open_directory(&mut explorer, "engine/");
    select_row(&mut explorer, "render.rs");
    let next = reopen(&explorer);
    assert_eq!(next.selected_path(), explorer.selected_path());
    assert!(!next.crawl);
    assert!(
        !next.files.is_listed(
            next.files
                .node_at(&directory.path().join("widgets"))
                .expect("folder")
        )
    );
}

#[test]
fn input_cancels_pending_focus_restoration() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.paste("mod.rs");
    settle(&mut explorer);
    explorer.handle_key(&press(KeyCode::End));
    let mut next = FileExplorer::open_bookmark(&explorer.bookmark()).expect("reader");
    next.handle_key(&press(KeyCode::Home));
    assert!(next.restoring.is_none());
    settle(&mut next);
    assert_ne!(next.selected_path(), explorer.selected_path());
    assert_eq!(
        next.selected_path(),
        Some(directory.path().join("engine/mod.rs").as_path())
    );
}

#[test]
fn reopening_again_before_reads_land_keeps_the_original_intent() {
    let directory = project();
    let mut explorer = opened_with_crawl(&directory, false);
    open_directory(&mut explorer, "engine/");
    select_row(&mut explorer, "render.rs");
    let pending = FileExplorer::open_bookmark(&explorer.bookmark()).expect("reader");
    let next = reopen(&pending);
    assert_eq!(next.selected_path(), explorer.selected_path());
}

#[test]
fn hidden_visibility_and_collapsed_root_survive() {
    let directory = project();
    fs::write(directory.path().join(".hidden"), "hidden").expect("fixture");
    let mut explorer = opened(&directory);
    explorer.toggle_hidden();
    select_row(&mut explorer, ".hidden");
    let mut next = reopen(&explorer);
    assert!(next.files.show_hidden());
    assert_eq!(next.selected_path(), explorer.selected_path());
    next.handle_key(&press(KeyCode::Home));
    next.handle_key(&press(KeyCode::Left));
    let again = reopen(&next);
    assert_eq!(again.tree.len(), 1);
    assert!(!again.tree.is_expanded(&again.files.root()));
}

#[test]
fn clean_oil_session_reopens_as_a_fresh_browse_listing() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.handle_key(&press(KeyCode::Tab));
    assert!(explorer.mode.is_editing());
    assert!(!explorer.has_unapplied_edits());
    let next = reopen(&explorer);
    assert!(!next.mode.is_editing());
}

#[test]
fn invalid_and_empty_queries_remain_visible_after_reopen() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.paste("/[");
    let mut next = reopen(&explorer);
    assert!(all_lines(&mut next).join("\n").contains("Bad pattern"));
    next.handle_key(&press(KeyCode::Escape));
    next.paste("zzzzzz");
    let mut empty = reopen(&next);
    assert!(
        all_lines(&mut empty)
            .join("\n")
            .contains("No matching files")
    );
    assert!(matches!(
        empty.handle_key(&press(KeyCode::Tab)),
        ExplorerOutcome::Failed(_)
    ));
}

#[test]
fn missing_root_finishes_loading_and_reports_the_read_error() {
    let directory = project();
    let explorer = opened(&directory);
    fs::remove_dir_all(directory.path()).expect("remove root");
    let mut next = reopen(&explorer);
    assert!(
        next.files
            .info(next.files.root())
            .expect("root")
            .error
            .is_some()
    );
    assert!(all_lines(&mut next).join("\n").contains("Cannot read"));
    next.paste("anything");
    assert!(all_lines(&mut next).join("\n").contains("Cannot read"));
    assert!(matches!(
        next.handle_key(&press(KeyCode::Tab)),
        ExplorerOutcome::Failed(_)
    ));
}

#[test]
fn filtered_focus_under_a_collapsed_folder_survives_without_crawling() {
    let directory = project();
    let mut explorer = opened_with_crawl(&directory, false);
    open_directory(&mut explorer, "engine/");
    explorer.handle_key(&press(KeyCode::Home));
    explorer.handle_key(&press(KeyCode::Left));
    explorer.paste("render.rs");
    settle(&mut explorer);
    let next = reopen(&explorer);
    assert_eq!(
        next.selected_path(),
        Some(directory.path().join("engine/render.rs").as_path())
    );
    assert_eq!(next.tree.len(), 1, "the root stays collapsed");
    let widgets = next
        .files
        .node_at(&directory.path().join("widgets"))
        .expect("folder");
    assert!(
        !next.files.is_listed(widgets),
        "unrelated folders are not crawled"
    );
}
