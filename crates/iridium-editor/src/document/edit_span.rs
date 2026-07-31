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
//! Every offset is computed against the **pre-edit** document. Content commands
//! inside a [`Command::Compound`] are emitted in reverse document order with
//! coordinates that are all valid in the original document (see
//! `emit_content_commands` in [`crate::input::keyboard::editing`]), so the whole
//! walk converts against one rope.

use std::fmt;

use super::Document;
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
/// The combined span of a compound is the minimum start and maximum old end
/// over its content commands, with the new end derived from the total byte
/// delta. That is deliberately conservative for a compound whose parts are far
/// apart — a multi-cursor edit at the top and bottom of a file reports the
/// whole file — because reporting two disjoint spans as one contiguous range is
/// the only alternative that stays correct, and a reparse of too much is a cost
/// rather than a bug.
///
/// Returns `Ok(None)` when the command contains no content edit at all, which
/// a pure [`Command::SetSelection`] does.
///
/// # Errors
///
/// [`EditSpanError`] when a position does not resolve against `document`. The
/// caller must degrade to a full reparse rather than report a span it guessed.
pub fn compute_edit_span(
    document: &Document,
    command: &Command,
) -> Result<Option<EditSpan>, EditSpanError> {
    struct Acc {
        start: usize,
        old_end: usize,
        delta: i64,
    }

    fn merge(acc: &mut Option<Acc>, start: usize, old_end: usize, delta: i64) {
        match acc {
            Some(acc) => {
                acc.start = acc.start.min(start);
                acc.old_end = acc.old_end.max(old_end);
                acc.delta += delta;
            },
            None => {
                *acc = Some(Acc {
                    start,
                    old_end,
                    delta,
                });
            },
        }
    }

    fn walk(
        document: &Document,
        command: &Command,
        acc: &mut Option<Acc>,
    ) -> Result<(), EditSpanError> {
        match command {
            Command::Insert { position, text } => {
                let start = document
                    .position_to_offset(*position)
                    .ok_or(EditSpanError)?;
                let inserted = i64::try_from(text.len()).map_err(|_| EditSpanError)?;
                merge(acc, start, start, inserted);
            },
            Command::Delete {
                range,
                deleted_text,
            } => {
                let start = document
                    .position_to_offset(range.start)
                    .ok_or(EditSpanError)?;
                let removed = i64::try_from(deleted_text.len()).map_err(|_| EditSpanError)?;
                merge(acc, start, start + deleted_text.len(), -removed);
            },
            Command::Replace {
                range,
                old_text,
                new_text,
            } => {
                let start = document
                    .position_to_offset(range.start)
                    .ok_or(EditSpanError)?;
                let removed = i64::try_from(old_text.len()).map_err(|_| EditSpanError)?;
                let inserted = i64::try_from(new_text.len()).map_err(|_| EditSpanError)?;
                merge(acc, start, start + old_text.len(), inserted - removed);
            },
            Command::SetSelection { .. } => {},
            Command::Compound { commands } => {
                for inner in commands {
                    walk(document, inner, acc)?;
                }
            },
        }
        Ok(())
    }

    let mut acc: Option<Acc> = None;
    walk(document, command, &mut acc)?;

    let Some(acc) = acc else {
        return Ok(None);
    };

    let new_end = i64::try_from(acc.old_end).map_err(|_| EditSpanError)? + acc.delta;
    let new_end_byte = usize::try_from(new_end)
        .map_err(|_| EditSpanError)?
        .max(acc.start);
    let (old_end_row, old_end_column) = byte_point(document, acc.old_end).ok_or(EditSpanError)?;

    Ok(Some(EditSpan {
        start_byte: acc.start,
        old_end_byte: acc.old_end,
        new_end_byte,
        old_end_row,
        old_end_column,
    }))
}

#[cfg(test)]
mod tests;
