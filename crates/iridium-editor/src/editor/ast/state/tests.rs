//! Tests for the editor's retained syntax tree.
//!
//! Two of these are the point of the whole module:
//! [`folds_refresh_after_every_edit`], which is the bug this step exists to
//! fix, and [`a_missed_edit_still_leaves_a_correct_tree`], which is the promise
//! that the cheap path can never be the difference between right and wrong.

use std::fmt::Write as _;

use iridium_lang::Language;
use iridium_syntax::SyntaxTree;

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

/// What a tree says about its document: its shape, its folds, its colours.
#[derive(Debug, PartialEq, Eq)]
struct Reading {
    shape: String,
    folds: Vec<iridium_syntax::FoldRegion>,
    spans: Vec<iridium_syntax::HighlightSpan>,
}

/// The colours `tree` gives `text`, over the whole of it.
fn spans_of(tree: &iridium_syntax::Tree, text: &str) -> Vec<iridium_syntax::HighlightSpan> {
    iridium_syntax::Highlighter::try_new(Language::Rust)
        .expect("rust has a highlighter")
        .spans_in_range(tree, text, 0..text.len())
}

/// What a parse from scratch says about `text`.
fn reading_from_scratch(text: &str) -> Reading {
    let mut tree = SyntaxTree::new(Language::Rust).expect("rust parses");
    let parsed = tree.parse(text).expect("a parse must produce a tree");
    Reading {
        shape: parsed.root_node().to_sexp(),
        folds: iridium_syntax::FoldDetector::new(Language::Rust).regions_in(parsed),
        spans: spans_of(parsed, text),
    }
}

/// What the editor says about its document: the tree it holds, the folds it
/// publishes and the colours that tree gives.
fn reading_of(editor: &crate::editor::Editor) -> Reading {
    let text = editor.content();
    let tree = editor
        .state()
        .syntax
        .tree()
        .expect("a document with a language has a tree once settled");
    Reading {
        shape: tree.root_node().to_sexp(),
        folds: editor.state().fold_state.regions().to_vec(),
        spans: spans_of(tree, &text),
    }
}

/// A small seeded generator, so a failure is always the same failure.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        usize::try_from(self.next() % bound as u64).unwrap_or(0)
    }
}

/// A position for a random character boundary of `document`.
fn random_position(document: &Document, rng: &mut Rng) -> Position {
    let text = document.text();
    let mut offset = rng.below(text.len() + 1);
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    document
        .offset_to_position(offset)
        .expect("a character boundary inside the document has a position")
}

/// One random edit of the kinds an editor makes: typing, deleting, replacing
/// across lines, and typing at several carets at once. `None` stands for an
/// undo or a redo, which the caller performs.
fn random_command(document: &Document, rng: &mut Rng) -> Option<Command> {
    const TYPED: &[&str] = &[
        "x",
        " ",
        "\n",
        "{",
        "}",
        "\"",
        "(",
        ";",
        "é",
        "/*",
        "*/",
        "    ",
        "fn g() {\n    h();\n}\n",
        "let more = ready + 1;",
    ];
    let typed = |rng: &mut Rng| TYPED[rng.below(TYPED.len())].to_string();
    let random_range = |rng: &mut Rng| {
        let start = random_position(document, rng);
        let start_offset = document
            .position_to_offset(start)
            .expect("the position came from an offset");
        let text = document.text();
        let mut end_offset = (start_offset + 1 + rng.below(40)).min(text.len());
        while !text.is_char_boundary(end_offset) {
            end_offset -= 1;
        }
        let end = document
            .offset_to_position(end_offset)
            .expect("a character boundary inside the document has a position");
        Range::new(start, end)
    };

    match rng.below(10) {
        0..=3 => Some(Command::Insert {
            position: random_position(document, rng),
            text: typed(rng),
        }),
        4 | 5 => {
            let range = random_range(rng);
            Some(Command::Delete {
                range,
                deleted_text: document.slice(range),
            })
        },
        6 => {
            let range = random_range(rng);
            Some(Command::Replace {
                range,
                old_text: document.slice(range),
                new_text: typed(rng),
            })
        },
        7 => {
            // Several carets typing the same thing, applied last caret first so
            // each position still names the place it was chosen for.
            let text = typed(rng);
            let mut positions: Vec<Position> = (0..2 + rng.below(2))
                .map(|_| random_position(document, rng))
                .collect();
            positions.sort_unstable_by(|a, b| b.cmp(a));
            positions.dedup();
            Some(Command::Compound {
                commands: positions
                    .into_iter()
                    .map(|position| Command::Insert {
                        position,
                        text: text.clone(),
                    })
                    .collect(),
            })
        },
        _ => None,
    }
}

