//! Tests for fold state, folding operations and the line mapping.

mod history_replay;
#[cfg(not(feature = "syntax"))]
mod without_a_grammar;

use std::borrow::Cow;

use super::*;

/// Parses `source` and refreshes `state`'s regions from the result.
///
/// The detector borrows a tree rather than owning one, so a test that wants
/// regions has to parse first — exactly as the editor does.
fn update_regions_of(state: &mut FoldState, source: &str) -> bool {
    #[cfg(not(feature = "syntax"))]
    use crate::syntax_stubs::SyntaxTree;
    #[cfg(feature = "syntax")]
    use iridium_syntax::SyntaxTree;

    let Some(language) = state.language() else {
        return false;
    };
    let Ok(mut tree) = SyntaxTree::new(language) else {
        return false;
    };
    let Some(parsed) = tree.parse(source) else {
        return false;
    };
    // A fresh parse of a fresh tree: nothing about the previous one is known,
    // which is exactly what `Full` says.
    state.update_regions(parsed, &SyntaxDelta::Full, || Cow::Borrowed(source))
}

fn setup_rust_fold_state(source: &str) -> FoldState {
    let mut state = FoldState::for_language(Language::Rust);
    update_regions_of(&mut state, source);
    state
}

#[test]
fn basic_fold_unfold() {
    let source = "fn main() {\n    println!(\"hello\");\n}";
    let mut state = setup_rust_fold_state(source);

    // Should have detected the function
    assert!(!state.regions().is_empty());
    assert!(state.is_foldable(0));
    assert!(!state.is_folded(0));

    // Fold it
    assert!(state.fold_at(0));
    assert!(state.is_folded(0));

    // Line 1 should be hidden
    assert!(state.is_line_hidden(1));
    assert!(!state.is_line_hidden(0)); // Start line is never hidden

    // Unfold it
    assert!(state.unfold_at(0));
    assert!(!state.is_folded(0));
    assert!(!state.is_line_hidden(1));
}

#[test]
fn fold_all_unfold_all() {
    let source = r#"fn foo() {
    println!("foo");
}

fn bar() {
    println!("bar");
}"#;
    let mut state = setup_rust_fold_state(source);

    state.fold_all();
    assert!(state.is_folded(0));
    assert!(state.is_folded(4));

    state.unfold_all();
    assert!(!state.is_folded(0));
    assert!(!state.is_folded(4));
}

#[test]
fn visual_line_mapping() {
    let source = r"line 0
fn foo() {
    hidden 1
    hidden 2
}
line 5";
    let mut state = setup_rust_fold_state(source);

    // Without folds, visual == document
    assert_eq!(state.visual_to_document_line(0), 0);
    assert_eq!(state.visual_to_document_line(5), 5);

    // Fold the function
    assert!(state.fold_at(1));

    // Visual line 0 -> doc line 0 (before fold)
    // Visual line 1 -> doc line 1 (fold start, visible)
    // Visual line 2 -> doc line 5 (after fold)
    assert_eq!(state.document_to_visual_line(0), Some(0));
    assert_eq!(state.document_to_visual_line(1), Some(1));
    assert_eq!(state.document_to_visual_line(2), None); // Hidden
    assert_eq!(state.document_to_visual_line(3), None); // Hidden
    assert_eq!(state.document_to_visual_line(4), None); // Hidden (closing brace)
    assert_eq!(state.document_to_visual_line(5), Some(2));
}

#[test]
fn hidden_line_count() {
    let source = r"fn foo() {
    line 1
    line 2
    line 3
}";
    let mut state = setup_rust_fold_state(source);

    assert_eq!(state.hidden_line_count(), 0);

    state.fold_at(0);
    assert_eq!(state.hidden_line_count(), 4); // Lines 1-4 are hidden
}

#[test]
fn toggle_fold() {
    let source = "fn main() {\n    println!(\"hello\");\n}";
    let mut state = setup_rust_fold_state(source);

    assert!(!state.is_folded(0));
    assert!(state.toggle_fold_at(0));
    assert!(state.is_folded(0));
    assert!(state.toggle_fold_at(0));
    assert!(!state.is_folded(0));
}

#[test]
fn non_foldable_line() {
    let source = "let x = 1;";
    let mut state = setup_rust_fold_state(source);

    assert!(!state.is_foldable(0));
    assert!(!state.fold_at(0));
    assert!(!state.toggle_fold_at(0));
}

