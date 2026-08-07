//! Flattening possibly-nested highlight spans into the runs a renderer draws.
//!
//! A tree-sitter query answers about *nodes*, and nodes nest, so the spans it
//! produces nest with them. A `template_string` is a string and the
//! `${...}` inside it is embedded code; both captures are correct and both are
//! emitted. A renderer draws a line as a flat sequence of coloured slices, so
//! somewhere between the two the nesting has to collapse — and the byte covered
//! by two spans has to be given to one of them.
//!
//! # The rule
//!
//! ⭐ **The innermost span owns the byte.** A span nested inside another is the
//! more specific answer to the same question, and the specific answer is the
//! one worth showing. The containing span keeps every byte the inner one does
//! not take, so `` `a${b}c` `` renders as string, code, string rather than as
//! one flat colour or as a hole.
//!
//! # Why this is not left to each face
//!
//! It was, and both faces got it wrong the same way: walk the spans in start
//! order, skip any that begins inside a range already claimed. An outer span
//! starts earlier, so it claimed everything and the inner span was dropped.
//! Worse, the rule was not even consistent with itself — when the two spans
//! happened to *share* a start the order reversed, the inner one won, and the
//! outer was dropped entirely, tail included. Three shapes of the same
//! relationship, three answers, none of them chosen.
//!
//! Written once here, in the always-compiled half of `render`, it is covered by
//! every kernel test gate. Its second caller lives in `wasm.rs`, which nothing
//! executes and no test can reach — an algorithm there could only ever be
//! verified by reading it.

/// A span to be flattened: a byte range and whatever the caller carries.
///
/// Offsets are relative to the text being covered, not to the document, and it
/// is the caller's job to have clamped them — a face that folds or virtualizes
/// has already had to translate document offsets into visible ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanRun<P> {
    /// First byte of the range.
    pub start: usize,
    /// One past the last byte of the range.
    pub end: usize,
    /// What the caller resolves into an appearance.
    pub payload: P,
}

/// One run of the flattened result: a range with a single owner.
///
/// The runs returned by [`flatten_spans`] are in order, do not overlap, and
/// together cover every byte from 0 to the length given — so a caller can draw
/// them straight through without tracking where it has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatRun<P> {
    /// First byte of the run.
    pub start: usize,
    /// One past the last byte of the run.
    pub end: usize,
    /// The span that owns these bytes, or `None` where no span does.
    pub payload: Option<P>,
}

/// Collapses `spans` into ordered, non-overlapping runs covering `0..len`.
///
/// The innermost span owns each byte; see the module documentation for why.
/// Three further cases have to be decided by any implementation, and are
/// decided here rather than left to emerge:
///
/// - **Identical ranges** — the first in `spans` wins. That matches
///   `Highlighter::spans_with`, which resolves exact duplicates the same way at
///   emission, so a caller that has been through the kernel's highlighter sees
///   one consistent rule rather than two.
/// - **Equal starts, different ends** — the shorter span owns its extent and
///   the longer keeps the tail past it. Nothing is dropped.
/// - **Partial overlap**, neither span containing the other — the
///   later-starting span takes the overlap. It is the only resolution that
///   keeps the output ordered without splitting a span in two, and grammars do
///   not produce this shape: nodes nest or are disjoint.
///
/// Degenerate and out-of-range spans are discarded, and any span reaching past
/// `len` is clamped to it, so a caller may pass whatever a range query returned
/// without filtering first.
///
/// The sort is stable because the identical-range rule depends on input order.
#[must_use]
pub fn flatten_spans<P: Clone>(mut spans: Vec<SpanRun<P>>, len: usize) -> Vec<FlatRun<P>> {
    spans.retain(|span| span.start < span.end && span.start < len);
    for span in &mut spans {
        span.end = span.end.min(len);
    }
    // Containers before their contents: a longer span at the same start is the
    // one that encloses, and it has to be on the stack before the span it
    // encloses arrives.
    spans.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| right.end.cmp(&left.end))
    });

    let mut runs: Vec<FlatRun<P>> =
        Vec::with_capacity(spans.len().saturating_mul(2).saturating_add(1));
    // The spans covering the cursor, outermost first — so the last is the one
    // that owns the byte under it.
    let mut stack: Vec<SpanRun<P>> = Vec::new();
    let mut cursor = 0_usize;

    for span in spans {
        while let Some(top) = stack.last() {
            if top.end > span.start {
                break;
            }
            if cursor < top.end {
                runs.push(FlatRun {
                    start: cursor,
                    end: top.end,
                    payload: Some(top.payload.clone()),
                });
                cursor = top.end;
            }
            stack.pop();
        }

        if cursor < span.start {
            runs.push(FlatRun {
                start: cursor,
                end: span.start,
                payload: stack.last().map(|top| top.payload.clone()),
            });
            cursor = span.start;
        }

        // An exact duplicate of the span already covering this range: the first
        // one keeps it. Sorting puts identical ranges next to each other, so
        // the top of the stack is the only place a duplicate can hide.
        if stack
            .last()
            .is_some_and(|top| top.start == span.start && top.end == span.end)
        {
            continue;
        }
        stack.push(span);
    }

    while let Some(top) = stack.pop() {
        if cursor < top.end {
            runs.push(FlatRun {
                start: cursor,
                end: top.end,
                payload: Some(top.payload),
            });
            cursor = top.end;
        }
    }

    if cursor < len {
        runs.push(FlatRun {
            start: cursor,
            end: len,
            payload: None,
        });
    }

    runs
}

