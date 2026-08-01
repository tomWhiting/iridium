//! Benchmarks for the per-keystroke cost of a document that has a language set.
//!
//! # Why this file exists
//!
//! Typing one character into a 100,000-line JSON file with a language set cost
//! **752 ms**, against an 8 ms budget, and it grew with the document. It
//! survived because nothing measured it. `benches/syntax.rs` measures
//! `SyntaxState::note_edit`, which is genuinely 1.66 µs; the expensive work sat
//! in `EditorState::refresh_syntax`, one call further along, and was reached
//! only through `Editor::apply_command`. Every existing benchmark that touched
//! the editor ran without a language, where `sync` returns early and the fold
//! walk never happens — so every existing benchmark looked healthy.
//!
//! Everything here therefore goes through `Editor::apply_command` on a document
//! with a language **set**, which is the only configuration the bug existed in.
//!
//! # What is asserted, and what is only reported
//!
//! Criterion reports times; it cannot fail a build for being slow, and a
//! wall-clock threshold on a shared machine fails for reasons that have nothing
//! to do with this code. So the assertions are on *work done*:
//! `FoldState::fold_nodes_visited` counts the tree nodes fold detection examined,
//! and the checks below require that count to be small and, more importantly,
//! **identical across two document sizes an order of magnitude apart**. A
//! regression to walking the tree moves that count from single digits to
//! millions, and no amount of machine noise can hide it.

use std::fmt::Write as _;
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use iridium_editor::editor::{Editor, EditorConfig};
use iridium_editor::{Command, Language, Position};

/// The size the original measurement was taken at.
const LARGE_RECORDS: usize = 100_000;

/// A tenth of it, to show the cost does not scale with the document.
const SMALL_RECORDS: usize = 10_000;

/// The most fold work a single keystroke may do.
///
/// The edit site is one record deep in a flat array, so the nodes worth looking
/// at are the array, the record, its members and the string being typed into.
/// A regression to walking the document puts this in the millions; the exact
/// value below is not the point, the order of magnitude is.
const NODE_BUDGET: u64 = 64;

/// One JSON record per line — the shape the cost was measured on, and the one
/// that makes the tree widest: a 100,000-record array's root has two hundred
/// thousand direct children.
fn json_document(records: usize) -> String {
    let mut out = String::with_capacity(records * 64 + 16);
    out.push_str("[\n");
    for i in 0..records {
        // Writing into a `String` is infallible, so the result is discarded
        // rather than unwrapped: there is no failure path to handle.
        let _ = writeln!(
            out,
            "  {{ \"id\": {i}, \"name\": \"record-{i}\", \"active\": true, \"score\": {}.5 }},",
            i % 97
        );
    }
    out.push_str("  null\n]\n");
    out
}