/// The generation counter is what lets a retained frame notice a fold:
/// folding never touches the document revision, so every mutation that can
/// change what folding makes visible must move this counter — and the
/// no-op paths must not, or every frame would rebuild for nothing.
#[test]
fn generation_moves_with_every_fold_mutation_and_only_those() {
    let source = "fn main() {\n    println!(\"hello\");\n}";
    let mut state = setup_rust_fold_state(source);
    let after_setup = state.generation();
    assert!(
        after_setup > 0,
        "update_regions producing fresh regions moves the generation"
    );

    // Queries move nothing.
    let _ = state.is_foldable(0);
    let _ = state.is_folded(0);
    let _ = state.is_line_hidden(1);
    let _ = state.visual_to_document_line(0);
    assert_eq!(state.generation(), after_setup, "queries are not mutations");

    // The no-op mutation paths move nothing either.
    assert!(!state.fold_at(99), "line 99 is not foldable");
    assert!(!state.unfold_at(0), "line 0 is not folded yet");
    assert_eq!(
        state.generation(),
        after_setup,
        "refused fold operations change nothing observable"
    );

    // Every successful mutation moves it.
    assert!(state.fold_at(0));
    let after_fold = state.generation();
    assert_ne!(after_fold, after_setup, "folding moves the generation");

    assert!(state.unfold_at(0));
    let after_unfold = state.generation();
    assert_ne!(after_unfold, after_fold, "unfolding moves the generation");

    state.fold_all();
    let after_fold_all = state.generation();
    assert_ne!(after_fold_all, after_unfold);

    state.unfold_all();
    let after_unfold_all = state.generation();
    assert_ne!(after_unfold_all, after_fold_all);

    let info = state.export_fold_info();
    state.import_fold_info(&info);
    let after_import = state.generation();
    assert_ne!(after_import, after_unfold_all, "import rebuilds fold state");

    state.set_language(state.language().expect("a language was set"));
    assert_eq!(
        state.generation(),
        after_import,
        "setting the same language is the documented early-out"
    );

    state.clear_language();
    assert_ne!(
        state.generation(),
        after_import,
        "clearing the language clears folds and must say so"
    );
}

#[test]
fn export_import() {
    let source = "fn main() {\n    println!(\"hello\");\n}";
    let mut state = setup_rust_fold_state(source);
    state.fold_at(0);

    let info = state.export_fold_info();
    assert!(info.folded.contains(&0));

    state.unfold_all();
    assert!(!state.is_folded(0));

    state.import_fold_info(&info);
    assert!(state.is_folded(0));
}

#[test]
fn language_change_clears_state() {
    let source = "fn main() {\n    println!(\"hello\");\n}";
    let mut state = setup_rust_fold_state(source);
    state.fold_at(0);
    assert!(state.is_folded(0));

    state.set_language(Language::Python);
    assert!(state.regions().is_empty());
    assert!(!state.is_folded(0));
}

/// Regression: a fold nested inside another must not have its lines counted
/// twice.
///
/// `fold_all` folds every detected region, including nested ones, and
/// `LineMapping::build` used to sum each fold's span independently. For an
/// outer fold hiding four lines and an inner fold hiding two of the same
/// four, that reported six hidden lines out of six — so
/// `visible_line_count` returned 0 while `is_line_hidden` correctly
/// reported two visible lines, and `document_to_visual_line` underflowed
/// `doc_line - hidden_before` and panicked on the first line after the
/// folds. Any nested block reached it.
#[test]
fn nested_folds_do_not_double_count_hidden_lines() {
    let source = "fn a() {\n    if x {\n        y();\n    }\n}\nend\n";
    let mut state = setup_rust_fold_state(source);
    let total_lines = 6;

    state.fold_all();

    let visible: Vec<usize> = (0..total_lines)
        .filter(|&line| !state.is_line_hidden(line))
        .collect();
    assert_eq!(visible, vec![0, 5], "the wrong lines are hidden");
    assert_eq!(
        state.visible_line_count(total_lines),
        visible.len(),
        "visible_line_count disagrees with is_line_hidden"
    );

    // The mapping must be total over every line: hidden lines report None,
    // visible ones report their row, and nothing panics.
    assert_eq!(state.document_to_visual_line(0), Some(0));
    for hidden_line in 1..=4 {
        assert_eq!(state.document_to_visual_line(hidden_line), None);
    }
    assert_eq!(
        state.document_to_visual_line(5),
        Some(1),
        "the first line after a nested fold is misplaced"
    );
    assert_eq!(state.visual_to_document_line(1), 5);
}

