//! Which lines changed since a revision, for readers that follow a document
//! line by line.
//!
//! Every text mutation — typed, pasted or replayed by undo — reaches the
//! document through [`Document::insert`](super::Document::insert) or
//! [`Document::delete`](super::Document::delete), so recording there covers
//! them all. A reader keeps the revision it last saw and asks for the span
//! since, instead of rereading the whole document; when the record no longer
//! reaches back that far it is told so and rereads.

use std::collections::VecDeque;

/// How many edits the record keeps before forgetting the oldest. One
/// keystroke is usually one edit; a multi-caret edit is one per caret.
const KEPT: usize = 64;

/// The lines one or more edits replaced, as half-open line ranges.
///
/// Lines `start..old_end` of the document as it was became lines
/// `start..new_end` of the document as it is; every line outside them is
/// unchanged, those after shifted by `new_end - old_end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineChange {
    /// The first line that may differ, the same in both documents.
    pub start: usize,
    /// One past the last replaced line, in the document as it was.
    pub old_end: usize,
    /// One past the last replacing line, in the document as it is.
    pub new_end: usize,
}

impl LineChange {
    /// Whether nothing changed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.old_end && self.start == self.new_end
    }

    /// This change followed by `next`, which is in this one's new lines.
    ///
    /// Two far-apart edits merge into one span covering the lines between:
    /// too wide is a cost to the reader, too narrow would be a wrong answer.
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        if self.is_empty() {
            return next;
        }
        let low = self.start.min(next.start);
        let high = self.new_end.max(next.old_end);
        Self {
            start: low,
            old_end: high - self.new_end + self.old_end,
            new_end: high - next.old_end + next.new_end,
        }
    }
}

/// The recent edits, each with the revision it produced.
#[derive(Debug, Clone, Default)]
pub(super) struct LineEdits {
    /// Oldest first; the revisions are consecutive.
    kept: VecDeque<(u64, LineChange)>,
    /// The earliest revision the record answers from.
    floor: u64,
}

impl LineEdits {
    /// A record that answers from `revision` on.
    pub(super) const fn starting_at(revision: u64) -> Self {
        Self {
            kept: VecDeque::new(),
            floor: revision,
        }
    }

    /// Notes `change`, which produced `revision`.
    pub(super) fn record(&mut self, revision: u64, change: LineChange) {
        if self.kept.len() == KEPT
            && let Some((dropped, _)) = self.kept.pop_front()
        {
            self.floor = dropped;
        }
        self.kept.push_back((revision, change));
    }

    /// Forgets everything before `revision`, for an edit whose lines could
    /// not be worked out.
    pub(super) fn forget(&mut self, revision: u64) {
        self.kept.clear();
        self.floor = revision;
    }

    /// The lines changed between `revision` and `current`, or `None` when the
    /// record does not reach back to `revision`.
    pub(super) fn since(&self, revision: u64, current: u64) -> Option<LineChange> {
        if revision < self.floor || revision > current {
            return None;
        }
        let empty = LineChange {
            start: 0,
            old_end: 0,
            new_end: 0,
        };
        Some(
            self.kept
                .iter()
                .filter(|(produced, _)| *produced > revision)
                .fold(empty, |merged, (_, change)| merged.then(*change)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{LineChange, LineEdits};
    use crate::document::{Document, Position, Range};

    /// Every line of `document`, without endings.
    fn all_lines(document: &Document) -> Vec<String> {
        (0..document.line_count())
            .map(|line| document.line(line).unwrap_or_default())
            .collect()
    }

    /// A reader following the document only through the reported spans, now
    /// after every edit and now after several, ends with the same lines as
    /// one rereading it.
    #[test]
    fn a_reader_following_the_spans_keeps_every_line() {
        let mut document = Document::new("alpha\nbeta\ngamma\ndelta\n");
        let mut mirror = all_lines(&document);
        let mut followed = document.revision();
        let pieces = [
            "x",
            "\n",
            "yz\n",
            "\n\n",
            "q\r\n",
            "",
            "long piece\nof text",
        ];
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = |bound: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            usize::try_from(seed % u64::try_from(bound.max(1)).unwrap_or(1)).unwrap_or(0)
        };
        for step in 0..2_000 {
            let line = next(document.line_count());
            let length = document.line_len(line).unwrap_or(0);
            let at = Position::new(line, next(length + 1));
            if step % 3 == 0 {
                let end_line = (line + next(3)).min(document.line_count() - 1);
                let end_length = document.line_len(end_line).unwrap_or(0);
                let end = Position::new(end_line, next(end_length + 1));
                if end_line > line || end.column >= at.column {
                    document
                        .delete(Range::new(at, end))
                        .expect("an in-bounds range deletes");
                }
            } else {
                document
                    .insert(at, pieces[next(pieces.len())])
                    .expect("an in-bounds position inserts");
            }
            // Reading after some edits only exercises merging spans.
            if next(3) == 0 {
                continue;
            }
            match document.lines_changed_since(followed) {
                Some(change) => {
                    let replacing = (change.start..change.new_end)
                        .map(|line| document.line(line).unwrap_or_default());
                    mirror.splice(change.start..change.old_end, replacing);
                },
                None => mirror = all_lines(&document),
            }
            followed = document.revision();
            assert_eq!(mirror, all_lines(&document), "after step {step}");
        }
    }

    const fn change(start: usize, old_end: usize, new_end: usize) -> LineChange {
        LineChange {
            start,
            old_end,
            new_end,
        }
    }

    #[test]
    fn a_later_edit_below_widens_the_span_down_to_it() {
        // Line 2 split in two, then old line 6 (now 7) edited in place.
        let merged = change(2, 3, 4).then(change(7, 8, 8));
        assert_eq!(merged, change(2, 7, 8));
    }

    #[test]
    fn a_later_edit_above_widens_the_span_up_to_it() {
        let merged = change(5, 6, 6).then(change(1, 3, 2));
        assert_eq!(merged, change(1, 6, 5));
    }

    #[test]
    fn an_edit_inside_the_span_leaves_its_ends() {
        let merged = change(2, 4, 9).then(change(3, 5, 4));
        assert_eq!(merged, change(2, 4, 8));
    }

    #[test]
    fn the_record_answers_only_from_its_floor() {
        let mut edits = LineEdits::starting_at(3);
        edits.record(4, change(0, 1, 1));
        edits.record(5, change(2, 3, 3));
        assert_eq!(edits.since(2, 5), None, "before the floor");
        assert_eq!(edits.since(3, 5), Some(change(0, 3, 3)));
        assert_eq!(edits.since(4, 5), Some(change(2, 3, 3)));
        assert!(edits.since(5, 5).is_some_and(|change| change.is_empty()));
        assert_eq!(edits.since(6, 5), None, "a revision not reached yet");
        edits.forget(6);
        assert_eq!(edits.since(5, 6), None, "forgotten");
    }

    #[test]
    fn the_oldest_edit_is_dropped_past_the_limit() {
        let mut edits = LineEdits::starting_at(0);
        for revision in 1..=u64::try_from(super::KEPT).unwrap_or(0) + 1 {
            edits.record(revision, change(0, 1, 1));
        }
        assert_eq!(edits.since(0, 65), None, "the first edit was dropped");
        assert!(edits.since(1, 65).is_some());
    }
}
