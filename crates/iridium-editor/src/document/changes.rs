//! What recent edits did, so a reader can catch up without a rescan.
//!
//! Every edit to a [`Document`](super::Document) leaves some bytes before it
//! and some after it untouched, and replaces the text between two positions
//! with text ending at a third. One record per edit holds both, so a reader
//! that follows bytes, one that follows lines and one that follows positions
//! all read the same log.
//!
//! The log is inline and fixed in size, so cloning a document stays an
//! allocation-free copy. When it fills, its two oldest records merge into one
//! that covers both: conservatively, never wrongly. Bytes are therefore
//! answerable for any revision the document has had; positions and lines are
//! answerable unless an edit among them could not be described, which is the
//! case beside a carriage return.

use super::Position;

/// How much of the text an edit, or a run of edits, left as it was.
///
/// The first `prefix` bytes and the last `suffix` bytes are the same bytes
/// before and after. The two may overlap only when nothing changed at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unchanged {
    /// Bytes at the start that the edits did not touch.
    pub prefix: usize,
    /// Bytes at the end that the edits did not touch.
    pub suffix: usize,
}

impl Unchanged {
    /// Both descriptions at once: what neither edit touched.
    fn and(self, other: Self) -> Self {
        Self {
            prefix: self.prefix.min(other.prefix),
            suffix: self.suffix.min(other.suffix),
        }
    }
}

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

/// Where an edit, or a merged run of edits, replaced text.
///
/// The text from `start` to `old_end` of the document as it was became the
/// text from `start` to `new_end` of the document as it is. Columns count
/// code points, as [`Position`] does. A merged run is widened to whole lines,
/// so it may claim more than was replaced, never less.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Replaced {
    /// Where the replaced text began, the same in both documents.
    pub start: Position,
    /// Where the replaced text ended, in the document as it was.
    pub old_end: Position,
    /// Where the replacing text ends, in the document as it is.
    pub new_end: Position,
}

impl Replaced {
    /// The lines this replaced.
    #[must_use]
    pub const fn lines(self) -> LineChange {
        LineChange {
            start: self.start.line,
            old_end: self.old_end.line + 1,
            new_end: self.new_end.line + 1,
        }
    }

    /// Which line the start of `line`, a line of the document as it was, is
    /// on now: the same line before the edit, shifted after it, and `None`
    /// when the edit replaced the text there.
    ///
    /// Text inserted at the very start of a line pushes that line down, and
    /// text inserted anywhere after its start leaves it where it is, which is
    /// what keeps a fold on its header as lines are added around it.
    #[must_use]
    pub const fn follow_line_start(self, line: usize) -> Option<usize> {
        let at = Position::new(line, 0);
        if at.line < self.start.line || (at.line == self.start.line && self.start.column > 0) {
            Some(line)
        } else if at.line > self.old_end.line
            || (at.line == self.old_end.line && self.old_end.column == 0)
        {
            Some(line - self.old_end.line + self.new_end.line)
        } else {
            None
        }
    }

    /// This edit followed by `next`, which is in this one's new text, as one
    /// edit covering both, widened to whole lines.
    fn then(self, next: Self) -> Self {
        let lines = self.lines().then(next.lines());
        Self {
            start: Position::new(lines.start, 0),
            old_end: Position::new(lines.old_end - 1, usize::MAX),
            new_end: Position::new(lines.new_end - 1, usize::MAX),
        }
    }
}

/// Records kept before the oldest two merge. A face asks every frame, so it
/// is rarely more than a keystroke's few edits behind.
const CAPACITY: usize = 16;

/// One edit, or a merged run of them, ending at revision `after`.
#[derive(Debug, Clone, Copy)]
struct Entry {
    after: u64,
    unchanged: Unchanged,
    /// Where the edit replaced text, or `None` when that could not be
    /// worked out, as beside a carriage return.
    replaced: Option<Replaced>,
}

impl Entry {
    /// This entry followed by `next`, as one.
    fn then(self, next: Self) -> Self {
        Self {
            after: next.after,
            unchanged: self.unchanged.and(next.unchanged),
            replaced: match (self.replaced, next.replaced) {
                (Some(first), Some(second)) => Some(first.then(second)),
                _ => None,
            },
        }
    }
}

