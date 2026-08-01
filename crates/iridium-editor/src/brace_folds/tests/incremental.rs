//! The incremental half: an edit must cost the edit, and change nothing.
//!
//! Kept apart from the equivalence tests in the parent because the two prove
//! different things. Those show the scan reproduces the scanner it replaced;
//! these show that maintaining it across an edit reproduces the scan.

use super::super::*;
use super::{full_scan, legacy_scan};

/// Applies `edit` to `source` and returns the new text with the descriptor.
///
/// The rows are computed the way a document computes them, so that a test edit
/// describes itself exactly as the editor's would.
fn apply(source: &str, start: usize, old_end: usize, insert: &str) -> (String, LineEdit) {
    let mut updated = String::with_capacity(source.len() - (old_end - start) + insert.len());
    updated.push_str(&source[..start]);
    updated.push_str(insert);
    updated.push_str(&source[old_end..]);

    let new_end = start + insert.len();
    let edit = LineEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_row: row_of(source, start),
        old_end_row: row_of(source, old_end),
        new_end_row: row_of(&updated, new_end),
    };
    (updated, edit)
}

/// The row `offset` lies on, counted the way the document counts rows.
///
/// A document breaks a line after every `\n` *and* after a `\r` that no `\n`
/// follows, which is what ropey's `LF_CR` means. The scanner breaks only on
/// `\n`; where the two disagree the cache must decline to be incremental, and
/// this counts the document's way so that a test can actually put it in that
/// position.
fn row_of(source: &str, offset: usize) -> usize {
    let bytes = &source.as_bytes()[..offset];
    let mut rows = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' || (*byte == b'\r' && bytes.get(index + 1) != Some(&b'\n')) {
            rows += 1;
        }
    }
    rows
}

/// Drives `edits` through a cache incrementally and checks it against a full
/// rescan after every one.
///
/// Returns how many lines the incremental cache read in total, so a caller can
/// insist it was cheaper than rebuilding — which is the assertion that stops
/// this from silently comparing a full scan with itself.
fn drive(source: &str, edits: &[(usize, usize, &str)]) -> (u64, u64) {
    let mut cache = BraceFoldCache::new();
    cache.rebuild(source);

    let mut text = source.to_owned();
    let mut rebuilt_lines = 0u64;

    for (step, (start, old_end, insert)) in edits.iter().enumerate() {
        let start = (*start).min(text.len());
        let old_end = (*old_end).max(start).min(text.len());
        if !text.is_char_boundary(start) || !text.is_char_boundary(old_end) {
            continue;
        }

        let (updated, edit) = apply(&text, start, old_end, insert);
        cache.update(&updated, &edit);
        text = updated;

        let expected = full_scan(&text);
        rebuilt_lines += u64::try_from(text.lines().count()).unwrap_or(0);
        assert_eq!(
            cache.regions(),
            expected.as_slice(),
            "step {step}: the incremental scan drifted from a full one on {text:?}"
        );
        assert_eq!(
            cache.regions(),
            legacy_scan(&text).as_slice(),
            "step {step}: the incremental scan drifted from the scanner this \
             replaced on {text:?}"
        );
    }

    (cache.lines_scanned(), rebuilt_lines)
}

/// A document of `blocks` three-line functions.
fn blocks_document(blocks: usize) -> String {
    use std::fmt::Write as _;

    let mut source = String::new();
    for index in 0..blocks {
        // Writing into a `String` is infallible.
        let _ = write!(source, "fn f{index}() {{\n    body();\n}}\n");
    }
    source
}

/// How many lines the cache reads for one typed character near the top of a
/// `blocks`-block document.
///
/// The measured keystroke is the second one, so it is a steady-state edit rather
/// than whatever the first edit after a load happens to pay.
fn lines_read_for_one_keystroke(blocks: usize) -> u64 {
    let source = blocks_document(blocks);
    let mut cache = BraceFoldCache::new();
    cache.rebuild(&source);

    let at = source.find("body").expect("the fixture has a body");
    let (source, edit) = apply(&source, at, at, "x");
    cache.update(&source, &edit);

    let before = cache.lines_scanned();
    let at = source.find("body").expect("the fixture still has a body");
    let (source, edit) = apply(&source, at, at, "y");
    cache.update(&source, &edit);

    assert_eq!(
        cache.regions(),
        full_scan(&source).as_slice(),
        "the incremental result must equal a full one"
    );
    cache.lines_scanned() - before
}

