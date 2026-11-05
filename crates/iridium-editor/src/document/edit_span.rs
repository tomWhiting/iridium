//! The byte span a command touches, in tree-sitter's coordinates.
//!
//! A [`Command`] describes an edit in line/column terms, because that is what
//! the editor reasons in. Everything that consumes an edit *incrementally* —
//! tree-sitter's `InputEdit`, a highlight-span shift, a remote-edit protocol —
//! wants byte offsets and `(row, byte_column)` points instead. This module is
//! the one conversion between the two.
//!
//! It lives in the kernel rather than in a binding because both the kernel's
//! own retained syntax tree and the web face's need the same answer, and two
//! implementations of "which bytes did that command change?" that disagree by
//! one would produce a parse tree that disagrees with the document — silently,
//! and only on the edits that straddle a boundary.
//!
//! Every offset is reported against the **pre-edit** document. A
//! [`Command::Compound`]'s parts each address the document the parts before
//! them left, so a compound of several is replayed on a copy to find out what
//! it did; see [`compute_edit_span`].

use std::fmt;

use super::{CursorState, Document, Position};
use crate::history::Command;

/// The byte range a command replaced, against the pre-edit document.
///
/// `start_byte` and `old_end_byte` are offsets into the document as it was;
/// `new_end_byte` is an offset into the document as it is now. The old-end
/// *point* has to be captured here rather than derived later, because the
/// document it refers to no longer exists once the command has been applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditSpan {
    /// Byte offset where the edit begins — identical in both documents, since
    /// everything before it is untouched.
    pub start_byte: usize,
    /// Byte offset where the replaced text ended in the pre-edit document.
    pub old_end_byte: usize,
    /// Byte offset where the new text ends in the post-edit document.
    pub new_end_byte: usize,
    /// Row of the old end position, in the pre-edit document.
    pub old_end_row: usize,
    /// Byte column of the old end position within its row, in the pre-edit
    /// document.
    pub old_end_column: usize,
}

/// Why a command's span could not be computed.
///
/// There is one cause and it is never routine: a command carrying a position
/// the document does not contain, which means the command and the document have
/// already diverged. A caller must respond by reparsing wholesale, never by
/// guessing a span — a wrong span produces a tree that disagrees with the
/// document in a way nothing downstream can detect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditSpanError;

impl fmt::Display for EditSpanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("command position does not resolve against this document")
    }
}

impl std::error::Error for EditSpanError {}

/// Converts a byte offset into a tree-sitter point `(row, byte_column)`.
///
/// The column is in **bytes**, not characters: that is what tree-sitter's
/// `Point` means, and using character columns here would misplace every edit on
/// a line containing anything outside ASCII.
///
/// Returns `None` when the offset lies outside the document.
#[must_use]
pub fn byte_point(document: &Document, offset: usize) -> Option<(usize, usize)> {
    let position = document.offset_to_position(offset)?;
    let line_start = document.line_to_byte_offset(position.line)?;
    Some((position.line, offset.checked_sub(line_start)?))
}

/// Computes the byte span a command affects, against the pre-edit document.
///
/// A compound's parts are applied one after another, each against the
/// document its predecessors left, so each part's position is in *that*
/// document's coordinates. The keyboard's multi-cursor edits happen to be
/// emitted last-caret-first, which makes every position valid in the original
/// document too; nothing else promises that. Undo groups and the inverse of a
/// multi-cursor edit — which reverses the order — address text that only
/// exists partway through. So a compound with more than one content edit is
/// replayed, part by part, on a copy of the document (a rope clone, so the
/// copy shares the text rather than duplicating it), and the parts' spans are
/// composed into one. Taking the extremes of the parts' coordinates instead,
/// as this once did, describes an edit that did not happen whenever a later
/// part lands before an earlier one's end — and a parse handed that edit
/// reuses nodes for text that is no longer there.
///
/// The combined span is still one contiguous range, so a compound whose parts
/// are far apart — a multi-cursor edit at the top and bottom of a file —
/// reports everything between them. Reporting two disjoint spans as one range
/// is the only alternative that stays correct, and a reparse of too much is a
/// cost rather than a bug.
///
/// Returns `Ok(None)` when the command contains no content edit at all, which
/// a pure [`Command::SetSelection`] does.
///
/// # Errors
///
/// [`EditSpanError`] when a position does not resolve against the document it
/// applies to, or a part cannot be replayed. The caller must degrade to a full
/// reparse rather than report a span it guessed.
pub fn compute_edit_span(
    document: &Document,
    command: &Command,
) -> Result<Option<EditSpan>, EditSpanError> {
    let mut parts = Vec::new();
    content_parts(command, &mut parts);

    let bytes = match parts.as_slice() {
        [] => return Ok(None),
        [only] => part_bytes(document, only)?,
        [first, rest @ ..] => {
            let mut replayed = document.clone();
            let mut cursor = CursorState::at(Position::zero());
            let mut combined = part_bytes(&replayed, first)?;
            first
                .apply(&mut replayed, &mut cursor)
                .map_err(|_| EditSpanError)?;
            for part in rest {
                let next = part_bytes(&replayed, part)?;
                part.apply(&mut replayed, &mut cursor)
                    .map_err(|_| EditSpanError)?;
                combined = combined.then(&next)?;
            }
            combined
        },
    };

    let (old_end_row, old_end_column) = byte_point(document, bytes.old_end).ok_or(EditSpanError)?;
    Ok(Some(EditSpan {
        start_byte: bytes.start,
        old_end_byte: bytes.old_end,
        new_end_byte: bytes.new_end,
        old_end_row,
        old_end_column,
    }))
}

