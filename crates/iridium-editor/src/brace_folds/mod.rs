//! Brace-matched fold regions, maintained across edits instead of rescanned.
//!
//! # Why this exists
//!
//! Without a grammar there is no parse tree, so folds are found by matching
//! braces over the document text. That scan is O(document): every byte, every
//! keystroke. In the browser — the one face that ships without tree-sitter, see
//! `docs/WASM-SYNTAX-SPIKE.md` — that was the whole of fold detection, and it
//! ran on every content change.
//!
//! [`BraceFoldCache`] makes the same scan incremental, on the same principle as
//! the tree-sitter fold cache in `iridium-syntax`: keep what the edit did not
//! reach, redo only what it did. The mechanism has to differ because the inputs
//! do. Tree-sitter hands its cache a set of changed byte ranges and guarantees
//! the trees agree outside them; a left-to-right text scan has no such oracle,
//! and its own state — how many braces are open, whether a string or a comment
//! is in progress — is carried forward from every byte to the next. So the
//! boundary of "what the edit reached" is not given, it is **measured**.
//!
//! # How
//!
//! 1. The scanner state at the start of every line is kept (see
//!    [`scan::LineState`]), and the open-brace stack is kept at intervals.
//! 2. An edit resumes the scan from the last recorded stack at or before the
//!    first changed line.
//! 3. The scan runs forward until, at some line boundary past the edit, its live
//!    state equals the state recorded for the corresponding line of the pre-edit
//!    document. From there the text is identical and the state is identical, so
//!    the rest of the old scan is by definition the rest of the new one. That is
//!    the **convergence point**, and for an edit within one line it is the very
//!    next line boundary.
//! 4. Everything closed before the resume point is kept as it was, everything
//!    closed after the convergence point is kept with its line numbers moved,
//!    and only the span between the two is rescanned.
//!
//! Line numbers of braces still open at the convergence point are not shifted by
//! arithmetic — they are read out of the live stack by depth. The two stacks are
//! equal at that point by construction, which is exactly what the convergence
//! test established, so this is a lookup rather than a guess. It has to be:
//! arithmetic cannot describe where a line the edit ran through ended up.
//!
//! # What this does not make sublinear
//!
//! The published region list is rebuilt each time, and the per-line state array
//! is spliced. Both are linear in the document, and both are memory traffic over
//! plain integers rather than a byte-by-byte scan — the same shape the
//! tree-sitter cache has, which also walks its whole retained region list per
//! edit. What is now proportional to the edit is the *scanning*, which was the
//! expensive part.

mod incremental;
mod scan;

use scan::{LineState, has_lone_carriage_return, lines_from, scan_line};

pub use scan::BraceRegion;

#[cfg(test)]
mod tests;

/// How far apart the recorded open-brace stacks are, in lines.
///
/// The only cost of a wider spacing is a longer resume scan; the only cost of a
/// narrower one is memory. At 128 an edit rescans at most 128 lines it did not
/// have to, which is microseconds, and a 100,000-line document holds under a
/// thousand stacks.
const CHECKPOINT_INTERVAL: usize = 128;

/// The deepest brace nesting whose stack is worth recording.
///
/// Recording a stack costs memory proportional to the nesting at that point, so
/// a pathological document — a hundred thousand lines each opening a brace —
/// would otherwise cost memory proportional to the *square* of its length.
/// Beyond this depth the stack is simply not recorded, which is not a
/// correctness question: the resume scan falls back to an earlier stack and does
/// more work. No real source nests 256 braces.
const MAX_CHECKPOINT_DEPTH: usize = 256;

/// One brace pair, with what the incremental update needs to move it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrackedBrace {
    /// Line of the opening `{`.
    open_line: usize,
    /// How many braces were open beneath this one.
    ///
    /// This is what lets an opening line be recovered from a live stack rather
    /// than computed from an edit's line delta, which cannot describe a line the
    /// edit ran through.
    open_depth: usize,
    /// Line of the matching `}`.
    end_line: usize,
}

/// The open-brace stack recorded at one line, so a scan can resume there.
#[derive(Debug, Clone)]
struct Checkpoint {
    /// The line the stack describes the start of.
    line: usize,
    /// The byte offset that line begins at.
    byte: usize,
    /// The line of every open `{`, innermost last.
    stack: Vec<usize>,
}

/// The rows and bytes one edit moved.
///
/// Rows are the document's, and are what the scan works in; bytes are needed
/// only to move recorded offsets and to inspect the replacement text. This is
/// the subset of a tree-sitter `InputEdit` that a text scan can use, named
/// separately so this module compiles — and is tested — in every feature
/// configuration rather than only in the one that has no parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineEdit {
    /// Byte offset where the edit begins, in both documents.
    pub start_byte: usize,
    /// Byte offset where the replaced text ended, in the pre-edit document.
    pub old_end_byte: usize,
    /// Byte offset where the new text ends, in the post-edit document.
    pub new_end_byte: usize,
    /// Row the edit begins on, in both documents.
    pub start_row: usize,
    /// Row the replaced text ended on, in the pre-edit document.
    pub old_end_row: usize,
    /// Row the new text ends on, in the post-edit document.
    pub new_end_row: usize,
}