/// Clamps `index` into `content` and moves it down to the nearest character
/// boundary.
///
/// Span offsets are document bytes, while the text a face draws is its own
/// extraction — folds collapsed to a placeholder, the viewport virtualized —
/// so an offset that was a boundary in the document can land inside a
/// multi-byte character in the extraction. Slicing there panics.
///
/// Moving *down* rather than up keeps a span from growing over text it does
/// not cover, and a span that this empties is discarded by [`flatten_spans`]
/// rather than drawn as a zero-width run.
///
/// ⚠️ Every face needs this before it slices, and until this was hoisted only
/// one of them had it: the desktop resolver snapped, both web resolvers sliced
/// the raw offset and would panic on the first fold drift over non-ASCII text.
#[must_use]
pub fn snap_down(content: &str, index: usize) -> usize {
    let mut index = index.min(content.len());
    while index > 0 && !content.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::{FlatRun, SpanRun, flatten_spans, snap_down};

    /// `(start, end, payload)` for brevity in the fixtures below.
    fn span(start: usize, end: usize, payload: &'static str) -> SpanRun<&'static str> {
        SpanRun {
            start,
            end,
            payload,
        }
    }

    /// The runs as `(start, end, payload)`, with a gap written as `""`.
    fn flat(runs: &[FlatRun<&'static str>]) -> Vec<(usize, usize, &'static str)> {
        runs.iter()
            .map(|run| (run.start, run.end, run.payload.unwrap_or("")))
            .collect()
    }

    /// ⭐ The defect this module exists for, in its simplest form.
    ///
    /// Both faces used to answer `[(0, 20, "string")]` here: the outer span
    /// starts first, claims everything, and the interpolation inside it is
    /// discarded.
    #[test]
    fn an_inner_span_takes_its_bytes_and_the_outer_keeps_the_rest() {
        let runs = flatten_spans(vec![span(0, 20, "string"), span(5, 10, "code")], 20);
        assert_eq!(
            flat(&runs),
            vec![(0, 5, "string"), (5, 10, "code"), (10, 20, "string")]
        );
    }

    #[test]
    fn nesting_three_deep_unwinds_in_order() {
        let runs = flatten_spans(
            vec![span(0, 20, "a"), span(2, 10, "b"), span(4, 6, "c")],
            20,
        );
        assert_eq!(
            flat(&runs),
            vec![
                (0, 2, "a"),
                (2, 4, "b"),
                (4, 6, "c"),
                (6, 10, "b"),
                (10, 20, "a"),
            ]
        );
    }

    /// ⚠️ The case the old rule got wrong in the opposite direction: the inner
    /// span won and the outer was dropped whole, so its tail lost its colour.
    #[test]
    fn an_equal_start_gives_the_shorter_span_its_extent_and_keeps_the_tail() {
        let runs = flatten_spans(vec![span(0, 10, "outer"), span(0, 4, "inner")], 10);
        assert_eq!(flat(&runs), vec![(0, 4, "inner"), (4, 10, "outer")]);
    }

    /// Input order decides, and it is the same rule `spans_with` applies to
    /// exact duplicates at emission.
    #[test]
    fn identical_ranges_are_won_by_the_first_span_given() {
        let runs = flatten_spans(vec![span(0, 5, "first"), span(0, 5, "second")], 5);
        assert_eq!(flat(&runs), vec![(0, 5, "first")]);
    }

    #[test]
    fn gaps_before_between_and_after_the_spans_are_marked_rather_than_dropped() {
        let runs = flatten_spans(vec![span(2, 4, "a"), span(6, 8, "b")], 10);
        assert_eq!(
            flat(&runs),
            vec![
                (0, 2, ""),
                (2, 4, "a"),
                (4, 6, ""),
                (6, 8, "b"),
                (8, 10, "")
            ]
        );
    }

    #[test]
    fn no_spans_at_all_is_one_gap_over_the_whole_text() {
        let runs = flatten_spans(Vec::<SpanRun<&str>>::new(), 7);
        assert_eq!(flat(&runs), vec![(0, 7, "")]);
    }

    #[test]
    fn an_empty_text_produces_no_runs() {
        let runs = flatten_spans(vec![span(0, 5, "a")], 0);
        assert_eq!(flat(&runs), Vec::new());
    }

    /// A caller may pass a range query's output unfiltered.
    #[test]
    fn degenerate_and_out_of_range_spans_are_discarded_and_long_ones_clamped() {
        let runs = flatten_spans(
            vec![
                span(3, 3, "empty"),
                span(9, 4, "reversed"),
                span(12, 20, "past the end"),
                span(2, 40, "overlong"),
            ],
            10,
        );
        assert_eq!(flat(&runs), vec![(0, 2, ""), (2, 10, "overlong")]);
    }

    /// Not a shape any grammar produces — nodes nest or are disjoint — but the
    /// output must still be ordered and total, so the rule is stated and tested
    /// rather than left to whatever the loop happens to do.
    #[test]
    fn a_partial_overlap_goes_to_the_later_starting_span() {
        let runs = flatten_spans(vec![span(0, 10, "a"), span(5, 15, "b")], 20);
        assert_eq!(flat(&runs), vec![(0, 5, "a"), (5, 15, "b"), (15, 20, "")]);
    }

    /// Input order must not matter except where the rules say it does.
    #[test]
    fn the_result_does_not_depend_on_the_order_spans_arrive_in() {
        let forwards = flatten_spans(
            vec![span(0, 20, "a"), span(2, 10, "b"), span(4, 6, "c")],
            20,
        );
        let backwards = flatten_spans(
            vec![span(4, 6, "c"), span(2, 10, "b"), span(0, 20, "a")],
            20,
        );
        assert_eq!(flat(&forwards), flat(&backwards));
    }

    #[test]
    fn snapping_lands_on_character_boundaries_and_never_grows_a_span() {
        // Four bytes: 'a', then a three-byte '€'.
        let content = "a€";
        assert_eq!(snap_down(content, 0), 0);
        assert_eq!(snap_down(content, 1), 1, "already a boundary");
        assert_eq!(snap_down(content, 2), 1, "inside the euro sign, moves down");
        assert_eq!(snap_down(content, 3), 1, "still inside it");
        assert_eq!(snap_down(content, 4), 4, "the end is a boundary");
        assert_eq!(snap_down(content, 99), 4, "past the end clamps to it");
        assert_eq!(snap_down("", 5), 0, "empty content has only one answer");
    }

    /// The property every caller relies on: draw the runs straight through and
    /// every byte is painted exactly once.
    #[test]
    fn the_runs_are_ordered_disjoint_and_cover_the_whole_text() {
        const LEN: usize = 30;
        let runs = flatten_spans(
            vec![
                span(0, 12, "a"),
                span(3, 3, "degenerate"),
                span(4, 8, "b"),
                span(4, 8, "b again"),
                span(6, 7, "c"),
                span(12, 12, "empty"),
                span(15, 25, "d"),
                span(20, 40, "e"),
            ],
            LEN,
        );

        let mut expected_start = 0;
        for run in &runs {
            assert_eq!(run.start, expected_start, "runs must not skip or overlap");
            assert!(run.start < run.end, "a run must cover at least one byte");
            expected_start = run.end;
        }
        assert_eq!(expected_start, LEN, "the runs must reach the end");
    }
}
