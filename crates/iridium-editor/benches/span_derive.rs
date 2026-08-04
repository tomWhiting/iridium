//! Benchmarks for span derivation: the parser tax, measured.
//!
//! The parser-tax map (docs/design/PARSER-TAX-MAP.md) diagnosed the ~30ms a
//! keystroke with a grammar paid on a 10k-line file as the whole-document
//! span re-derive — `Highlighter::spans_in` walking the entire tree into a
//! full `SpanIndex` rebuild on every parse generation — and noted that
//! nothing anywhere measured `spans_in`: the one O(document) call on the
//! keystroke path was the one call with no ruler on it. This file is that
//! ruler. Three claims are pinned:
//!
//! 1. `spans_in` whole-document on the 10k-line fixture — the red number,
//!    recorded so today's tax survives the fix as a measurement.
//! 2. `spans_in_range` over a ~90-line viewport window, and over that window
//!    widened by the faces' ±100-line overscan — the cost the caches now pay
//!    per rebuild. Target: sub-millisecond, expected tens of microseconds.
//! 3. `SpanIndex::new` on each span set — the map's rank-2 term, timed
//!    separately so the derive/index split stays observable.
//!
//! Everything is reached through the crate's public surface, exactly as
//! `benches/syntax.rs` reaches it. The fixture generator below mirrors that
//! bench's generator line for line rather than importing it: the generator
//! lives inside a bench binary whose measured functions are the standing
//! proof that stage 1 never touched the kernel's parse path, and that file
//! stays untouched — the duplication is the cost of keeping it so, and this
//! comment says so rather than hiding it.

use std::hint::black_box;
use std::io::{self, Write as _};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use iridium_editor::editor::SyntaxState;
use iridium_editor::span_index::{OVERSCAN_LINES, SpanIndex};
use iridium_editor::syntax::Highlighter;
use iridium_editor::{Document, Language};
use iridium_syntax::Tree;

/// The smallest file the whole-document claim is made about.
const TARGET_LINES: usize = 10_000;

/// The viewport the range claims are made about: the ~90 lines a frame
/// paints on a common surface.
const VIEWPORT_LINES: usize = 90;

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

/// The parsed fixture and the byte windows the range claims are made about.
struct Fixture {
    /// The document's full text — the source every derive reads.
    text: String,
    /// The retained parse of that text.
    state: SyntaxState,
    /// The rules under measurement.
    highlighter: Highlighter,
    /// A mid-file viewport of [`VIEWPORT_LINES`] lines, in bytes.
    viewport: std::ops::Range<usize>,
    /// The same viewport widened by [`OVERSCAN_LINES`] each way, in bytes —
    /// the window the faces' caches derive per rebuild.
    widened: std::ops::Range<usize>,
}

/// The byte offset where `line` starts, with the document's end answering
/// for lines past the last.
fn line_start_byte(document: &Document, text_len: usize, line: usize) -> usize {
    document.line_to_byte_offset(line).unwrap_or(text_len)
}

/// Parses the fixture and computes the two windows, mid-file so the range
/// walk cannot lean on a cheap prefix.
///
/// Every failure path asserts before returning `None`, so the caller's bail
/// arm is unreachable — stated rather than trusted, because a bench that
/// silently measured nothing would look like a bench that measured something
/// very fast.
fn build_fixture() -> Option<Fixture> {
    let text = rust_source();
    let document = Document::new(&text);
    let line_count = document.line_count();
    assert!(
        line_count >= TARGET_LINES,
        "the fixture must be at least as large as the claim is made about"
    );

    let mut state = SyntaxState::new();
    state.set_language(Language::Rust);
    assert!(
        state.sync(&document).is_some(),
        "the generated fixture must parse as Rust"
    );
    let highlighter = Highlighter::try_new(Language::Rust);
    assert!(
        highlighter.is_some(),
        "rust ships a highlights query; a miss is a broken vendored dependency"
    );
    let highlighter = highlighter?;

    let first_line = line_count / 2;
    let last_line = first_line + VIEWPORT_LINES;
    let viewport = line_start_byte(&document, text.len(), first_line)
        ..line_start_byte(&document, text.len(), last_line);
    let widened = line_start_byte(&document, text.len(), first_line - OVERSCAN_LINES)
        ..line_start_byte(&document, text.len(), last_line + OVERSCAN_LINES);

    Some(Fixture {
        text,
        state,
        highlighter,
        viewport,
        widened,
    })
}

