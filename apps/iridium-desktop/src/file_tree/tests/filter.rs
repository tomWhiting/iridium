//! What a query does to the rows.
//!
//! Only the rows: which of them survive a query, where they are drawn, what
//! the selection and the keys do to them. What a query causes to be *read*
//! is [`super::crawl`]'s question.

use iridium_editor::KeyCode;
use iridium_editor::theme::Theme;
use iridium_file::test_support::TempDir;

use super::support::{
    FIT, all_lines, lines, open_directory, opened, press, project, select_row, selected_row,
    selected_text, settle, type_query,
};
use crate::file_tree::{ExplorerOutcome, FileExplorer};

/// The fixture with both directories read, so the filter has them to work
/// with.
fn opened_project(directory: &TempDir) -> FileExplorer {
    let mut explorer = opened(directory);
    open_directory(&mut explorer, "engine/");
    open_directory(&mut explorer, "widgets/");
    explorer
}

#[test]
fn a_match_is_drawn_where_it_lives_and_not_in_a_flat_list() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "button");
    let rows = lines(&mut explorer);

    // The folder is context, indented above its one hit. A flat list would
    // have shown `button.rs` alone, and two files of the same name in
    // different folders would then have been indistinguishable.
    assert_eq!(rows.len(), 3, "root, folder, hit: {rows:?}");
    assert!(rows[0].starts_with("▾ "), "the root: {:?}", rows[0]);
    assert!(rows[1].contains("widgets/"), "the folder: {:?}", rows[1]);
    assert!(rows[2].contains("button.rs"), "the hit: {:?}", rows[2]);
    assert!(
        rows[2].starts_with("    "),
        "the hit keeps its indent under the folder: {:?}",
        rows[2]
    );
}

#[test]
fn a_folder_with_nothing_matching_inside_it_disappears() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "render");
    let rows = lines(&mut explorer);

    assert!(
        rows.iter().all(|row| !row.contains("widgets/")),
        "a folder is kept only for what is inside it: {rows:?}"
    );
    assert!(rows.iter().any(|row| row.contains("engine/")));
    assert!(rows.iter().any(|row| row.contains("render.rs")));
}

#[test]
fn two_files_of_the_same_name_are_told_apart_by_the_folders_above_them() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "mod");
    let rows = lines(&mut explorer);

    // Both hits, each under its own folder. This is the whole argument for
    // keeping the hierarchy, stated as a test.
    assert_eq!(
        rows.iter().filter(|row| row.contains("mod.rs")).count(),
        2,
        "{rows:?}"
    );
    assert!(rows.iter().any(|row| row.contains("engine/")));
    assert!(rows.iter().any(|row| row.contains("widgets/")));
}

#[test]
fn typing_part_of_a_folder_name_finds_what_is_inside_it() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    // `wid` names no file. It matches the folder, and the folder's own
    // children match it through their paths.
    type_query(&mut explorer, "wid");
    let rows = lines(&mut explorer);
    assert!(rows.iter().any(|row| row.contains("widgets/")), "{rows:?}");
    assert!(rows.iter().any(|row| row.contains("button.rs")), "{rows:?}");
    assert!(
        rows.iter().all(|row| !row.contains("engine/")),
        "and nothing from the folder that does not match: {rows:?}"
    );
}

#[test]
fn the_selection_lands_on_the_best_match_and_not_on_the_first_row() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "readme");
    // Row 0 is the root, which matched nothing. The selection is on the file
    // the query names — a panel that selected the first row would need an
    // arrow press before `Enter` did the obvious thing.
    let selected = selected_text(&mut explorer).expect("something is selected");
    assert!(selected.contains("README.md"), "{selected:?}");
}