/// Two folds that merely sit next to each other must stay separate: the
/// line between them is visible, so coalescing them would hide it.
#[test]
fn adjacent_folds_are_not_merged() {
    // Lines 0-2 are one block, 3-5 another; line 3 is visible.
    let source = "fn a() {\n    x();\n}\nfn b() {\n    y();\n}\nend\n";
    let mut state = setup_rust_fold_state(source);
    let total_lines = 7;

    state.fold_all();

    let visible: Vec<usize> = (0..total_lines)
        .filter(|&line| !state.is_line_hidden(line))
        .collect();
    assert_eq!(
        visible,
        vec![0, 3, 6],
        "the line between two folds must stay visible"
    );
    assert_eq!(state.visible_line_count(total_lines), visible.len());
    assert_eq!(state.document_to_visual_line(3), Some(1));
    assert_eq!(state.document_to_visual_line(6), Some(2));
}

/// Every line of a document must map without panicking, whatever is folded.
/// This is the property the underflow violated.
#[test]
fn every_line_maps_under_every_combination_of_folds() {
    let source = "fn a() {\n    if x {\n        y();\n    }\n}\nend\n";
    let total_lines = 6;

    // Every pair, so nested and disjoint combinations are both covered —
    // folding one line at a time can never nest, and nesting is where the
    // accounting broke.
    for first in 0..total_lines {
        for second in 0..total_lines {
            let mut state = setup_rust_fold_state(source);
            if !state.fold_at(first) {
                continue;
            }
            state.fold_at(second);

            let mut rows = Vec::new();
            for line in 0..total_lines {
                if let Some(row) = state.document_to_visual_line(line) {
                    rows.push(row);
                }
            }
            assert_eq!(
                rows.len(),
                state.visible_line_count(total_lines),
                "folding {first} then {second} disagrees on how many lines are visible"
            );
            // Rows must be a gap-free 0..n sequence, or a renderer would
            // skip or repeat a screen line.
            assert!(
                rows.iter().copied().eq(0..rows.len()),
                "folding {first} then {second} produced non-contiguous rows: {rows:?}"
            );
            // And the round-trip must land back on a visible line.
            for (row, &line) in rows.iter().enumerate() {
                let _ = line;
                let doc = state.visual_to_document_line(row);
                assert!(
                    !state.is_line_hidden(doc),
                    "row {row} maps to hidden line {doc} after folding {first} then {second}"
                );
            }
        }
    }
}

// ==========================================================================
// The keystroke path
//
// Everything above exercises `FoldState` directly. These go through
// `Editor::apply_command`, because that is where the cost was: the fold walk was
// only reachable via `refresh_syntax`, which only `apply_command_internal`
// calls, and every benchmark that existed measured something else.
//
// They need a real grammar, so they are absent without the `syntax` feature —
// with the stub, folding is a brace scan of the whole document and there is no
// tree to count nodes in.
// ==========================================================================
#[cfg(feature = "syntax")]
mod keystroke {
    use iridium_syntax::{FoldDetector, SyntaxTree};

    use crate::editor::{Editor, EditorConfig};
    use crate::{Command, Language, Position};

    /// One JSON record per line: the shape a 100,000-line file is usually in,
    /// and the one the whole-tree walk was slowest on.
    fn json_document(records: usize) -> String {
        use std::fmt::Write as _;

        let mut out = String::from("[\n");
        for i in 0..records {
            // Writing into a `String` is infallible; discarding the result
            // invents no failure path for a case that has none.
            let _ = writeln!(
                out,
                "  {{ \"id\": {i}, \"name\": \"record-{i}\", \"active\": true }},"
            );
        }
        out.push_str("  null\n]\n");
        out
    }

    /// An editor holding `content`, with `language` set and folds already built.
    fn editor_for(language: Language, content: &str) -> Editor {
        let mut editor = Editor::new(EditorConfig::default());
        editor.set_content(content);
        editor.set_language(language);
        editor
    }

    /// The column inside the first record's name, which is a safe place to type.
    ///
    /// Safe matters: a character that makes the document ill-formed genuinely
    /// restructures everything after it, and re-walking is then correct rather
    /// than a regression. Measuring "typing does not walk the document" needs
    /// typing that does not rewrite the document's meaning.
    const NAME_COLUMN: usize = 26;

