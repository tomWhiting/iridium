//! Benchmarks for the retained syntax tree.
//!
//! Two claims are measured here, both from the plan's verification section.
//!
//! 1. `note_edit` under 10µs — the architectural claim that *typing never
//!    parses*. [`SyntaxState::note_edit`] shifts the retained tree's positions
//!    and sets a dirty flag; the benchmark asserts afterwards that neither parse
//!    counter moved across the whole run, so "it did not parse" is checked
//!    rather than assumed.
//! 2. Expand after a one-character edit on a 10k-line file under 1ms — the
//!    daily-driver latency claim. This one *includes* the incremental reparse,
//!    because that is the work a user waits through between typing a character
//!    and seeing the selection widen. The benchmark counts its own iterations
//!    and asserts that the incremental-parse counter matches exactly, which is
//!    what stops it from silently measuring an already-synced tree.
//!
//! Everything is reached through the crate's public surface: `SyntaxState` is
//! re-exported from the public `editor` module, and `compute_edit_span`,
//! `EditSpan` and `AstRequest` are public too. No visibility was widened for
//! these benchmarks.
//!
//! # Why two documents rather than one that is edited in place
//!
//! The fixture holds the same file in both states — with and without one typed
//! character — plus the [`EditSpan`] for the insert and for the delete that
//! undoes it. Each iteration alternates between them, which is exactly what
//! typing a character and backspacing it does to the tree, and it keeps the
//! measured loop free of any setup that would otherwise be timed alongside the
//! call under test.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use iridium_editor::document::{EditSpan, compute_edit_span};
use iridium_editor::editor::SyntaxState;
use iridium_editor::input::AstRequest;
use iridium_editor::{Command, CursorState, Document, Language, Position, Range};

/// The smallest file the expand claim is made about.
const TARGET_LINES: usize = 10_000;

/// The identifier the cursor sits inside, and the one the typed character
/// extends.
const IDENTIFIER: &str = "limit";

/// The line the edit and the expansion both happen on, matched by prefix.
const EDIT_SITE: &str = "self.weight >";

/// Emits one structurally complete unit of Rust source.
///
/// Varied on purpose: a file of 10,000 identical lines parses unrepresentatively
/// fast, because tree-sitter's incremental reparse reuses subtrees and identical
/// text maximises what can be reused. This block carries a doc comment, an
/// attribute, a struct, an impl with three methods, a `for` loop, an `if`, a
/// `match` and a format string — the node variety a real file has — and every
/// block is textually distinct through `index`.
fn record_block(index: usize) -> String {
    format!(
        "/// A record produced by stage {index} of the pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Record{index} {{
    pub id: u64,
    pub name: String,
    pub weight: f64,
    pub tags: Vec<String>,
}}

impl Record{index} {{
    /// Creates a record with the given identifier.
    pub fn new(id: u64, name: String) -> Self {{
        Self {{
            id,
            name,
            weight: {index}.5,
            tags: Vec::new(),
        }}
    }}

    /// Returns true when the record outweighs the bound and carries tags.
    pub fn heavier_than(&self, limit: f64) -> bool {{
        self.weight > limit && !self.tags.is_empty()
    }}

