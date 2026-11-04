//! Whether a document still reads exactly as a reference text, such as the
//! file it was opened from, kept current edit by edit.
//!
//! A face asks this every frame to mark a buffer modified. Comparing the
//! whole text each time costs the file's size per frame, and the length
//! alone settles only the edits that change it: typing over a selection or
//! moving a line keeps the length. So this keeps what it last learnt, how
//! much of each end agrees and whether a difference was seen there, and on
//! each new revision narrows that by what the edits left alone. Only the
//! stretch nobody has vouched for is read.

use std::cell::Cell;

use ropey::Rope;

use super::Document;
use super::changes::Unchanged;

/// The comparison of one document against one reference text.
///
/// Holds no text: the caller passes both on every question, with a stamp
/// that changes whenever the reference does. Everything learnt is keyed to
/// the document's [`id`](Document::id) and that stamp, so a different
/// document or a new reference starts again from nothing.
#[derive(Debug, Default)]
pub struct Divergence {
    known: Cell<Option<Known>>,
    bytes_read: Cell<u64>,
}

/// What the last answer established.
#[derive(Debug, Clone, Copy)]
struct Known {
    document: u64,
    revision: u64,
    reference: u64,
    /// At least this many bytes at the start agree.
    head: usize,
    /// At least this many bytes at the end agree.
    tail: usize,
    /// The byte just after the agreeing start differs, so `head` is exact.
    head_differs: bool,
    /// The byte just before the agreeing end differs, so `tail` is exact.
    tail_differs: bool,
}

impl Known {
    /// Nothing learnt yet about this pair.
    const fn nothing(document: &Document, reference: u64) -> Self {
        Self {
            document: document.id(),
            revision: document.revision(),
            reference,
            head: 0,
            tail: 0,
            head_differs: false,
            tail_differs: false,
        }
    }

    /// The same knowledge after edits that left `unchanged` alone. A
    /// difference stays witnessed only if the edits stopped short of it.
    fn after(self, unchanged: Unchanged, revision: u64) -> Self {
        Self {
            revision,
            head: self.head.min(unchanged.prefix),
            tail: self.tail.min(unchanged.suffix),
            head_differs: self.head_differs && self.head < unchanged.prefix,
            tail_differs: self.tail_differs && self.tail < unchanged.suffix,
            ..self
        }
    }
}

impl Divergence {
    /// A comparison that knows nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `document` differs from `reference`, the text stamped
    /// `reference_stamp`.
    ///
    /// The stamp must change whenever the reference text does; equal stamps
    /// are taken to mean equal text. The answer is always exact. Its cost is
    /// the bytes no earlier answer vouched for: nothing for a frame with no
    /// edit and the edited stretch for an edit. The whole text is read for a
    /// new document or reference, and an edit that undoes the outermost
    /// differences at both ends reads on to the next ones.
    #[must_use]
    pub fn differs(&self, document: &Document, reference: &str, reference_stamp: u64) -> bool {
        let rope = document.rope();
        let reference = reference.as_bytes();
        let mut known = Known::nothing(document, reference_stamp);
        if let Some(previous) = self.known.get()
            && previous.document == document.id()
            && previous.reference == reference_stamp
        {
            if previous.revision == document.revision() {
                known = previous;
            } else if let Some(unchanged) = document.unchanged_since(previous.revision) {
                known = previous.after(unchanged, document.revision());
                self.witness_edit(&mut known, rope, reference, unchanged);
            }
        }
        let (known, differs) = self.settle(known, rope, reference);
        self.known.set(Some(known));
        differs
    }

