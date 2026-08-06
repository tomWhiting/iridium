//! What a query causes to be *read*.
//!
//! Split from [`super::filter`] by the question a defect here presents as.
//! That file is about the rows a query produces from what has already been
//! read; this one is about the crawl behind them — which directories get
//! asked for, which are left alone, and what the panel says while it is
//! still looking.

use iridium_file::test_support::TempDir;

use super::support::{
    lines, open_directory, opened, opened_with_crawl, press, project, select_row, selected_text,
    settle, type_query,
};
use iridium_editor::KeyCode;

#[test]
fn a_query_reaches_into_folders_nobody_opened() {
    // **The point of the crawl.** The filter itself walks only what has been
    // read; the query is what makes more of it get read. Nothing here is
    // opened by hand.
    let directory = project();
    let mut explorer = opened(&directory);
    assert_eq!(
        lines(&mut explorer).len(),
        4,
        "the root's own listing and nothing below it"
    );

    type_query(&mut explorer, "button");
    settle(&mut explorer);

    let rows = lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("button.rs")),
        "a file two levels down, in a folder never expanded: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("widgets/")),
        "and its folder above it, as context: {rows:?}"
    );
}

#[test]
fn a_panel_that_will_not_crawl_reads_nothing_a_query_asks_for() {
    // **The mirror of the test above, and the whole point of the flag.**
    // Opened at a root nobody chose — the `/` a Finder launch hands a bundle,
    // or a home directory — a query must not set off across the disk. The
    // same fixture, the same query, the same file two levels down: this time
    // it stays unread.
    let directory = project();
    let mut explorer = opened_with_crawl(&directory, false);

    type_query(&mut explorer, "button");
    // Enough frames that a crawl of this fixture would have finished several
    // times over, so "nothing was read" is a statement about the flag rather
    // than about how quickly the assertion ran.
    for _ in 0..8 {
        explorer.poll();
    }

    // One observable, two ways to fail it, and the message names both: the
    // panel either posted a read it was told not to post, or `is_waiting`
    // forgot the flag and is asking for frames forever over a frontier
    // nothing will ever drain.
    assert!(
        !explorer.is_waiting(),
        "a panel that will never crawl is still asking for frames under a query"
    );
    let rows = lines(&mut explorer);
    assert!(
        !rows.iter().any(|row| row.contains("button.rs")),
        "a folder nobody opened was read anyway: {rows:?}"
    );
    // And it names the scope it searched. "No matching files" here would be
    // a claim about the disk that this panel never went and checked.
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].contains("No matches in what is open"),
        "{:?}",
        rows[0]
    );
}

#[test]
fn the_crawl_skips_what_the_project_says_to_ignore() {
    let directory = TempDir::new("explorer-ignored");
    let root = directory.path();
    std::fs::write(root.join(".gitignore"), "build/\n").expect("the fixture was written");
    std::fs::create_dir(root.join("build")).expect("the fixture directory was made");
    std::fs::write(root.join("build/artefact.rs"), "a").expect("the fixture was written");
    std::fs::create_dir(root.join("src")).expect("the fixture directory was made");
    std::fs::write(root.join("src/artefact.rs"), "a").expect("the fixture was written");

    let mut explorer = opened(&directory);
    type_query(&mut explorer, "artefact");
    settle(&mut explorer);

    let rows = lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("src/")),
        "the source copy is found: {rows:?}"
    );
    assert!(
        rows.iter().all(|row| !row.contains("build/")),
        "the ignored copy is not, and neither is the folder holding it: {rows:?}"
    );
}

#[test]
fn an_ignored_folder_still_opens_when_someone_asks_for_it_by_hand() {
    // The rule is about the *crawl*, not about the tree. A file tree that hid
    // a build directory would be lying about the disk, and opening a
    // generated file is a legitimate thing to want.
    let directory = TempDir::new("explorer-ignored-manual");
    let root = directory.path();
    std::fs::write(root.join(".gitignore"), "build/\n").expect("the fixture was written");
    std::fs::create_dir(root.join("build")).expect("the fixture directory was made");
    std::fs::write(root.join("build/artefact.rs"), "a").expect("the fixture was written");

    let mut explorer = opened(&directory);
    let rows = lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("build/")),
        "the folder is listed like any other: {rows:?}"
    );

    open_directory(&mut explorer, "build/");
    let rows = lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("artefact.rs")),
        "and opens like any other: {rows:?}"
    );
}