#[test]
fn the_arrows_step_between_matches_and_skip_the_folders_holding_them() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "mod");
    let rows = lines(&mut explorer);
    let hits: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.contains("mod.rs"))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(hits.len(), 2, "{rows:?}");

    explorer.handle_key(&press(KeyCode::Home));
    assert_eq!(
        selected_row(&mut explorer),
        Some(hits[0]),
        "Home goes to the first *match*, not to the root above it"
    );
    explorer.handle_key(&press(KeyCode::Down));
    assert_eq!(
        selected_row(&mut explorer),
        Some(hits[1]),
        "one press crosses the folder row between the two hits"
    );
    explorer.handle_key(&press(KeyCode::Down));
    assert_eq!(
        selected_row(&mut explorer),
        Some(hits[1]),
        "the selection clamps at the last match rather than wrapping"
    );
    explorer.handle_key(&press(KeyCode::Up));
    assert_eq!(selected_row(&mut explorer), Some(hits[0]));
    explorer.handle_key(&press(KeyCode::Up));
    assert_eq!(
        selected_row(&mut explorer),
        Some(hits[0]),
        "and clamps at the first"
    );
}

#[test]
fn enter_on_a_filtered_file_opens_it() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "button");
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Enter)),
        ExplorerOutcome::Open(directory.path().join("widgets/button.rs"))
    );
}

#[test]
fn enter_on_a_filtered_folder_drops_the_query_and_opens_the_tree_to_it() {
    let directory = project();
    let mut explorer = opened_project(&directory);
    // Collapse it first, so the reveal has something to do.
    select_row(&mut explorer, "widgets/");
    explorer.handle_key(&press(KeyCode::Left));

    type_query(&mut explorer, "widgets");
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Enter)),
        ExplorerOutcome::Handled,
        "a folder is not a tab"
    );

    let rows = all_lines(&mut explorer);
    assert_eq!(
        rows.first().map(String::as_str),
        Some("> "),
        "the query is gone: {rows:?}"
    );
    let selected = selected_text(&mut explorer).expect("the revealed folder is selected");
    assert!(
        selected.contains("widgets/"),
        "the selection is on the folder that was entered: {selected:?}"
    );
}

#[test]
fn clearing_the_query_restores_exactly_the_tree_that_was_open() {
    let directory = project();
    let mut explorer = opened(&directory);
    // One folder open, one closed — a state a filter must not quietly change.
    open_directory(&mut explorer, "engine/");
    let before = lines(&mut explorer);

    type_query(&mut explorer, "button");
    assert_ne!(lines(&mut explorer), before, "the query narrowed something");

    for _ in 0.."button".len() {
        explorer.handle_key(&press(KeyCode::Backspace));
    }
    assert_eq!(
        lines(&mut explorer),
        before,
        "filtering reads the tree and never writes to it"
    );
}

#[test]
fn escape_takes_back_the_query_before_it_takes_back_the_panel() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "mod");
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Escape)),
        ExplorerOutcome::Handled,
        "the first Escape clears the query"
    );
    assert_eq!(
        all_lines(&mut explorer).first().map(String::as_str),
        Some("> ")
    );
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Escape)),
        ExplorerOutcome::Closed,
        "the second closes the panel"
    );
}

#[test]
fn a_query_that_matches_nothing_says_so_rather_than_showing_the_tree() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "zzzz");
    // The crawl has to finish before "nothing matched" is a true statement
    // rather than a premature one — which is the distinction
    // `an_empty_list_says_it_is_still_looking_before_it_says_there_is_nothing`
    // pins.
    settle(&mut explorer);
    let rows = lines(&mut explorer);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].contains("No matching files"), "{:?}", rows[0]);

    // And `Enter` on nothing does nothing, rather than opening whatever the
    // selection happened to be pointing at before the query.
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Enter)),
        ExplorerOutcome::Handled
    );
}