/// The bug, stated as a test.
///
/// The brace scanner read every byte of the document for every keystroke. The
/// two sizes are compared rather than one bound asserted, so that this says
/// "does not grow with the document" rather than "is under a number I chose" —
/// and the bound that is asserted is the interval between recorded stacks, which
/// is the most a resume can have to re-read.
#[test]
fn the_lines_one_keystroke_reads_do_not_grow_with_the_document() {
    let small = lines_read_for_one_keystroke(20);
    let large = lines_read_for_one_keystroke(2_000);

    assert_eq!(
        small, large,
        "one keystroke read {small} lines in a 60-line document and {large} in \
         a 6,000-line one; the cost of typing must not depend on how much is \
         already typed"
    );
    assert!(
        large <= CHECKPOINT_INTERVAL as u64 + 2,
        "one keystroke read {large} lines, which is more than resuming from the \
         nearest recorded stack can account for"
    );
}

#[test]
fn the_cache_matches_a_full_rescan_across_a_long_edit_sequence() {
    let source = concat!(
        "fn alpha() {\n",
        "    let s = \"{ not a brace }\";\n",
        "    // } not a brace either\n",
        "    if s.len() > 0 {\n",
        "        beta();\n",
        "    }\n",
        "}\n",
        "\n",
        "/* a block comment {\n",
        "   spanning lines } */\n",
        "\n",
        "fn gamma() {\n",
        "    delta();\n",
        "}\n",
    );

    // Deliberately varied: insertions and deletions, inside and across lines,
    // structural (braces, quotes, comment delimiters) and not, at the start,
    // middle and end of the document.
    let edits: &[(usize, usize, &str)] = &[
        (0, 0, "// leading\n"),
        (30, 30, "x"),
        (30, 31, ""),
        (12, 12, "\n    {\n    }\n"),
        (5, 9, ""),
        (0, 0, "{\n"),
        (40, 44, "\"}\""),
        (60, 60, "/*"),
        (62, 62, "*/"),
        (2, 2, "}\n{\n"),
        (100, 140, ""),
        (0, 3, "fn a() {\n"),
        (7, 7, "\r\n"),
        (0, 0, "\n\n\n"),
        (20, 20, "'"),
        (25, 25, "'"),
        (0, 200, "{\n{\n{\n}\n}\n}\n"),
        (6, 6, "// {\n"),
        (0, 0, "}"),
        (11, 11, "\\"),
    ];

    let (incremental, rebuilt) = drive(source, edits);
    assert!(
        incremental < rebuilt,
        "the cache read {incremental} lines where rebuilding every time would \
         have read {rebuilt}; it is not taking the incremental path, and an \
         equivalence check against a cache that always rebuilds compares a full \
         scan with itself"
    );
}

#[test]
fn the_cache_matches_a_full_rescan_across_generated_edit_sequences() {
    const INSERTS: [&str; 10] = ["", "x", "{", "}", "\n", "{\n}\n", "\"", "//", "/*\n*/", "'"];

    let source = "fn a() {\n    one();\n}\n\nfn b() {\n    two();\n}\n";
    let mut total_incremental = 0u64;
    let mut total_rebuilt = 0u64;

    for seed in 0..120u64 {
        let mut state = seed.wrapping_mul(2_862_933_555_777_941_757).wrapping_add(3);
        let mut edits = Vec::new();
        for _ in 0..12 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let start = usize::try_from((state >> 33) % 64).unwrap_or(0);
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let span = usize::try_from((state >> 33) % 6).unwrap_or(0);
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let insert = INSERTS[usize::try_from((state >> 33) % 10).unwrap_or(0)];
            edits.push((start, start + span, insert));
        }

        let (incremental, rebuilt) = drive(source, &edits);
        total_incremental += incremental;
        total_rebuilt += rebuilt;
    }

    assert!(
        total_incremental < total_rebuilt,
        "over 120 generated sequences the cache read {total_incremental} lines \
         against {total_rebuilt} for rebuilding every time; it is not taking \
         the incremental path"
    );
}

#[test]
fn a_document_with_a_lone_carriage_return_is_only_ever_scanned_whole() {
    // The scanner numbers lines by `\n` alone; the document also breaks on a
    // lone `\r`. The two disagree here, so the cache must not trust the rows an
    // edit is described in.
    let source = "{\rone\r}\r{\n}\n";
    let mut cache = BraceFoldCache::new();
    cache.rebuild(source);

    let (updated, edit) = apply(source, 2, 2, "x");
    cache.update(&updated, &edit);

    assert_eq!(
        cache.regions(),
        full_scan(&updated).as_slice(),
        "a lone carriage return must not make the incremental result wrong"
    );
}