    /// How many tree nodes fold detection examines for one typed character.
    fn fold_nodes_for_one_keystroke(records: usize) -> u64 {
        let mut editor = editor_for(Language::Json, &json_document(records));

        // Type once first, so the measured keystroke is a steady-state one and
        // not whatever the first edit after a load happens to pay.
        editor.apply_command(Command::Insert {
            position: Position::new(1, NAME_COLUMN),
            text: "a".to_owned(),
        });
        let before = editor.state().fold_state.fold_nodes_visited();

        editor.apply_command(Command::Insert {
            position: Position::new(1, NAME_COLUMN),
            text: "b".to_owned(),
        });
        editor.state().fold_state.fold_nodes_visited() - before
    }

    /// The bug, stated as a test.
    ///
    /// Typing one character used to walk every node of the parse tree — on a
    /// 100,000-line JSON document, millions of them, to produce a single fold
    /// region, at over half a second per keystroke against an 8 ms budget. The
    /// two sizes are compared rather than one bound asserted so that the test
    /// says "does not grow with the document" rather than "is under a number I
    /// chose".
    #[test]
    fn typing_with_a_language_set_does_not_walk_the_document() {
        let small = fold_nodes_for_one_keystroke(200);
        let large = fold_nodes_for_one_keystroke(8_000);

        assert_eq!(
            small, large,
            "one keystroke examined {small} tree nodes for folds on a \
             200-record document and {large} on an 8,000-record one; the cost \
             of typing must not depend on how much is already typed"
        );
        assert!(
            large < 64,
            "one keystroke examined {large} tree nodes for folds, which is more \
             than the edit site can account for"
        );
    }

    /// Typing must leave the editor's folds exactly where a full recompute would.
    ///
    /// Runs a real burst of typing — including newlines, which move every fold
    /// after them — and checks after every character.
    #[test]
    fn folds_after_typing_match_a_full_recompute() {
        let source = "fn main() {\n    let total = compute(1, 2);\n    println!(\"{total}\");\n}\n";
        let mut editor = editor_for(Language::Rust, source);
        let detector = FoldDetector::new(Language::Rust);

        for (step, text) in ["x", "\n", "    let extra = 3;", "\n", "y", " "]
            .into_iter()
            .cycle()
            .take(24)
            .enumerate()
        {
            editor.apply_command(Command::Insert {
                position: Position::new(1, 4),
                text: text.to_owned(),
            });

            let state = editor.state();
            let tree = state
                .syntax
                .tree()
                .expect("a language is set, so there is a tree");
            assert_eq!(
                state.fold_state.regions(),
                detector.regions_in(tree).as_slice(),
                "step {step}: the editor's folds drifted from a full walk of its \
                 own tree"
            );

            let content = state.content();
            let mut fresh = SyntaxTree::new(Language::Rust).expect("rust parses");
            let parsed = fresh.parse(&content).expect("the document parses");
            assert_eq!(
                state.fold_state.regions(),
                detector.regions_in(parsed).as_slice(),
                "step {step}: the editor's folds drifted from a fresh parse"
            );
        }
    }

    /// Deleting is an edit too, and it moves folds the other way.
    ///
    /// Whole lines go, including their newline, so every fold after the deletion
    /// shifts up and some stop being folds at all — which is the case the
    /// retained-and-shifted half of the cache has to get right.
    #[test]
    fn folds_after_deleting_match_a_full_recompute() {
        let source =
            "fn a() {\n    one();\n    two();\n    three();\n}\n\nfn b() {\n    four();\n}\n";
        let mut editor = editor_for(Language::Rust, source);
        let detector = FoldDetector::new(Language::Rust);
        let before = editor.state().fold_state.regions().to_vec();
        assert!(!before.is_empty(), "the fixture folds to begin with");

        for step in 0..4 {
            let line = editor
                .state()
                .document
                .line(1)
                .expect("the fixture keeps more than two lines throughout");
            editor.apply_command(Command::Delete {
                range: crate::Range::new(Position::new(1, 0), Position::new(2, 0)),
                deleted_text: line,
            });

            let state = editor.state();
            let tree = state
                .syntax
                .tree()
                .expect("a language is set, so there is a tree");
            assert_eq!(
                state.fold_state.regions(),
                detector.regions_in(tree).as_slice(),
                "step {step}: the editor's folds drifted after a deletion"
            );
        }

        assert_ne!(
            editor.state().fold_state.regions(),
            before.as_slice(),
            "four deleted lines must have moved something, or this test would \
             pass without the folds ever being recomputed"
        );
    }
}