/// Realistic Rust, for the other shape a document comes in: deep rather than
/// wide, and with orders of magnitude more fold regions than JSON's one.
///
/// Both matter. JSON exposes the width of the tree; Rust exposes how much work
/// the retained region list costs to carry across an edit, which is the part of
/// the incremental scheme that is proportional to *regions* rather than nodes.
fn rust_document(blocks: usize) -> String {
    let mut out = String::with_capacity(blocks * 512);
    for index in 0..blocks {
        let _ = write!(
            out,
            "/// Record produced by stage {index}.
#[derive(Debug, Clone)]
pub struct Record{index} {{
    pub id: u64,
    pub name: String,
    pub tags: Vec<String>,
}}

impl Record{index} {{
    pub fn new(id: u64, name: String) -> Self {{
        Self {{
            id,
            name,
            tags: Vec::new(),
        }}
    }}

    pub fn summary(&self) -> String {{
        let mut out = String::new();
        for tag in &self.tags {{
            if tag.is_empty() {{
                continue;
            }}
            out.push_str(tag);
        }}
        match self.tags.len() {{
            0 => String::from(\"empty\"),
            other => format!(\"{{other}}\"),
        }}
    }}
}}

"
        );
    }
    out
}

/// An editor holding `content` with `language` set and its folds already built.
fn editor_for(language: Language, content: &str) -> Editor {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content(content);
    editor.set_language(language);
    editor
}

/// Column inside the first JSON record's name — a place where typing keeps the
/// document well formed.
///
/// It has to be. A character that makes the document ill-formed genuinely
/// restructures everything after it, tree-sitter says so, and re-walking is then
/// the correct answer; benchmarking that would measure the fallback rather than
/// the path a keystroke normally takes.
const JSON_NAME_COLUMN: usize = 26;

/// Line and column inside a Rust identifier, for the same reason.
const RUST_EDIT: (usize, usize) = (3, 12);

/// Types one character and returns how many tree nodes folds examined for it.
fn fold_nodes_for_one_keystroke(editor: &mut Editor, at: Position) -> u64 {
    let before = editor.state().fold_state.fold_nodes_visited();
    editor.apply_command(Command::Insert {
        position: at,
        text: "x".to_owned(),
    });
    editor.state().fold_state.fold_nodes_visited() - before
}

/// Proves, before anything is timed, that a keystroke's fold cost is bounded and
/// does not depend on the size of the document.
///
/// This is the assertion the whole file is for. Without it the benchmark below
/// would report a number, someone would read it as healthy, and the next
/// regression would be invisible again — which is exactly what happened.
fn assert_folding_is_bounded() {
    let mut small = editor_for(Language::Json, &json_document(SMALL_RECORDS));
    let mut large = editor_for(Language::Json, &json_document(LARGE_RECORDS));
    let at = Position::new(1, JSON_NAME_COLUMN);

    // One keystroke each first: the measured one must be steady state, not
    // whatever the first edit after a load happens to pay.
    let _ = fold_nodes_for_one_keystroke(&mut small, at);
    let _ = fold_nodes_for_one_keystroke(&mut large, at);

    let small_nodes = fold_nodes_for_one_keystroke(&mut small, at);
    let large_nodes = fold_nodes_for_one_keystroke(&mut large, at);

    assert_eq!(
        small_nodes, large_nodes,
        "one keystroke examined {small_nodes} tree nodes for folds on a \
         {SMALL_RECORDS}-record document and {large_nodes} on a \
         {LARGE_RECORDS}-record one; the cost of typing must not depend on how \
         much is already typed"
    );
    assert!(
        large_nodes < NODE_BUDGET,
        "one keystroke examined {large_nodes} tree nodes for folds, over a \
         budget of {NODE_BUDGET}"
    );

    // And the folds must still be there. A detector that produced nothing would
    // satisfy every count above.
    assert!(
        !large.state().fold_state.regions().is_empty(),
        "the JSON array is a fold region; a count of zero would make the \
         assertions above meaningless"
    );
}

/// The headline: one typed character on a 100,000-line file with a language set.
///
/// This is the number the bug was reported as. It includes everything a
/// keystroke pays — the command, the undo entry, the incremental reparse and the
/// fold refresh — because that is what a user waits through.
fn keystroke_with_language_benchmark(c: &mut Criterion) {
    assert_folding_is_bounded();

    for (name, records) in [
        ("keystroke_json_10k_lines", SMALL_RECORDS),
        ("keystroke_json_100k_lines", LARGE_RECORDS),
    ] {
        let mut editor = editor_for(Language::Json, &json_document(records));
        let mut column = JSON_NAME_COLUMN;
        c.bench_function(name, |b| {
            b.iter(|| {
                editor.apply_command(Command::Insert {
                    position: black_box(Position::new(1, column)),
                    text: "x".to_owned(),
                });
                column += 1;
            });
        });

        assert!(
            !editor.state().fold_state.regions().is_empty(),
            "{name}: the document must still have folds after being typed into"
        );
    }
}

/// The same on Rust source, where the document has thousands of fold regions
/// rather than one.
///
/// The incremental scheme carries every retained region across each edit, so
/// this is the shape that exercises the part of it proportional to region count.
fn keystroke_rust_benchmark(c: &mut Criterion) {
    let source = rust_document(300);
    let mut editor = editor_for(Language::Rust, &source);
    let (line, column) = RUST_EDIT;

    let regions = editor.state().fold_state.regions().len();
    assert!(
        regions > 1_000,
        "the fixture must have many fold regions for this to measure anything \
         JSON does not; it has {regions}"
    );

    let _ = fold_nodes_for_one_keystroke(&mut editor, Position::new(line, column));
    let nodes = fold_nodes_for_one_keystroke(&mut editor, Position::new(line, column));
    assert!(
        nodes < NODE_BUDGET,
        "one keystroke examined {nodes} tree nodes for folds on Rust source, \
         over a budget of {NODE_BUDGET}"
    );

    let mut offset = 0;
    c.bench_function("keystroke_rust_10k_lines", |b| {
        b.iter(|| {
            editor.apply_command(Command::Insert {
                position: black_box(Position::new(line, column + offset)),
                text: "x".to_owned(),
            });
            offset += 1;
        });
    });
}

criterion_group!(
    benches,
    keystroke_with_language_benchmark,
    keystroke_rust_benchmark
);
criterion_main!(benches);