    /// Bytes compared since this was made.
    ///
    /// The observable form of the claim that an edit costs its own size and
    /// not the file's, measured in the algorithm's unit rather than time.
    #[must_use]
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read.get()
    }

    /// Looks inside the edited stretch for a new witness at each end the
    /// edit took one from. Without this, a later edit that undoes the one
    /// surviving witness would have to read across the file to find another.
    fn witness_edit(&self, known: &mut Known, rope: &Rope, reference: &[u8], edit: Unchanged) {
        let len = rope.len();
        let edit_end = len.saturating_sub(edit.suffix);
        if !known.head_differs && known.head == edit.prefix {
            let end = edit_end.min(reference.len());
            if known.head < end {
                match self.first_difference(rope, reference, known.head, end) {
                    Some(at) => {
                        known.head = at;
                        known.head_differs = true;
                    },
                    None => known.head = end,
                }
            }
        }
        if !known.tail_differs && known.tail == edit.suffix {
            // Aligned at the end, so no byte before the reference's start;
            // with equal lengths the stretch the head just read agrees too.
            let mut floor = edit.prefix.max(len.saturating_sub(reference.len()));
            if len == reference.len() {
                floor = floor.max(known.head);
            }
            let top = len - known.tail;
            if floor < top {
                match self.last_difference(rope, reference, floor, top) {
                    Some(at) => {
                        known.tail = len - 1 - at;
                        known.tail_differs = true;
                    },
                    None => known.tail = len - floor,
                }
            }
        }
    }

    /// Settles the answer, reading only the stretch between the two agreeing
    /// ends when no difference is already witnessed.
    fn settle(&self, mut known: Known, rope: &Rope, reference: &[u8]) -> (Known, bool) {
        let len = rope.len();
        if len != reference.len() || known.head_differs || known.tail_differs {
            return (known, true);
        }
        let end = len.saturating_sub(known.tail);
        if known.head >= end {
            known.head = len;
            known.tail = len;
            return (known, false);
        }
        let Some(first) = self.first_difference(rope, reference, known.head, end) else {
            known.head = len;
            known.tail = len;
            return (known, false);
        };
        // Find the last difference too, so that undoing the first one later
        // still leaves a witness without reading back across the file.
        let last = self
            .last_difference(rope, reference, first, end)
            .unwrap_or(first);
        known.head = first;
        known.head_differs = true;
        known.tail = len - 1 - last;
        known.tail_differs = true;
        (known, true)
    }

    /// The first byte in `start..end` where `rope` and `reference` differ.
    fn first_difference(
        &self,
        rope: &Rope,
        reference: &[u8],
        start: usize,
        end: usize,
    ) -> Option<usize> {
        let mut at = start;
        while at < end {
            let (chunk, chunk_start) = rope.chunk(at);
            let stop = (chunk_start + chunk.len()).min(end);
            // Lengths are equal and `at` is inside the chunk, so both reads
            // succeed; were either to fail, reporting a difference is the
            // side that cannot lose work.
            let (Some(ours), Some(theirs)) = (
                chunk.as_bytes().get(at - chunk_start..stop - chunk_start),
                reference.get(at..stop),
            ) else {
                return Some(at);
            };
            self.count(stop - at);
            if let Some(offset) = ours.iter().zip(theirs).position(|(a, b)| a != b) {
                return Some(at + offset);
            }
            at = stop;
        }
        None
    }

    /// The last byte in `start..end` where `rope` differs from `reference`
    /// with the two aligned at their ends. `start` must leave no byte before
    /// the reference's first.
    fn last_difference(
        &self,
        rope: &Rope,
        reference: &[u8],
        start: usize,
        end: usize,
    ) -> Option<usize> {
        let len = rope.len();
        let mut at = end;
        while at > start {
            let (chunk, chunk_start) = rope.chunk(at - 1);
            let from = chunk_start.max(start);
            let theirs = (from + reference.len())
                .checked_sub(len)
                .and_then(|first| reference.get(first..first + (at - from)));
            let (Some(ours), Some(theirs)) = (
                chunk.as_bytes().get(from - chunk_start..at - chunk_start),
                theirs,
            ) else {
                return Some(at - 1);
            };
            self.count(at - from);
            if let Some(offset) = ours.iter().zip(theirs).rposition(|(a, b)| a != b) {
                return Some(from + offset);
            }
            at = from;
        }
        None
    }

    fn count(&self, bytes: usize) {
        self.bytes_read
            .set(self.bytes_read.get().saturating_add(bytes as u64));
    }
}

#[cfg(test)]
#[path = "divergence_tests.rs"]
mod tests;