#[test]
fn an_edit_that_introduces_a_lone_carriage_return_is_caught() {
    let source = "{\n  a\n}\n{\n  b\n}\n";
    let mut cache = BraceFoldCache::new();
    cache.rebuild(source);

    // Splitting a `\r\n` is the awkward case: the `\r` was fine a moment ago.
    let (with_crlf, edit) = apply(source, 5, 5, "\r\n");
    cache.update(&with_crlf, &edit);
    let (updated, edit) = apply(&with_crlf, 6, 7, "");
    cache.update(&updated, &edit);

    assert_eq!(
        cache.regions(),
        full_scan(&updated).as_slice(),
        "deleting the `\\n` of a `\\r\\n` leaves a lone `\\r`, which renumbers \
         every line after it"
    );
}

#[test]
fn nesting_deeper_than_a_recorded_stack_is_still_correct() {
    // Deeper than `MAX_CHECKPOINT_DEPTH`, so most stacks are never recorded and
    // every rescan resumes from further back. Slower; it must not be wrong.
    let mut source = String::new();
    for _ in 0..(MAX_CHECKPOINT_DEPTH + 40) {
        source.push_str("{\n");
    }
    source.push_str("x\n");
    for _ in 0..(MAX_CHECKPOINT_DEPTH + 40) {
        source.push_str("}\n");
    }

    let mut cache = BraceFoldCache::new();
    cache.rebuild(&source);
    assert_eq!(cache.regions(), full_scan(&source).as_slice());

    let (updated, edit) = apply(&source, source.len() / 2, source.len() / 2, "y");
    cache.update(&updated, &edit);
    assert_eq!(
        cache.regions(),
        full_scan(&updated).as_slice(),
        "deep nesting must not change the answer, only the cost"
    );
}

#[test]
fn an_edit_far_beyond_a_checkpoint_interval_still_rejoins() {
    // Long enough that several stacks are recorded, so the resume point is not
    // trivially line zero and the retained-stack arithmetic is exercised.
    let source = blocks_document(CHECKPOINT_INTERVAL * 5);

    let mut cache = BraceFoldCache::new();
    cache.rebuild(&source);
    let before = cache.lines_scanned();

    // Insert a whole new block near the front, which moves every line after it.
    let (updated, edit) = apply(&source, 0, 0, "fn head() {\n    x();\n}\n");
    cache.update(&updated, &edit);

    assert_eq!(cache.regions(), full_scan(&updated).as_slice());
    let read = cache.lines_scanned() - before;
    let total = u64::try_from(updated.lines().count()).unwrap_or(u64::MAX);
    assert!(
        read * 4 < total,
        "inserting three lines at the front read {read} of {total} lines; the \
         scan is not rejoining the previous one"
    );
}

#[test]
fn an_unprimed_cache_scans_the_whole_document() {
    let mut cache = BraceFoldCache::new();
    assert!(!cache.is_primed());

    // An update against a cache that never saw a document has nothing to keep,
    // so it must scan everything rather than publish only what the edit covered.
    let source = "{\n  a\n}\n{\n  b\n}\n";
    let edit = LineEdit {
        start_byte: 0,
        old_end_byte: 0,
        new_end_byte: 0,
        start_row: 0,
        old_end_row: 0,
        new_end_row: 0,
    };
    cache.update(source, &edit);

    assert!(cache.is_primed());
    assert_eq!(cache.regions(), full_scan(source).as_slice());
}

#[test]
fn an_edit_describing_a_different_document_is_declined_rather_than_believed() {
    let source = "{\n  a\n}\n";
    let mut cache = BraceFoldCache::new();
    cache.rebuild(source);

    // Offsets past the end, and rows in the wrong order: neither describes this
    // document, and the answer must still be the right one.
    for edit in [
        LineEdit {
            start_byte: 0,
            old_end_byte: 0,
            new_end_byte: 9_999,
            start_row: 0,
            old_end_row: 0,
            new_end_row: 0,
        },
        LineEdit {
            start_byte: 0,
            old_end_byte: 0,
            new_end_byte: 0,
            start_row: 5,
            old_end_row: 1,
            new_end_row: 1,
        },
        LineEdit {
            start_byte: 0,
            old_end_byte: 0,
            new_end_byte: 0,
            start_row: 0,
            old_end_row: 9_999,
            new_end_row: 0,
        },
    ] {
        cache.update(source, &edit);
        assert_eq!(
            cache.regions(),
            full_scan(source).as_slice(),
            "a nonsensical edit descriptor must cost a rescan, not correctness"
        );
    }
}
