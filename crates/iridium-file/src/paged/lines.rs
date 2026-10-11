//! Progressive sparse line indexing. Checkpoints bound lookup scans by bytes,
//! including when one line itself is several GiB long.
mod repair;
#[cfg(test)]
mod repair_tests;
mod scan;
#[cfg(test)]
mod tests;
mod window;

use super::{Cancellation, PagedText, source::BLOCK_BYTES};
use scan::Scan;
use std::{io, ops::Range, sync::Arc};
pub use window::LineWindow;

/// How much of this revision has been indexed. Counts include the last line,
/// which may be empty; an unfinished count is only a known prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexProgress {
    /// Bytes inspected in this revision.
    pub indexed_bytes: u64,
    /// Total bytes in this revision.
    pub total_bytes: u64,
    /// Number of lines whose starting positions are known.
    pub known_lines: u64,
    /// Whether the whole revision has been indexed.
    pub complete: bool,
}

/// An indexed line's raw byte range, including its original line ending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineSpan {
    /// Known bytes belonging to the line. An incomplete range ends at the
    /// indexed frontier, potentially inside a UTF-8 character.
    pub bytes: Range<u64>,
    /// Whether the end of this line is known.
    pub complete: bool,
}

/// A sparse line index bound to one document and content history state.
///
/// Each advance examines at most 64 KiB and retains one small checkpoint.
/// Memory grows with file bytes / 64 KiB, not the number or length of lines.
/// LF, CRLF and bare CR match the editor's line convention. An index from a
/// different document or content state is rejected, even if lengths match.
/// Undo back to the exact indexed state permits reuse. `refresh` repairs one
/// observed edit/undo/redo with bounded scanning and suffix checkpoint reuse.
#[derive(Debug)]
pub struct LineIndex {
    identity: Arc<()>,
    state: u64,
    total: u64,
    checkpoints: Vec<Scan>,
    frontier: Scan,
    complete: bool,
    repair: Option<repair::Repair>,
}
impl LineIndex {
    /// Start an empty index for the current content state, without file I/O.
    pub fn new(text: &PagedText) -> Self {
        Self {
            identity: Arc::clone(&text.identity),
            state: text.state,
            total: text.len(),
            checkpoints: vec![Scan::default()],
            frontier: Scan::default(),
            complete: text.is_empty(),
            repair: None,
        }
    }
    /// Current known prefix, without doing additional indexing.
    pub const fn progress(&self) -> IndexProgress {
        IndexProgress {
            indexed_bytes: self.frontier.offset,
            total_bytes: self.total,
            known_lines: self.frontier.line + 1,
            complete: self.complete,
        }
    }
    /// Number of retained checkpoints, for memory budgeting/diagnostics.
    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }
    /// Inspect at most 64 KiB. Cancellation and read failure leave the frontier
    /// unchanged; callers can resume with a fresh cancellation token.
    pub fn advance(
        &mut self,
        text: &mut PagedText,
        cancel: &Cancellation,
    ) -> io::Result<IndexProgress> {
        self.validate(text)?;
        cancel.check()?;
        if self.complete {
            return Ok(self.progress());
        }
        let limit = self
            .repair
            .as_ref()
            .map_or(self.total, |repair| repair.new_offset);
        let end = self
            .frontier
            .offset
            .saturating_add(BLOCK_BYTES as u64)
            .min(limit);
        let bytes = text.read_bytes(self.frontier.offset..end)?;
        let mut next = self.frontier;
        for byte in bytes {
            next.feed(byte);
        }
        let complete = end == self.total;
        if complete {
            next.finish();
        }
        let tail = self.repaired_tail(next, cancel)?;
        cancel.check()?;
        if let Some(tail) = tail {
            let frontier = tail
                .last()
                .copied()
                .ok_or_else(|| io::Error::other("repair has no suffix"))?;
            self.checkpoints.extend(tail);
            self.frontier = frontier;
            self.complete = true;
            self.repair = None;
        } else {
            self.checkpoints.push(next);
            self.frontier = next;
            self.complete = complete;
        }
        Ok(self.progress())
    }
    fn validate(&self, text: &PagedText) -> io::Result<()> {
        if !Arc::ptr_eq(&self.identity, &text.identity) || self.state != text.state {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "line index belongs to a different document or content state",
            ));
        }
        Ok(())
    }
    fn line_start(
        &self,
        text: &mut PagedText,
        line: u64,
        cancel: &Cancellation,
    ) -> io::Result<Option<u64>> {
        if line > self.frontier.line {
            return Ok(None);
        }
        let upper = self
            .checkpoints
            .partition_point(|checkpoint| checkpoint.line <= line);
        let checkpoint = self
            .checkpoints
            .get(upper.saturating_sub(1))
            .ok_or_else(|| io::Error::other("line index has no initial checkpoint"))?;
        let mut cursor = *checkpoint;
        if cursor.line == line {
            return Ok(Some(cursor.start));
        }
        let end = cursor
            .offset
            .saturating_add(BLOCK_BYTES as u64)
            .min(self.frontier.offset);
        cancel.check()?;
        for byte in text.read_bytes(cursor.offset..end)? {
            cursor.feed(byte);
            if cursor.line == line {
                return Ok(Some(cursor.start));
            }
        }
        if self.complete && end == self.total {
            cursor.finish();
            if cursor.line == line {
                return Ok(Some(cursor.start));
            }
        }
        Err(io::Error::other(
            "line index checkpoint did not cover requested line",
        ))
    }
    /// Find a known line without scanning a whole long line. `None` means the
    /// line does not exist in the indexed prefix, not necessarily the document.
    pub fn line_span(
        &self,
        text: &mut PagedText,
        line: u64,
        cancel: &Cancellation,
    ) -> io::Result<Option<LineSpan>> {
        self.validate(text)?;
        cancel.check()?;
        let Some(start) = self.line_start(text, line, cancel)? else {
            return Ok(None);
        };
        let next_line = line
            .checked_add(1)
            .ok_or_else(|| io::Error::other("line number overflow"))?;
        let next = self.line_start(text, next_line, cancel)?;
        Ok(Some(LineSpan {
            bytes: start..next.unwrap_or(self.frontier.offset),
            complete: next.is_some() || self.complete,
        }))
    }
}