/// The edits since a document was made, newest kept separately.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChangeLog {
    /// The revision the document started at; nothing before it is answerable.
    floor: u64,
    /// A ring: `count` entries from `oldest`, in revision order.
    entries: [Entry; CAPACITY],
    oldest: usize,
    count: usize,
}

impl ChangeLog {
    /// An empty log for a document whose first revision is `floor`.
    pub(super) const fn starting_at(floor: u64) -> Self {
        let empty = Entry {
            after: floor,
            unchanged: Unchanged {
                prefix: 0,
                suffix: 0,
            },
            replaced: None,
        };
        Self {
            floor,
            entries: [empty; CAPACITY],
            oldest: 0,
            count: 0,
        }
    }

    /// Records the edit that produced revision `after`.
    pub(super) fn record(&mut self, after: u64, unchanged: Unchanged, replaced: Option<Replaced>) {
        if self.count == CAPACITY {
            // Merge the oldest into the next, which then covers both.
            let oldest = self.entries[self.oldest];
            self.oldest = (self.oldest + 1) % CAPACITY;
            let next = &mut self.entries[self.oldest];
            *next = oldest.then(*next);
            self.count -= 1;
        }
        self.entries[(self.oldest + self.count) % CAPACITY] = Entry {
            after,
            unchanged,
            replaced,
        };
        self.count += 1;
    }