/// Proves, before anything is timed, that both derives do real, agreeing
/// work — and reports the span counts the map's magnitude estimates rest on.
fn assert_derives_agree(fixture: &Fixture, tree: &Tree) {
    let whole = fixture.highlighter.spans_in(tree, &fixture.text);
    let full_range = fixture
        .highlighter
        .spans_in_range(tree, &fixture.text, 0..fixture.text.len());
    assert_eq!(
        whole, full_range,
        "an unrestricted range must be the whole-document walk, span for span"
    );
    let windowed = fixture
        .highlighter
        .spans_in_range(tree, &fixture.text, fixture.widened.clone());
    assert!(
        !windowed.is_empty(),
        "the mid-file window must produce spans"
    );
    for span in &windowed {
        assert!(
            whole.contains(span),
            "the ranged walk invents nothing the whole walk does not produce"
        );
    }
    // The counts the map's magnitude estimates rest on, on the record beside
    // the timings — written as the desktop face writes its latency lines,
    // best-effort to stderr.
    let _ = writeln!(
        io::stderr(),
        "span_derive fixture: {} spans whole-document, {} spans in the widened window",
        whole.len(),
        windowed.len()
    );
}

/// The whole-document derive — the tax the map exists to record — beside the
/// range-scoped derives the faces now run, and the index build on each span
/// set.
fn span_derive_benchmarks(c: &mut Criterion) {
    let fixture = build_fixture();
    assert!(
        fixture.is_some(),
        "the fixture builder asserts on every failure path before answering None"
    );
    let Some(fixture) = fixture else {
        // Unreachable past the assertion; returning keeps the harness free of
        // panic-family macros by construction.
        return;
    };
    let tree = fixture.state.tree();
    assert!(tree.is_some(), "the synced fixture holds a tree");
    let Some(tree) = tree else {
        return;
    };
    assert_derives_agree(&fixture, tree);

    c.bench_function("spans_in_whole_document_10k_lines", |b| {
        b.iter(|| {
            fixture
                .highlighter
                .spans_in(black_box(tree), black_box(&fixture.text))
        });
    });

    c.bench_function("spans_in_range_viewport_90_lines", |b| {
        b.iter(|| {
            fixture.highlighter.spans_in_range(
                black_box(tree),
                black_box(&fixture.text),
                black_box(fixture.viewport.clone()),
            )
        });
    });

    c.bench_function("spans_in_range_viewport_plus_overscan", |b| {
        b.iter(|| {
            fixture.highlighter.spans_in_range(
                black_box(tree),
                black_box(&fixture.text),
                black_box(fixture.widened.clone()),
            )
        });
    });

    let whole_spans = fixture.highlighter.spans_in(tree, &fixture.text);
    let window_spans =
        fixture
            .highlighter
            .spans_in_range(tree, &fixture.text, fixture.widened.clone());

    c.bench_function("span_index_new_whole_document", |b| {
        b.iter_batched(
            || whole_spans.clone(),
            |spans| SpanIndex::new(black_box(spans)),
            BatchSize::LargeInput,
        );
    });

    c.bench_function("span_index_new_window", |b| {
        b.iter_batched(
            || window_spans.clone(),
            |spans| SpanIndex::new(black_box(spans)),
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, span_derive_benchmarks);
criterion_main!(benches);