#[test]
fn the_characters_the_query_matched_are_drawn_in_the_match_colour() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "but");
    let content = explorer.content(&Theme::dark(), FIT);
    let row = content
        .rows
        .iter()
        .find(|row| {
            row.spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
                .contains("button.rs")
        })
        .expect("the hit is on screen");

    // A row drawn as one run is a row with nothing underlined, and a filter
    // that shows no trace of what it matched reads as a list that changed for
    // no reason.
    let colours: Vec<_> = row.spans.iter().map(|span| span.color).collect();
    assert!(
        colours.windows(2).any(|pair| pair[0] != pair[1]),
        "the matched characters are a run of their own: {:?}",
        row.spans
    );
}

#[test]
fn a_leading_slash_makes_the_query_a_regular_expression() {
    let directory = project();
    let mut explorer = opened_project(&directory);

    // `.` is a wildcard here and a literal in a fuzzy query, so this pattern
    // matching `button.rs` is only possible in the mode the sigil selects.
    type_query(&mut explorer, "/^butt.n");
    let rows = lines(&mut explorer);
    assert!(rows.iter().any(|row| row.contains("button.rs")), "{rows:?}");
    assert!(
        rows.iter().all(|row| !row.contains("render.rs")),
        "and nothing the pattern did not match: {rows:?}"
    );
}

#[test]
fn a_regular_expression_keeps_the_hierarchy_a_fuzzy_query_keeps() {
    // The sigil chooses how rows are *matched*. It does not choose how they
    // are drawn, and a regex result that arrived as a flat list would be a
    // second kind of thing appearing in the same panel.
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "/button");
    let rows = lines(&mut explorer);
    assert_eq!(rows.len(), 3, "root, folder, hit: {rows:?}");
    assert!(rows[1].contains("widgets/"), "the folder: {:?}", rows[1]);
    assert!(rows[2].contains("button.rs"), "the hit: {:?}", rows[2]);
    assert!(
        rows[2].starts_with("    "),
        "still indented under it: {:?}",
        rows[2]
    );
}

#[test]
fn the_sigil_on_its_own_leaves_the_tree_exactly_as_it_was() {
    // **The one that would have been a real defect.** An empty regular
    // expression matches every string, so a bare `/` treated as a pattern
    // would flatten the whole tree into the result list the instant the key
    // was pressed — before the user had typed anything to search for.
    let directory = project();
    let mut explorer = opened_project(&directory);
    let before = lines(&mut explorer);

    type_query(&mut explorer, "/");
    assert_eq!(
        lines(&mut explorer),
        before,
        "pressing the key that means `a pattern is coming` is not a filter"
    );
}

#[test]
fn a_pattern_that_does_not_compile_says_why_rather_than_saying_nothing_matched() {
    // `/[` is two keystrokes into `/[a-z]`. "No matching files" would be a
    // claim about the project; the truth is about the pattern.
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "/[");
    let rows = lines(&mut explorer);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].starts_with("Bad pattern — "),
        "the reason, not a verdict on the files: {:?}",
        rows[0]
    );
    assert!(
        !rows[0].contains("No matching files") && !rows[0].contains("Searching"),
        "{:?}",
        rows[0]
    );
}

#[test]
fn finishing_a_half_typed_pattern_filters_without_anything_being_retyped() {
    // The rejection is a state, not an error to dismiss: the next character
    // is all it takes to leave it.
    let directory = project();
    let mut explorer = opened_project(&directory);

    type_query(&mut explorer, "/[b");
    assert!(lines(&mut explorer)[0].starts_with("Bad pattern — "));

    type_query(&mut explorer, "]");
    let rows = lines(&mut explorer);
    assert!(rows.iter().any(|row| row.contains("button.rs")), "{rows:?}");
}

#[test]
fn escape_takes_back_a_regular_expression_query_like_any_other() {
    let directory = project();
    let mut explorer = opened_project(&directory);
    let before = lines(&mut explorer);

    type_query(&mut explorer, "/button");
    assert_ne!(lines(&mut explorer), before);

    assert_eq!(
        explorer.handle_key(&press(KeyCode::Escape)),
        ExplorerOutcome::Handled,
        "the query goes before the panel does"
    );
    assert_eq!(lines(&mut explorer), before);
}