    /// The entries after `revision`, oldest first, or `None` when `revision`
    /// is not one the document has had up to `current`.
    fn after(&self, revision: u64, current: u64) -> Option<impl Iterator<Item = Entry> + '_> {
        if revision < self.floor || revision > current {
            return None;
        }
        Some(
            (0..self.count)
                .map(|index| self.entries[(self.oldest + index) % CAPACITY])
                .filter(move |entry| entry.after > revision),
        )
    }

    /// What the edits after `revision` left alone, up to `current`, in a
    /// document now `len` bytes long. `None` when `revision` is not one the
    /// document has had.
    pub(super) fn since(&self, revision: u64, current: u64, len: usize) -> Option<Unchanged> {
        let all = Unchanged {
            prefix: len,
            suffix: len,
        };
        Some(
            self.after(revision, current)?
                .fold(all, |unchanged, entry| unchanged.and(entry.unchanged)),
        )
    }

    /// Where each edit after `revision` replaced text, oldest first. `None`
    /// when `revision` is not one the document has had, or an edit since
    /// could not be described.
    pub(super) fn replaced_since(&self, revision: u64, current: u64) -> Option<Vec<Replaced>> {
        self.after(revision, current)?
            .map(|entry| entry.replaced)
            .collect()
    }

    /// The lines changed after `revision`, up to `current`, as one span.
    pub(super) fn lines_since(&self, revision: u64, current: u64) -> Option<LineChange> {
        let empty = LineChange {
            start: 0,
            old_end: 0,
            new_end: 0,
        };
        self.after(revision, current)?
            .try_fold(empty, |merged, entry| {
                Some(merged.then(entry.replaced?.lines()))
            })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{LineChange, Replaced};
    use crate::document::{Document, Position, Range};

    /// Asserts that what `document` reports as left alone since each earlier
    /// revision really is the same bytes in `history[revision]`.
    fn assert_honest(document: &Document, history: &[String]) {
        let now = document.text();
        for (revision, then) in (0_u64..).zip(history) {
            let unchanged = document.unchanged_since(revision).unwrap();
            assert!(unchanged.prefix <= then.len().min(now.len()));
            assert!(unchanged.suffix <= then.len().min(now.len()));
            assert_eq!(
                then.as_bytes()[..unchanged.prefix],
                now.as_bytes()[..unchanged.prefix],
                "prefix since revision {revision}",
            );
            assert_eq!(
                then.as_bytes()[then.len() - unchanged.suffix..],
                now.as_bytes()[now.len() - unchanged.suffix..],
                "suffix since revision {revision}",
            );
        }
    }

    #[test]
    fn what_is_reported_unchanged_is_unchanged_long_after_the_log_fills() {
        let mut document = Document::new("abcdefghij\nklmnopqrst\n");
        let mut history = vec![document.text()];
        // Far more edits than the log holds, at varied places, so merged
        // entries are asked about from inside their span as well as outside.
        for step in 0..100_usize {
            let offset = (step * 7) % (document.byte_count() + 1);
            let at = document.offset_to_position(offset).unwrap();
            if step % 3 == 2 && offset + 2 <= document.byte_count() {
                let end = document.offset_to_position(offset + 2).unwrap();
                document.delete(Range::new(at, end)).unwrap();
            } else {
                let text = if step % 2 == 0 { "x" } else { "\n" };
                document.insert(at, text).unwrap();
            }
            assert_eq!(document.revision(), history.len() as u64);
            history.push(document.text());
            assert_honest(&document, &history);
        }
    }

    #[test]
    fn nothing_changed_since_now_and_unknown_revisions_are_refused() {
        let mut document = Document::new("hello");
        let length = document.byte_count();
        let unchanged = document.unchanged_since(0).unwrap();
        assert_eq!((unchanged.prefix, unchanged.suffix), (length, length));
        document.insert(Position::new(0, 5), "!").unwrap();
        assert!(document.unchanged_since(2).is_none(), "from the future");
        let replaced = Document::continuing_from("other", &document);
        assert!(replaced.unchanged_since(0).is_none(), "from the old text");
        assert!(replaced.unchanged_since(replaced.revision()).is_some());
    }

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

    /// A document answers lines and positions only from revisions it has had,
    /// and not across an edit it could not describe.
    #[test]
    fn lines_are_answered_only_from_revisions_the_document_had() {
        let mut document = Document::new("one\ntwo\nthree\n");
        document.insert(Position::new(1, 0), "x").unwrap();
        assert_eq!(document.lines_changed_since(0), Some(change(1, 2, 2)));
        assert_eq!(document.lines_changed_since(5), None, "not reached yet");
        assert!(
            document
                .lines_changed_since(1)
                .is_some_and(|lines| lines.is_empty())
        );
        let replaced = Document::continuing_from("other", &document);
        assert_eq!(replaced.lines_changed_since(0), None, "from the old text");
        document.insert(Position::new(0, 3), "\r").unwrap();
        assert_eq!(
            document.lines_changed_since(1),
            None,
            "beside a carriage return"
        );
        assert!(document.lines_changed_since(2).is_some());
    }

    /// Past the log's capacity the oldest edits merge, and the merged span
    /// still covers every line they changed.
    #[test]
    fn edits_beyond_the_log_still_answer_with_a_covering_span() {
        let mut document = Document::new(&"line\n".repeat(100));
        for line in (0..40).map(|step| step * 2) {
            document.insert(Position::new(line, 0), "x").unwrap();
        }
        let lines = document.lines_changed_since(0).unwrap();
        assert_eq!(lines.start, 0);
        assert!(lines.old_end >= 79 && lines.new_end == lines.old_end);
    }

    /// Text inserted at a line's start pushes it down; after its start it
    /// does not; a line the edit replaced has nowhere to go.
    #[test]
    fn a_line_start_follows_the_text_around_it() {
        let at = |line, column| Position::new(line, column);
        let pasted_above = Replaced {
            start: at(3, 0),
            old_end: at(3, 0),
            new_end: at(5, 0),
        };
        assert_eq!(pasted_above.follow_line_start(3), Some(5));
        assert_eq!(pasted_above.follow_line_start(2), Some(2));
        let enter_at_end = Replaced {
            start: at(3, 9),
            old_end: at(3, 9),
            new_end: at(4, 0),
        };
        assert_eq!(enter_at_end.follow_line_start(3), Some(3));
        assert_eq!(enter_at_end.follow_line_start(4), Some(5));
        let deleted = Replaced {
            start: at(1, 0),
            old_end: at(4, 2),
            new_end: at(1, 0),
        };
        assert_eq!(deleted.follow_line_start(2), None);
        assert_eq!(deleted.follow_line_start(4), None);
        assert_eq!(deleted.follow_line_start(6), Some(3));
    }
}