/// Collects the content edits in `command`, in the order they apply.
fn content_parts<'command>(command: &'command Command, out: &mut Vec<&'command Command>) {
    match command {
        Command::Insert { .. } | Command::Delete { .. } | Command::Replace { .. } => {
            out.push(command);
        },
        Command::SetSelection { .. } => {},
        Command::Compound { commands } => {
            for inner in commands {
                content_parts(inner, out);
            }
        },
    }
}

/// One edit as byte offsets: what it replaced in the document before it, and
/// where its new text ends in the document after.
#[derive(Debug, Clone, Copy)]
struct Bytes {
    /// Where the edit begins, the same in both documents.
    start: usize,
    /// Where the replaced text ended, before the edit.
    old_end: usize,
    /// Where the new text ends, after it.
    new_end: usize,
}

impl Bytes {
    /// The single edit equal to this one followed by `next`, which is in the
    /// coordinates this one left.
    ///
    /// Everything before the earlier of the two starts is untouched by both.
    /// The combined new text ends wherever the later of the two ends lands
    /// once `next` has moved this one's end; everything past that is past
    /// both edits, so it is the old text past the old end, moved by both
    /// deltas — which is what fixes the old end.
    fn then(self, next: &Self) -> Result<Self, EditSpanError> {
        let start = self.start.min(next.start);
        let carried = if self.new_end <= next.start {
            self.new_end
        } else if self.new_end >= next.old_end {
            (self.new_end - next.old_end)
                .checked_add(next.new_end)
                .ok_or(EditSpanError)?
        } else {
            next.new_end
        };
        let new_end = next.new_end.max(carried);
        let delta = signed(self.new_end)? - signed(self.old_end)? + signed(next.new_end)?
            - signed(next.old_end)?;
        let old_end = usize::try_from(signed(new_end)? - delta).map_err(|_| EditSpanError)?;
        if old_end < start {
            return Err(EditSpanError);
        }
        Ok(Self {
            start,
            old_end,
            new_end,
        })
    }
}

/// `value` as an `i64`, for byte deltas.
fn signed(value: usize) -> Result<i64, EditSpanError> {
    i64::try_from(value).map_err(|_| EditSpanError)
}

/// The bytes one content edit replaces in `document`, the document it is
/// about to be applied to.
fn part_bytes(document: &Document, part: &Command) -> Result<Bytes, EditSpanError> {
    let (position, removed, inserted) = match part {
        Command::Insert { position, text } => (*position, 0, text.len()),
        Command::Delete {
            range,
            deleted_text,
        } => (range.start, deleted_text.len(), 0),
        Command::Replace {
            range,
            old_text,
            new_text,
        } => (range.start, old_text.len(), new_text.len()),
        Command::SetSelection { .. } | Command::Compound { .. } => return Err(EditSpanError),
    };
    let start = document.position_to_offset(position).ok_or(EditSpanError)?;
    Ok(Bytes {
        start,
        old_end: start.checked_add(removed).ok_or(EditSpanError)?,
        new_end: start.checked_add(inserted).ok_or(EditSpanError)?,
    })
}

#[cfg(test)]
mod tests;
