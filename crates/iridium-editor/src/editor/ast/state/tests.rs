//! Tests for the editor's retained syntax tree.
//!
//! Two of these are the point of the whole module:
//! [`folds_refresh_after_every_edit`], which is the bug this step exists to
//! fix, and [`a_missed_edit_still_leaves_a_correct_tree`], which is the promise
//! that the cheap path can never be the difference between right and wrong.

use iridium_syntax::{Language, SyntaxTree};

use super::SyntaxState;
use crate::document::{CursorState, Document, Position, Range, compute_edit_span};
use crate::history::Command;

/// The tree's shape as text, for comparing two parses of one document.
fn shape(state: &SyntaxState) -> String {
    state
        .tree()
        .expect("a synced state has a tree")
        .root_node()
        .to_sexp()
}

/// The shape a parse from scratch would produce for `source`.
fn shape_from_scratch(source: &str) -> String {
    let mut tree = SyntaxTree::new(Language::Rust).expect("rust parses");
    tree.parse(source).expect("a parse must produce a tree");
    tree.root().expect("a parsed tree has a root").to_sexp()
}

/// Applies `command` the way the editor does: span first, then edit, then
/// report.
fn apply(state: &mut SyntaxState, document: &mut Document, command: &Command) {
    let span = compute_edit_span(document, command)
        .expect("the command must resolve against this document");
    let mut cursor = CursorState::at(Position::zero());
    command
        .apply(document, &mut cursor)
        .expect("the command must apply");
    if let Some(span) = span {
        state.note_edit(document, &span);
    }
}

#[test]
fn a_state_with_no_language_has_nothing_to_say() {
    let mut state = SyntaxState::new();
    let document = Document::new("fn main() {}");

    assert!(state.language().is_none());
    assert!(state.tree().is_none());
    assert!(
        state.sync(&document).is_none(),
        "with no language there is no structure, and that is not an error"
    );
}

#[test]
fn syncing_parses_the_document_the_first_time() {
    let source = "fn main() {\n    let x = 1;\n}\n";
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    let document = Document::new(source);

    state.sync(&document).expect("rust source must parse");
    assert_eq!(shape(&state), shape_from_scratch(source));
}

#[test]
fn reported_edits_keep_the_tree_matching_a_parse_from_scratch() {
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    let mut document = Document::new("fn main() {}");
    state.sync(&document).expect("initial parse");

    for command in [
        Command::Insert {
            position: Position::new(0, 11),
            text: " ".to_string(),
        },
        Command::Insert {
            position: Position::new(0, 12),
            text: "let x = 1;".to_string(),
        },
        Command::Insert {
            position: Position::new(0, 22),
            text: " ".to_string(),
        },
    ] {
        apply(&mut state, &mut document, &command);
        state.sync(&document).expect("incremental parse");
        assert_eq!(
            shape(&state),
            shape_from_scratch(&document.text()),
            "the incremental tree diverged after {command:?}"
        );
    }
}

#[test]
fn a_missed_edit_still_leaves_a_correct_tree() {
    // The whole safety net: mutate the document without telling the state, and
    // it must notice from the revision alone and parse the document whole.
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    let mut document = Document::new("fn main() {}");
    state.sync(&document).expect("initial parse");

    let mut cursor = CursorState::at(Position::zero());
    Command::Insert {
        position: Position::new(0, 11),
        text: " let x = 1; ".to_string(),
    }
    .apply(&mut document, &mut cursor)
    .expect("the command must apply");

    state.sync(&document).expect("the fallback must parse");
    assert_eq!(
        shape(&state),
        shape_from_scratch(&document.text()),
        "an unreported edit produced a tree that disagrees with the document"
    );
}

#[test]
fn replacing_the_document_reparses_even_when_the_revision_repeats() {
    // A replacement document starts counting again, so revision equality is not
    // enough on its own — this is the case a comparison alone would miss.
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);

    let first = Document::new("fn main() {}");
    state.sync(&first).expect("initial parse");
    assert_eq!(first.revision(), 0, "a fresh document starts at zero");

    let second = Document::new("struct Different { field: u8 }");
    assert_eq!(second.revision(), first.revision());

    state.invalidate();
    state.sync(&second).expect("the replacement must parse");
    assert_eq!(shape(&state), shape_from_scratch(&second.text()));
}

#[test]
fn syncing_twice_over_an_unchanged_document_does_not_reparse() {
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    let document = Document::new("fn main() {\n    let x = 1;\n}\n");

    state.sync(&document).expect("initial parse");
    let after_first = (state.full_parses(), state.incremental_parses());
    state.sync(&document).expect("second sync");

    assert_eq!(
        (state.full_parses(), state.incremental_parses()),
        after_first,
        "an unchanged document was parsed again; every read of the tree would \
         pay for a parse it does not need"
    );
}