/// Random edits, undos, redos and multi-caret typing, with the tree, folds and
/// colours compared against a parse from scratch every time the parse is
/// allowed to catch up — inline, and with the parse on a worker thread.
///
/// On the worker the parse is deliberately left behind most of the time: edits
/// keep arriving while one is in flight, so it lands on a document that has
/// moved on and has to be carried forward. And between landings the folds must
/// still be exactly what the tree on hand says, because that tree is what the
/// colours are drawn from.
#[test]
fn incremental_results_match_a_parse_from_scratch_through_random_editing() {
    let source = "use std::fmt;\nuse std::io;\n\n/* a block\n   comment */\n\
                  pub struct Record {\n    id: u64,\n}\n\nimpl Record {\n    \
                  pub fn describe(&self) -> String {\n        if self.id == 0 {\n            \
                  return String::from(\"zero\");\n        }\n        format!(\"{}\", \
                  self.id)\n    }\n}\n\nfn main() {\n    let r = Record { id: 1 };\n    \
                  println!(\"{}\", r.describe());\n}\n";

    for background in [false, true] {
        let mut editor = crate::editor::Editor::new(crate::editor::EditorConfig::default());
        if background {
            assert!(
                editor
                    .parse_in_background(None)
                    .expect("the system gives the parser a thread"),
                "a native build with the syntax feature can parse on a worker"
            );
        }
        editor.set_content(source);
        editor.set_language(Language::Rust);
        editor.settle_syntax();
        assert_eq!(reading_of(&editor), reading_from_scratch(source));

        let mut rng = Rng(if background {
            0x9E37_79B9_7F4A_7C15
        } else {
            0xD1B5_4A32_D192_ED03
        });
        for step in 0..160 {
            let parses = |editor: &crate::editor::Editor| {
                editor.state().syntax.full_parses() + editor.state().syntax.incremental_parses()
            };
            let parses_before = parses(&editor);
            let colours_before = editor
                .state()
                .syntax
                .tree()
                .map(|tree| spans_of(tree, &editor.content()));
            let mut reported = None;
            match random_command(&editor.state().document, &mut rng) {
                Some(command) => {
                    reported = compute_edit_span(&editor.state().document, &command)
                        .ok()
                        .flatten();
                    editor.apply_command(command);
                },
                None if rng.below(2) == 0 => {
                    editor.undo();
                },
                None => {
                    editor.redo();
                },
            }

            // No parse landed, so the tree on hand is the last one shifted by
            // this edit: every colour clear of the edit must be exactly where
            // it was, moved by the edit's length if it lay after it.
            if let (Some(before), Some(span), true) =
                (&colours_before, reported, parses(&editor) == parses_before)
            {
                let tree = editor.state().syntax.tree().expect("the tree was shifted");
                let after = spans_of(tree, &editor.content());
                for colour in before {
                    let moved = if colour.end < span.start_byte {
                        Some(colour.clone())
                    } else if colour.start > span.old_end_byte {
                        Some(iridium_syntax::HighlightSpan {
                            start: colour.start - span.old_end_byte + span.new_end_byte,
                            end: colour.end - span.old_end_byte + span.new_end_byte,
                            highlight: colour.highlight,
                        })
                    } else {
                        None
                    };
                    if let Some(moved) = moved {
                        assert!(
                            after.contains(&moved),
                            "background {background}, step {step}: {colour:?}, clear of \
                             the edit {span:?}, should now be at {moved:?}"
                        );
                    }
                }
            }

            // Whatever tree is on hand — reparsed or merely shifted — the folds
            // published are the ones it describes.
            let text = editor.content();
            let tree = editor
                .state()
                .syntax
                .tree()
                .expect("an edited document keeps a tree");
            assert_eq!(
                editor.state().fold_state.regions(),
                iridium_syntax::FoldDetector::new(Language::Rust)
                    .regions_in(tree)
                    .as_slice(),
                "background {background}, step {step}: the folds do not describe \
                 the tree they are shown with"
            );
            for span in spans_of(tree, &text) {
                assert!(
                    span.start <= span.end && span.end <= text.len(),
                    "background {background}, step {step}: a colour lies outside the \
                     document: {span:?} in {} bytes",
                    text.len()
                );
            }

            if rng.below(4) == 0 {
                editor.settle_syntax();
                assert_eq!(
                    reading_of(&editor),
                    reading_from_scratch(&text),
                    "background {background}, step {step}: the settled tree, folds \
                     or colours differ from a parse from scratch of\n{text}"
                );
            }
        }
        editor.settle_syntax();
        assert_eq!(
            reading_of(&editor),
            reading_from_scratch(&editor.content()),
            "background {background}: the final settle differs from a parse from scratch"
        );
    }
}