    /// Folds the tags into a single summary string.
    pub fn summary(&self) -> String {{
        let mut out = String::with_capacity(self.tags.len() * 8);
        for (position, tag) in self.tags.iter().enumerate() {{
            if position > 0 {{
                out.push_str(\", \");
            }}
            out.push_str(tag);
        }}
        match self.tags.len() {{
            0 => String::from(\"empty\"),
            1 => out,
            other => format!(\"{{out}} (+{{other}})\"),
        }}
    }}
}}

"
    )
}

/// Builds at least [`TARGET_LINES`] lines of realistic Rust source.
///
/// Generated rather than committed: a 300KB fixture file does not belong in the
/// tree, and the generator makes the shape of what is being parsed reviewable.
fn rust_source() -> String {
    let mut source = String::with_capacity(512 * 1024);
    let mut lines = 0;
    let mut index = 0;
    while lines < TARGET_LINES {
        let block = record_block(index);
        lines += block.lines().count();
        source.push_str(&block);
        index += 1;
    }
    source
}

/// The file in both states, with the spans and cursors that move between them.
struct Fixture {
    /// The document before the character is typed.
    clean: Document,
    /// The document after the character is typed.
    typed: Document,
    /// The span of the insert, measured against `clean`.
    insert_span: EditSpan,
    /// The span of the delete that undoes it, measured against `typed`.
    delete_span: EditSpan,
    /// A collapsed cursor inside the identifier in `clean`.
    clean_cursor: CursorState,
    /// A collapsed cursor inside the same identifier in `typed`.
    typed_cursor: CursorState,
    /// How many lines the fixture actually has, for the report.
    line_count: usize,
}

/// Computes the span of `command` against `document`.
///
/// The assertion is the real check; `unwrap_or` only exists because a fallback
/// value is needed to satisfy the type, and the assertion above it guarantees
/// the fallback is never reached.
fn span_of(document: &Document, command: &Command) -> EditSpan {
    let span = compute_edit_span(document, command).ok().flatten();
    assert!(
        span.is_some(),
        "the benchmark's own command must resolve against its own document"
    );
    span.unwrap_or(EditSpan {
        start_byte: 0,
        old_end_byte: 0,
        new_end_byte: 0,
        old_end_row: 0,
        old_end_column: 0,
    })
}

/// Builds the fixture, placing the edit in the middle of the file.
///
/// The middle rather than the start because tree-sitter's reparse cost depends
/// on where the damage is: an edit in the first line invalidates a prefix that
/// is cheap to redo, and one at the very end reuses almost everything.
fn build_fixture() -> Fixture {
    let source = rust_source();

    let total_lines = source.lines().count();
    let line_index = source
        .lines()
        .enumerate()
        .skip(total_lines / 2)
        .find(|(_, line)| line.trim_start().starts_with(EDIT_SITE))
        .map_or(0, |(index, _)| index);
    assert!(
        line_index > 0,
        "the generated source must contain the edit site past its midpoint"
    );

    let line = source.lines().nth(line_index).unwrap_or_default();
    let identifier_at = line.find(IDENTIFIER).map_or(0, |column| column);
    assert!(
        identifier_at > 0,
        "the edit site must contain the identifier the cursor sits in"
    );

    // Typing lands at the end of the identifier, which is what extending a name
    // by one character does. The cursor sits two characters into it, so it is
    // inside the identifier in both states of the document and the expansion
    // starts from the same node either way.
    let insert_at = Position::new(line_index, identifier_at + IDENTIFIER.len());
    let cursor_at = Position::new(line_index, identifier_at + 2);
    let delete_range = Range::new(
        insert_at,
        Position::new(line_index, identifier_at + IDENTIFIER.len() + 1),
    );

    let clean = Document::new(&source);
    let mut typed = Document::new(&source);
    assert!(
        typed.insert(insert_at, "x").is_ok(),
        "the typed character must apply to the fixture"
    );
    assert!(
        typed
            .line(line_index)
            .is_some_and(|line| line.contains("limitx")),
        "the typed character must land inside the identifier"
    );

    let insert_span = span_of(
        &clean,
        &Command::Insert {
            position: insert_at,
            text: "x".to_string(),
        },
    );
    let delete_span = span_of(
        &typed,
        &Command::Delete {
            range: delete_range,
            deleted_text: "x".to_string(),
        },
    );

    let line_count = clean.line_count();
    assert!(
        line_count >= TARGET_LINES,
        "the fixture must be at least as large as the claim is made about"
    );

    Fixture {
        clean,
        typed,
        insert_span,
        delete_span,
        clean_cursor: CursorState::at(cursor_at),
        typed_cursor: CursorState::at(cursor_at),
        line_count,
    }
}

/// Builds a state with the fixture parsed once, ready to be edited.
fn parsed_state(fixture: &Fixture) -> SyntaxState {
    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    assert!(
        state.sync(&fixture.clean).is_some(),
        "the generated fixture must parse as Rust"
    );
    state
}

/// Proves, before anything is timed, that both measured paths do real work.
///
/// Without this a benchmark that measured nothing would look like a benchmark
/// that measured something very fast. Three things are checked: that a reported
/// edit keeps `sync` off the full-parse path (so `note_edit` genuinely reached
/// the tree rather than returning early), that the expansion reparses, and that
/// it moves the selection rather than declining to answer.
fn assert_both_paths_do_work(fixture: &Fixture) {
    let mut state = parsed_state(fixture);
    assert_eq!(
        state.full_parses(),
        1,
        "the first sync parses the file whole"
    );
    assert_eq!(state.incremental_parses(), 0, "nothing has been edited yet");

    state.note_edit(&fixture.typed, &fixture.insert_span);
    let expanded = state.apply_ast_request(
        &fixture.typed,
        &fixture.typed_cursor,
        AstRequest::ExpandSelection,
    );

    assert_eq!(
        state.full_parses(),
        1,
        "a reported edit must not degrade to a full parse"
    );
    assert_eq!(
        state.incremental_parses(),
        1,
        "the expansion must have reparsed the edited tree"
    );
    assert!(
        expanded.is_some_and(|cursor| cursor != fixture.typed_cursor),
        "expanding inside an identifier must widen the selection"
    );
}

/// Measures [`SyntaxState::note_edit`] alone: the keystroke path.
///
/// Nothing else is inside the timed closure. The documents and spans are built
/// once, so what is measured is the two `byte_point` lookups, the expansion
/// stack clear and tree-sitter's `edit` — and nothing that a real keystroke
/// would not also pay.
///
/// The parse counters are read after the run rather than during it. Asserting
/// inside the loop would be measured; asserting after it covers every iteration
/// criterion ran, warm-up included.
fn note_edit_benchmark(c: &mut Criterion) {
    let fixture = build_fixture();
    assert_both_paths_do_work(&fixture);
    let mut state = parsed_state(&fixture);

    let mut typed = false;
    c.bench_function("note_edit", |b| {
        b.iter(|| {
            if typed {
                state.note_edit(black_box(&fixture.clean), black_box(&fixture.delete_span));
            } else {
                state.note_edit(black_box(&fixture.typed), black_box(&fixture.insert_span));
            }
            typed = !typed;
        });
    });

    assert_eq!(
        state.full_parses(),
        1,
        "typing must never parse: the only full parse is the initial one"
    );
    assert_eq!(
        state.incremental_parses(),
        0,
        "typing must never parse, incrementally or otherwise"
    );
}

/// Measures a one-character edit followed by an expansion on a 10k-line file.
///
/// The timed region covers both halves deliberately. `note_edit` marks the tree
/// dirty and `apply_ast_request` calls `sync`, which reparses incrementally
/// before it walks — so this is the whole of what a user waits through, not the
/// expansion on top of an already-current tree.
///
/// `iterations` is what makes that verifiable: it counts every pass through the
/// closure, and the assertion afterwards requires the incremental-parse counter
/// to equal it exactly. One reused sync would break that equality.
fn expand_after_edit_benchmark(c: &mut Criterion) {
    let fixture = build_fixture();
    assert_both_paths_do_work(&fixture);
    let mut state = parsed_state(&fixture);

    let mut typed = false;
    let mut iterations: u64 = 0;
    c.bench_function("expand_after_edit_10k_lines", |b| {
        b.iter(|| {
            let (document, cursor, span) = if typed {
                (&fixture.clean, &fixture.clean_cursor, &fixture.delete_span)
            } else {
                (&fixture.typed, &fixture.typed_cursor, &fixture.insert_span)
            };
            typed = !typed;
            iterations += 1;

            state.note_edit(black_box(document), black_box(span));
            let expanded = state.apply_ast_request(
                black_box(document),
                black_box(cursor),
                black_box(AstRequest::ExpandSelection),
            );
            black_box(expanded)
        });
    });

    assert_eq!(
        state.full_parses(),
        1,
        "no iteration may have fallen back to a full parse"
    );
    assert_eq!(
        state.incremental_parses(),
        iterations,
        "every measured iteration must have reparsed; a reused tree measures nothing"
    );
    assert!(
        fixture.line_count >= TARGET_LINES,
        "the claim is about a file of at least 10,000 lines"
    );
}

criterion_group!(benches, note_edit_benchmark, expand_after_edit_benchmark);
criterion_main!(benches);
