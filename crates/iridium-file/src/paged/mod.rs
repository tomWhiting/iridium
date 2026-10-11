//! Disk-backed UTF-8 editing with 64-bit spans, bounded reads and streamed output.
//!
//! This is a storage foundation, not yet the editor's `Document` backend.
//! Capture copies a stable source into an unnamed temporary file; callers must
//! run it on a worker and can cancel between 64 KiB blocks. Later external writes
//! cannot alter that backing. Edits append only their new bytes; undo retains
//! spans rather than entire document copies. Capture is not a filesystem-wide
//! transaction: metadata checks detect ordinary concurrent source modification.
//! Publishing output, save conflict checks and UI integration belong
//! to subsequent layers. `write_to` alone does not publish or mark a file saved.
mod lines;
mod pieces;
mod reader;
mod source;
pub use reader::PagedReader;
mod utf8;

pub use lines::{IndexProgress, LineIndex, LineSpan, LineWindow};

use pieces::{Piece, Store};
use source::Source;
use std::io;
use std::ops::Range;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Maximum materialized source window; full output uses `write_to`.
pub const MAX_WINDOW_BYTES: u64 = 1024 * 1024;

/// Cooperative cancellation shared with a worker thread.
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    /// Ask any operation using this token to stop at its next I/O block.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    fn check(&self) -> io::Result<()> {
        if self.is_cancelled() {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "operation cancelled",
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone)]
struct Change {
    at: u64,
    removed: Vec<Piece>,
    inserted: Vec<Piece>,
    before: u64,
    after: u64,
}