/// While a parse is still to come, colours are drawn from the last tree
/// shifted by every edit since. Each colour clear of an edit must then sit
/// exactly where it sat, moved by the edit's length if it lay after it — never
/// at an offset that no longer holds the text it colours.
///
/// The background parser makes that state real but not reproducible: whether a
/// parse has landed by the next keystroke is a race. Reporting edits and not
/// parsing is the same state, reached on purpose, so this drives it directly
/// and parses only now and then.
#[test]
fn colours_from_a_shifted_tree_stay_with_their_text() {
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    let mut document = Document::new(
        "use std::fmt;\n\nfn main() {\n    let caf\u{e9} = \"x\";\n    println!(\"{}\", caf\u{e9});\n}\n\n\
         struct Point {\n    x: i32,\n}\n",
    );
    state.sync(&document).expect("the fixture parses");
    let mut rng = Rng(0x7F4A_7C15_9E37_79B9);
    let mut shifted_checks = 0_u32;

    for step in 0..300 {
        let Some(command) = random_command(&document, &mut rng) else {
            continue;
        };
        let text_before = document.text();
        let before = spans_of(state.tree().expect("a tree is held"), &text_before);
        let span = compute_edit_span(&document, &command)
            .expect("the command resolves")
            .expect("the command edits content");
        apply(&mut state, &mut document, &command);

        let text_after = document.text();
        let after = spans_of(state.tree().expect("a tree is held"), &text_after);
        for colour in &before {
            let moved = if colour.end < span.start_byte {
                colour.clone()
            } else if colour.start > span.old_end_byte {
                iridium_syntax::HighlightSpan {
                    start: colour.start - span.old_end_byte + span.new_end_byte,
                    end: colour.end - span.old_end_byte + span.new_end_byte,
                    highlight: colour.highlight,
                }
            } else {
                continue;
            };
            assert_eq!(
                text_before.get(colour.start..colour.end),
                text_after.get(moved.start..moved.end),
                "step {step}: the shift itself is wrong for {colour:?} after {command:?}"
            );
            assert!(
                after.contains(&moved),
                "step {step}: {colour:?}, clear of {span:?}, should now be at {moved:?}"
            );
            shifted_checks += 1;
        }

        if rng.below(5) == 0 {
            state.sync(&document).expect("the document parses");
            assert_eq!(
                shape(&state),
                shape_from_scratch(&text_after),
                "step {step}: the reparse disagrees with a parse from scratch"
            );
        }
    }
    assert!(
        shifted_checks >= 300,
        "only {shifted_checks} colours were checked across the edits"
    );
}

/// A document replaced, or its language changed, while a parse of the old one
/// is still running: whatever lands for the old one must never be installed
/// over the new one.
#[test]
fn replacing_the_document_or_language_mid_parse_never_installs_the_old_parse() {
    let mut big = String::new();
    for index in 0..4_000 {
        let _ = writeln!(big, "fn f{index}() {{\n    let v = {index};\n}}");
    }
    let small = "fn only() {\n    let a = 1;\n}\n";

    let mut editor = crate::editor::Editor::new(crate::editor::EditorConfig::default());
    assert!(
        editor
            .parse_in_background(None)
            .expect("the system gives the parser a thread")
    );
    editor.set_language(Language::Rust);

    // Replaced while the big document's first parse is in flight.
    editor.set_content(&big);
    editor.apply_command(Command::Insert {
        position: Position::new(0, 0),
        text: "// a\n".to_string(),
    });
    editor.set_content(small);
    editor.settle_syntax();
    assert_eq!(reading_of(&editor), reading_from_scratch(small));

    // Language away and back while a parse of an edit is in flight.
    editor.set_content(&big);
    editor.settle_syntax();
    editor.apply_command(Command::Insert {
        position: Position::new(1, 0),
        text: "    let w = 0;\n".to_string(),
    });
    editor.set_language(Language::Json);
    editor.set_language(Language::Rust);
    editor.settle_syntax();
    assert_eq!(reading_of(&editor), reading_from_scratch(&editor.content()));

    // Language cleared mid-parse: nothing may come back.
    editor.apply_command(Command::Insert {
        position: Position::new(0, 0),
        text: "\n".to_string(),
    });
    editor.clear_language();
    editor.settle_syntax();
    assert!(editor.state().syntax.tree().is_none());
    assert!(editor.state().fold_state.regions().is_empty());
}