#[test]
fn a_reported_edit_reparses_incrementally_rather_than_wholly() {
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    let mut document = Document::new("fn main() {}");
    state.sync(&document).expect("initial parse");
    let full_before = state.full_parses();

    apply(
        &mut state,
        &mut document,
        &Command::Insert {
            position: Position::new(0, 11),
            text: " let x = 1; ".to_string(),
        },
    );
    assert_eq!(
        state.incremental_parses(),
        0,
        "reporting an edit must not parse; that is the whole point of it"
    );

    state.sync(&document).expect("incremental parse");
    assert_eq!(state.incremental_parses(), 1);
    assert_eq!(
        state.full_parses(),
        full_before,
        "a reported edit was answered with a full parse"
    );
}

#[test]
fn an_edit_spanning_lines_keeps_the_tree_matching_a_parse_from_scratch() {
    // The old end position must be the one the *pre-edit* document had. For an
    // edit inside one line the two coincide and any mistake hides; deleting
    // across lines is where a post-edit measurement goes wrong.
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    let mut document =
        Document::new("fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n");
    state.sync(&document).expect("initial parse");

    apply(
        &mut state,
        &mut document,
        &Command::Delete {
            range: Range::new(Position::new(1, 0), Position::new(3, 0)),
            deleted_text: "    let a = 1;\n    let b = 2;\n".to_string(),
        },
    );

    state.sync(&document).expect("incremental parse");
    assert_eq!(
        shape(&state),
        shape_from_scratch(&document.text()),
        "a multi-line deletion left the tree disagreeing with the document"
    );
}

#[test]
fn changing_the_language_discards_the_tree_parsed_with_the_old_one() {
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    let document = Document::new("fn main() {}");
    state.sync(&document).expect("initial parse");

    state.set_language(Language::Python);
    assert!(
        state.tree().is_none(),
        "a tree from another grammar describes nothing about this document"
    );

    state
        .sync(&document)
        .expect("reparse under the new language");
    assert_eq!(state.language(), Some(Language::Python));
}

#[test]
fn clearing_the_language_leaves_nothing_behind() {
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    let document = Document::new("fn main() {}");
    state.sync(&document).expect("initial parse");

    state.clear_language();
    assert!(state.language().is_none());
    assert!(state.tree().is_none());
    assert!(state.sync(&document).is_none());
}

#[test]
fn folds_refresh_after_every_edit() {
    // The bug this step exists to fix: `apply_command_internal` never refreshed
    // the fold regions, so the editor's folds were correct only until the first
    // keystroke. Before the fix this test sees one region after the edit —
    // the one detected when the content was set — instead of two.
    let mut editor = crate::editor::Editor::new(crate::editor::EditorConfig::default());
    editor.set_content("fn first() {\n    let a = 1;\n}\n");
    editor.set_language(Language::Rust);

    let before = editor.state().fold_state.regions().len();
    assert!(before >= 1, "the first function body is foldable");

    // Prepend a second function, which adds a foldable region.
    editor.apply_command(Command::Insert {
        position: Position::new(0, 0),
        text: "fn second() {\n    let b = 2;\n}\n".to_string(),
    });

    let after = editor.state().fold_state.regions().len();
    assert!(
        after > before,
        "folds went stale: {before} regions before the edit and {after} after, \
         with a whole second function added"
    );
}

#[test]
fn folds_stay_correct_when_an_edit_removes_a_region() {
    let mut editor = crate::editor::Editor::new(crate::editor::EditorConfig::default());
    editor.set_content("fn first() {\n    let a = 1;\n}\nfn second() {\n    let b = 2;\n}\n");
    editor.set_language(Language::Rust);
    let before = editor.state().fold_state.regions().len();

    // Delete the second function entirely.
    let start = Position::new(3, 0);
    let end = Position::new(6, 0);
    editor.apply_command(Command::Delete {
        range: Range::new(start, end),
        deleted_text: "fn second() {\n    let b = 2;\n}\n".to_string(),
    });

    let after = editor.state().fold_state.regions().len();
    assert!(
        after < before,
        "removing a function must remove its fold: {before} before, {after} after"
    );
}

#[test]
fn replacing_the_editors_content_refolds_the_new_document() {
    // Both documents are fresh, so both start at revision zero: without an
    // explicit invalidation the second one looks unchanged and the folds stay
    // those of the first. This is that case, driven through the editor.
    let mut editor = crate::editor::Editor::new(crate::editor::EditorConfig::default());
    editor.set_language(Language::Rust);
    editor.set_content("fn only() {\n    let a = 1;\n}\n");
    let one = editor.state().fold_state.regions().len();

    editor.set_content("fn first() {\n    let a = 1;\n}\nfn second() {\n    let b = 2;\n}\n");
    let two = editor.state().fold_state.regions().len();

    assert!(
        two > one,
        "the folds still describe the previous document: {one} region(s) for one \
         function, {two} for two"
    );
}