/// Editable byte spans over an immutable source snapshot and append-only edits.
pub struct PagedText {
    identity: Arc<()>,
    source: Source,
    pieces: Vec<Piece>,
    len: u64,
    undo: Vec<Change>,
    redo: Vec<Change>,
    state: u64,
    next_state: u64,
    saved: u64,
}
impl PagedText {
    /// Capture a UTF-8 file with bounded memory, reporting copied byte counts.
    /// This performs disk I/O; run it outside the UI thread.
    pub fn capture(
        path: &Path,
        cancel: &Cancellation,
        progress: impl FnMut(u64),
    ) -> io::Result<Self> {
        let (source, len) = Source::capture(path, cancel, progress)?;
        let pieces = if len == 0 {
            Vec::new()
        } else {
            vec![Piece {
                store: Store::Original,
                offset: 0,
                len,
            }]
        };
        Ok(Self {
            identity: Arc::new(()),
            source,
            pieces,
            len,
            undo: Vec::new(),
            redo: Vec::new(),
            state: 0,
            next_state: 1,
            saved: 0,
        })
    }
    /// Byte length, including any BOM and original line-ending bytes.
    pub const fn len(&self) -> u64 {
        self.len
    }
    /// Whether the document contains no bytes.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Whether the current history state is the acknowledged save point.
    pub const fn is_at_save_point(&self) -> bool {
        self.state == self.saved
    }
    /// Acknowledge a save only after the caller has successfully published it.
    pub const fn mark_saved(&mut self) {
        self.saved = self.state;
    }
    /// Number of current spans, independent of source byte length.
    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }
    /// Read a bounded UTF-8 range. Endpoints must lie on character boundaries.
    pub fn read(&mut self, range: Range<u64>) -> io::Result<String> {
        if range.is_empty() && (range.start > self.len || !self.boundary(range.start)?) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "read endpoint is not a UTF-8 boundary",
            ));
        }
        let bytes = self.read_bytes(range)?;
        String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
    }
    fn range_length(&self, range: &Range<u64>) -> io::Result<u64> {
        range
            .end
            .checked_sub(range.start)
            .filter(|_| range.end <= self.len)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "range is outside the document")
            })
    }
    fn read_bytes(&mut self, range: Range<u64>) -> io::Result<Vec<u8>> {
        let length = self.range_length(&range)?;
        if length > MAX_WINDOW_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "read exceeds 1 MiB window limit",
            ));
        }
        let size = usize::try_from(length).map_err(io::Error::other)?;
        let mut bytes = vec![0; size];
        let mut cursor = 0;
        let mut written = 0;
        for &piece in &self.pieces {
            let end = cursor + piece.len;
            let start = cursor.max(range.start);
            let stop = end.min(range.end);
            if start < stop {
                let count = usize::try_from(stop - start).map_err(io::Error::other)?;
                self.source.read(
                    Piece {
                        offset: piece.offset + (start - cursor),
                        len: stop - start,
                        ..piece
                    },
                    &mut bytes[written..written + count],
                )?;
                written += count;
            }
            cursor = end;
            if cursor >= range.end {
                break;
            }
        }
        if written != size {
            return Err(io::Error::other(
                "source spans do not cover requested range",
            ));
        }
        Ok(bytes)
    }
    fn boundary(&mut self, offset: u64) -> io::Result<bool> {
        if offset == self.len {
            return Ok(true);
        }
        let byte = self.read_bytes(
            offset
                ..offset
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("offset overflow"))?,
        )?;
        Ok(byte.first().is_some_and(|byte| byte & 0xc0 != 0x80))
    }
    /// Replace a UTF-8 byte range as one undoable change. Failed I/O leaves the
    /// document and history unchanged; unreachable appended bytes may remain.
    pub fn replace(&mut self, range: Range<u64>, text: &str) -> io::Result<()> {
        let removed_length = self.range_length(&range)?;
        if !self.boundary(range.start)? || !self.boundary(range.end)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "edit endpoints must be UTF-8 boundaries inside the document",
            ));
        }
        if range.is_empty() && text.is_empty() {
            return Ok(());
        }
        let next = self
            .next_state
            .checked_add(1)
            .ok_or_else(|| io::Error::other("history identity exhausted"))?;
        self.len
            .checked_sub(removed_length)
            .and_then(|length| length.checked_add(text.len() as u64))
            .ok_or_else(|| io::Error::other("document size exceeds u64"))?;
        let inserted = if text.is_empty() {
            Vec::new()
        } else {
            vec![self.source.append(text)?]
        };
        let (pieces, removed) = pieces::splice(&self.pieces, range.clone(), &inserted)?;
        let len = pieces::length(&pieces)?;
        self.undo.push(Change {
            at: range.start,
            removed,
            inserted,
            before: self.state,
            after: self.next_state,
        });
        self.pieces = pieces;
        self.len = len;
        self.state = self.next_state;
        self.next_state = next;
        self.redo.clear();
        Ok(())
    }
    /// Undo a change without loading the deleted bytes into memory.
    pub fn undo(&mut self) -> io::Result<bool> {
        let Some(change) = self.undo.last().cloned() else {
            return Ok(false);
        };
        let (pieces, _) = pieces::splice(
            &self.pieces,
            change.at..change.at + pieces::length(&change.inserted)?,
            &change.removed,
        )?;
        self.len = pieces::length(&pieces)?;
        self.pieces = pieces;
        self.state = change.before;
        self.undo.truncate(self.undo.len() - 1);
        self.redo.push(change);
        Ok(true)
    }
    /// Redo the most recently undone change.
    pub fn redo(&mut self) -> io::Result<bool> {
        let Some(change) = self.redo.last().cloned() else {
            return Ok(false);
        };
        let (pieces, _) = pieces::splice(
            &self.pieces,
            change.at..change.at + pieces::length(&change.removed)?,
            &change.inserted,
        )?;
        self.len = pieces::length(&pieces)?;
        self.pieces = pieces;
        self.state = change.after;
        self.redo.truncate(self.redo.len() - 1);
        self.undo.push(change);
        Ok(true)
    }
    /// Stream exact bytes using a 64 KiB buffer. Errors/cancellation propagate;
    /// a caller writing a real destination must provide atomic publication.
    pub fn write_to(
        &mut self,
        output: &mut impl io::Write,
        cancel: &Cancellation,
    ) -> io::Result<()> {
        cancel.check()?;
        let mut block = vec![0; source::BLOCK_BYTES];
        for &piece in &self.pieces {
            self.source.stream(piece, output, cancel, &mut block)?;
        }
        cancel.check()
    }
}

#[cfg(test)]
mod tests;