/// Brace-matched fold regions for one document, maintained incrementally.
#[derive(Debug, Default)]
pub struct BraceFoldCache {
    /// The published list: every foldable pair, sorted by opening line.
    regions: Vec<BraceRegion>,
    /// Every foldable pair, in the order its closing brace was met.
    tracked: Vec<TrackedBrace>,
    /// The scanner state at the start of each line, plus one for the end of the
    /// document — so `states[n]` is the state a scan of line `n` starts from,
    /// for every `n` up to and including the line count.
    states: Vec<LineState>,
    /// Open-brace stacks, ascending by line, at most one per
    /// [`CHECKPOINT_INTERVAL`] lines.
    checkpoints: Vec<Checkpoint>,
    /// How many lines the last scan found.
    line_count: usize,
    /// Whether any source has been scanned into this cache.
    ///
    /// An empty region list cannot answer that on its own — a document with no
    /// braces has one too — and an incremental update against a cache that never
    /// saw a document would publish only what the edit's own span happened to
    /// contain.
    primed: bool,
    /// Whether the last full scan saw a `\r` that no `\n` follows.
    ///
    /// Such a document is numbered differently by the scanner and by the
    /// document that describes edits against it, so it is only ever scanned
    /// whole. See [`has_lone_carriage_return`].
    lone_carriage_return: bool,
    /// How many lines have been scanned over this cache's whole life.
    lines_scanned: u64,
}

impl BraceFoldCache {
    /// Creates an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this cache has ever been given a document to work from.
    #[must_use]
    pub const fn is_primed(&self) -> bool {
        self.primed
    }

    /// The regions currently held: the complete, whole-document list, sorted by
    /// opening line.
    #[must_use]
    pub fn regions(&self) -> &[BraceRegion] {
        &self.regions
    }

    /// How many lines this cache has scanned since it was created.
    ///
    /// Exposed for the same reason the tree-sitter cache exposes its node count:
    /// "an edit does not read the whole document" is a claim about how much work
    /// happens, and a claim nothing can observe is a claim nothing can hold to.
    /// Wall-clock timing would measure the machine; this measures the algorithm.
    #[must_use]
    pub const fn lines_scanned(&self) -> u64 {
        self.lines_scanned
    }

    /// Rescans `source` from the beginning, reporting whether the regions
    /// changed.
    ///
    /// Always correct, and the right entry point whenever the document's
    /// relationship to the previous one is unknown — a file load, a wholesale
    /// replacement, a first scan.
    pub fn rebuild(&mut self, source: &str) -> bool {
        self.tracked.clear();
        self.states.clear();
        self.checkpoints.clear();

        let mut state = LineState::default();
        let mut stack = Vec::new();
        let mut line_number = 0;
        let mut last_checkpoint = None;

        for (offset, line) in lines_from(source, 0) {
            self.states.push(state);
            record_checkpoint(
                &mut self.checkpoints,
                &mut last_checkpoint,
                line_number,
                offset,
                &stack,
            );
            scan_line(line, line_number, &mut state, &mut stack, &mut self.tracked);
            line_number += 1;
        }
        self.states.push(state);

        self.line_count = line_number;
        self.lines_scanned += u64::try_from(line_number).unwrap_or(u64::MAX);
        self.lone_carriage_return = has_lone_carriage_return(source.as_bytes(), 0..source.len());
        self.primed = true;
        self.publish()
    }

    /// Rebuilds the published list from the raw one, reporting any change.
    ///
    /// The sort is by opening line and is stable, so pairs opening on one line
    /// keep the order their closing braces were met in. That is the order the
    /// whole-document scan this replaces produced, and reproducing it exactly is
    /// what makes the incremental result indistinguishable from a full one.
    fn publish(&mut self) -> bool {
        let mut regions: Vec<BraceRegion> = self
            .tracked
            .iter()
            .map(|brace| BraceRegion {
                start_line: brace.open_line,
                end_line: brace.end_line,
            })
            .collect();
        regions.sort_by_key(|region| region.start_line);

        if regions == self.regions {
            return false;
        }
        self.regions = regions;
        true
    }
}

/// Records the open-brace stack at `line`, if one is due and worth keeping.
fn record_checkpoint(
    into: &mut Vec<Checkpoint>,
    last: &mut Option<usize>,
    line: usize,
    byte: usize,
    stack: &[usize],
) {
    if stack.len() > MAX_CHECKPOINT_DEPTH {
        return;
    }
    if last.is_some_and(|last| line < last + CHECKPOINT_INTERVAL) {
        return;
    }

    *last = Some(line);
    into.push(Checkpoint {
        line,
        byte,
        stack: stack.to_vec(),
    });
}

/// Adds a signed offset to an index, or `None` if the result is not one.
const fn shift_line(value: usize, delta: isize) -> Option<usize> {
    value.checked_add_signed(delta)
}

/// An index as a signed offset, or `None` if it does not fit.
fn signed(value: usize) -> Option<isize> {
    isize::try_from(value).ok()
}
