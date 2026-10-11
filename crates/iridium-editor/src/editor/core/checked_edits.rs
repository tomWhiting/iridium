//! Revision-checked host edits share the existing command and undo machinery.
use crate::input::keyboard::editing::{CursorEdit, build_line_edits_command};
use crate::{Editor, Range};

/// One exact replacement expressed in original-document UTF-8 byte offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedEdit {
    /// Original source byte range, never a grapheme or UTF-16 offset.
    pub bytes: std::ops::Range<usize>,
    /// Bytes expected in that range; required for valid undo and stale detection.
    pub expected: String,
    /// New bytes, including any desired line endings.
    pub replacement: String,
}
impl Editor {
    /// Apply preflighted host replacements as one isolated undo unit.
    ///
    /// Existing selections are remapped through edits and restored by undo.
    /// No file is saved. All edits refer to the same input revision; callers
    /// preparing multiple documents must validate every document before commit.
    /// Limits: 10,000 edits and 32 MiB combined expected/replacement bytes.
    /// Returns the resulting revision, including for an empty/no-op edit list.
    ///
    /// # Errors
    /// Stale revision, read-only document, invalid/noncanonical byte boundary,
    /// mismatched expected source, overlapping/duplicate edits, or failed command.
    /// Preflight refusals leave text, history, selections and input mode unchanged.
    pub fn apply_checked_edits(
        &mut self,
        revision: u64,
        edits: &[CheckedEdit],
    ) -> Result<u64, String> {
        let command = self.prepare_checked_edits(revision, edits)?;
        let Some(command) = command else {
            return Ok(self.state.document.revision());
        };
        self.finish_change();
        if !self.apply_sequence_command(command) {
            return Err("checked edit commit failed; no edits applied".into());
        }
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        Ok(self.state.document.revision())
    }
    /// Validate a proposed edit group without changing any live editor state.
    ///
    /// Call again at commit through `apply_checked_edits`; this does not reserve
    /// a revision. Useful for preflighting an operation spanning documents.
    ///
    /// # Errors
    /// Same validation failures as `apply_checked_edits`.
    pub fn validate_checked_edits(
        &self,
        revision: u64,
        edits: &[CheckedEdit],
    ) -> Result<(), String> {
        self.prepare_checked_edits(revision, edits).map(|_| ())
    }
    fn prepare_checked_edits(
        &self,
        revision: u64,
        edits: &[CheckedEdit],
    ) -> Result<Option<crate::Command>, String> {
        if self.state.document.revision() != revision {
            return Err("document changed since replacement preview".into());
        }
        if self.state.read_only {
            return Err("document is read-only".into());
        }
        if edits.len() > 10_000 {
            return Err("checked edit group exceeds 10,000 replacements".into());
        }
        let bytes = edits.iter().try_fold(0usize, |sum, edit| {
            sum.checked_add(edit.expected.len())?
                .checked_add(edit.replacement.len())
        });
        if bytes.is_none_or(|bytes| bytes > 32 * 1024 * 1024) {
            return Err("checked edit group exceeds 32 MiB".into());
        }
        let mut sorted: Vec<_> = edits.iter().collect();
        sorted.sort_by_key(|edit| (edit.bytes.start, edit.bytes.end));
        for pair in sorted.windows(2) {
            if pair[0].bytes.end > pair[1].bytes.start || pair[0].bytes == pair[1].bytes {
                return Err("replacement ranges overlap or duplicate a location".into());
            }
        }
        let document = &self.state.document;
        let mut intents = Vec::with_capacity(edits.len());
        for edit in sorted {
            if edit.bytes.start > edit.bytes.end || edit.bytes.end > document.byte_count() {
                return Err("invalid replacement byte range".into());
            }
            let start = document
                .offset_to_position(edit.bytes.start)
                .ok_or("invalid replacement start")?;
            let end = document
                .offset_to_position(edit.bytes.end)
                .ok_or("invalid replacement end")?;
            if document.position_to_offset(start) != Some(edit.bytes.start)
                || document.position_to_offset(end) != Some(edit.bytes.end)
            {
                return Err("replacement boundary is inside a character or line ending".into());
            }
            let range = Range::new(start, end);
            if document.slice(range) != edit.expected {
                return Err("replacement source no longer matches preview".into());
            }
            if edit.expected != edit.replacement {
                intents.push(CursorEdit::replace(range, edit.replacement.clone()));
            }
        }
        if intents.is_empty() {
            return Ok(None);
        }
        build_line_edits_command(document, &self.state.cursor, intents)
            .map(Some)
            .ok_or_else(|| "could not prepare checked edit command".into())
    }
}
#[cfg(test)]
mod tests;
