//! What recent edits left alone, so a reader can catch up without a rescan.
//!
//! Every edit to a [`Document`](super::Document) leaves some bytes before it
//! and some after it untouched. Two of those descriptions compose by taking
//! the smaller of each, so the log can merge its oldest entries when full
//! and still answer for any revision the document has had: conservatively,
//! never wrongly.

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

/// Entries kept before the oldest two merge. A face asks every frame, so it
/// is rarely more than a keystroke's few edits behind.
const CAPACITY: usize = 16;

/// One edit, or a merged run of them, ending at revision `after`.
#[derive(Debug, Clone, Copy)]
struct Entry {
    after: u64,
    unchanged: Unchanged,
}

/// The edits since a document was made, newest kept separately.
///
/// Inline and `Copy`, so cloning a document stays an allocation-free copy.
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
        };
        Self {
            floor,
            entries: [empty; CAPACITY],
            oldest: 0,
            count: 0,
        }
    }

    /// Records the edit that produced revision `after`.
    pub(super) fn record(&mut self, after: u64, unchanged: Unchanged) {
        if self.count == CAPACITY {
            // Merge the oldest into the next, which then covers both.
            let oldest = self.entries[self.oldest].unchanged;
            self.oldest = (self.oldest + 1) % CAPACITY;
            let next = &mut self.entries[self.oldest].unchanged;
            *next = next.and(oldest);
            self.count -= 1;
        }
        self.entries[(self.oldest + self.count) % CAPACITY] = Entry { after, unchanged };
        self.count += 1;
    }

    /// What the edits after `revision` left alone, up to `current`, in a
    /// document now `len` bytes long. `None` when `revision` is not one the
    /// document has had.
    pub(super) fn since(&self, revision: u64, current: u64, len: usize) -> Option<Unchanged> {
        if revision < self.floor || revision > current {
            return None;
        }
        let mut unchanged = Unchanged {
            prefix: len,
            suffix: len,
        };
        for index in (0..self.count).rev() {
            let entry = self.entries[(self.oldest + index) % CAPACITY];
            if entry.after <= revision {
                break;
            }
            unchanged = unchanged.and(entry.unchanged);
        }
        Some(unchanged)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
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
}