#[test]
fn a_deeper_ignore_file_overrides_the_one_above_it() {
    // The reason each directory's rules are compiled against their own base
    // rather than all thrown into one matcher. A single root-anchored matcher
    // gets this wrong, and gets it wrong silently.
    let directory = TempDir::new("explorer-ignore-nested");
    let root = directory.path();
    std::fs::write(root.join(".gitignore"), "logs/\n").expect("the fixture was written");
    std::fs::create_dir(root.join("pkg")).expect("the fixture directory was made");
    std::fs::write(root.join("pkg/.gitignore"), "!logs/\n").expect("the fixture was written");
    std::fs::create_dir(root.join("pkg/logs")).expect("the fixture directory was made");
    std::fs::write(root.join("pkg/logs/kept.rs"), "k").expect("the fixture was written");
    std::fs::create_dir(root.join("logs")).expect("the fixture directory was made");
    std::fs::write(root.join("logs/dropped.rs"), "d").expect("the fixture was written");

    let mut explorer = opened(&directory);
    type_query(&mut explorer, "rs");
    settle(&mut explorer);

    let rows = lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("kept.rs")),
        "the nested negation re-included its folder: {rows:?}"
    );
    assert!(
        rows.iter().all(|row| !row.contains("dropped.rs")),
        "and the root rule still holds where nothing overrode it: {rows:?}"
    );
}

#[test]
fn the_crawl_never_walks_into_dot_git() {
    // No `.gitignore` names `.git`, because git does not need telling. A
    // crawl that took that literally would read every object directory in the
    // repository.
    let directory = TempDir::new("explorer-dot-git");
    let root = directory.path();
    std::fs::create_dir(root.join(".git")).expect("the fixture directory was made");
    std::fs::create_dir(root.join(".git/objects")).expect("the fixture directory was made");
    std::fs::write(root.join(".git/objects/marker.rs"), "m").expect("the fixture was written");

    let mut explorer = opened(&directory);
    type_query(&mut explorer, "marker");
    settle(&mut explorer);

    let rows = lines(&mut explorer);
    assert!(
        rows.iter().all(|row| !row.contains("marker.rs")),
        "{rows:?}"
    );
}

#[test]
fn an_empty_list_says_it_is_still_looking_before_it_says_there_is_nothing() {
    // Three statements, three meanings. "No matching files" while the crawl
    // is still reading is a lie that resolves itself a second later — which
    // is exactly long enough for someone to have given up.
    let directory = project();
    let mut explorer = opened(&directory);

    type_query(&mut explorer, "zzzz");
    let rows = lines(&mut explorer);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].contains("Searching…"),
        "the crawl has folders left to read: {:?}",
        rows[0]
    );

    settle(&mut explorer);
    let rows = lines(&mut explorer);
    assert!(
        rows[0].contains("No matching files"),
        "everything has been read, so there really is nothing: {:?}",
        rows[0]
    );
}

#[test]
fn nothing_is_crawled_until_something_is_typed() {
    // The crawl is the query's, not the panel's. Opening the explorer to look
    // at one folder must not read the project.
    let directory = project();
    let mut explorer = opened(&directory);

    for _ in 0..8 {
        explorer.poll();
    }
    assert!(
        !explorer.is_waiting(),
        "an idle panel posted a read it was never asked for"
    );
    assert_eq!(
        lines(&mut explorer).len(),
        4,
        "and the rows are still only what the root listing brought"
    );
}

#[test]
fn a_read_landing_under_a_query_does_not_move_the_selection_off_its_row() {
    let directory = project();
    let mut explorer = opened(&directory);
    open_directory(&mut explorer, "widgets/");

    // Post `engine/`'s read without waiting for it, so it lands under the
    // query.
    select_row(&mut explorer, "engine/");
    explorer.handle_key(&press(KeyCode::Right));

    type_query(&mut explorer, "mod");
    let before = selected_text(&mut explorer).expect("a hit is selected");
    assert!(before.contains("mod.rs"), "{before:?}");
    let rows_before = lines(&mut explorer).len();

    // `engine/mod.rs` arrives, ties with `widgets/mod.rs` on name and wins
    // the tie by sorting first — so the *best* match moves. The selection
    // must not, because a keypress may already be on its way to `Enter`, and
    // a background read is not a reason to open a different file.
    settle(&mut explorer);
    assert!(
        lines(&mut explorer).len() > rows_before,
        "the second folder's listing landed"
    );
    assert_eq!(
        selected_text(&mut explorer),
        Some(before),
        "the selection stayed on the row the user left it on"
    );
}
