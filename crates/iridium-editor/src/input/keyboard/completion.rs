//! Bounded buffer-word completion, shared by every host and replayed as one edit.
use std::collections::BTreeMap;

use crate::{Command, CursorState, Document, Position, Range};
use super::editing::{self, CaretPlacement, CursorEdit};

/// A completion cannot overwrite edits or cursors changed since it was offered.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CompletionError {
    /// The caret is not after a word, or a selection is active.
    #[error("Completion needs collapsed cursors after the same word prefix")]
    InvalidPrefix,
    /// Buffer words are deliberately read from a bounded neighbourhood.
    #[error("No matching buffer words nearby")]
    NoCandidates,
    /// A stale popup must never edit the new document state.
    #[error("Completion cancelled because the document or cursors changed")]
    Stale,
    /// A host supplied an invalid menu index.
    #[error("Completion candidate index is outside the menu")]
    InvalidCandidate,
}

/// Immutable candidates and the exact editing state they were offered for.
#[derive(Debug)]
pub struct CompletionSession {
    document: u64,
    revision: u64,
    cursors: CursorState,
    prefix: String,
    candidates: Vec<String>,
}

impl CompletionSession {
    /// Collects matching words from at most 64 KiB around the primary caret.
    ///
    /// # Errors
    /// Rejects selections, different prefixes, and an empty candidate set.
    pub fn new(document: &Document, cursors: &CursorState) -> Result<Self, CompletionError> {
        let primary = cursors.primary;
        let prefix = prefix_at(document, primary.head)?;
        if prefix.is_empty() || cursors.all_selections().any(|selection| !selection.is_collapsed()) {
            return Err(CompletionError::InvalidPrefix);
        }
        for selection in cursors.all_selections() {
            if prefix_at(document, selection.head)? != prefix { return Err(CompletionError::InvalidPrefix); }
        }
        let caret = document.position_to_offset(primary.head).ok_or(CompletionError::InvalidPrefix)?;
        let mut start = caret.saturating_sub(32_768);
        let mut end = caret.saturating_add(32_768).min(document.byte_count());
        while start < end && !document.rope().is_char_boundary(start) { start += 1; }
        while end > start && !document.rope().is_char_boundary(end) { end -= 1; }
        let text = document.rope().slice(start..end).to_string();
        let mut words = BTreeMap::new();
        let words_in_window: Vec<_> = text.split(|c: char| !is_word(c)).collect();
        for (index, word) in words_in_window.iter().copied().enumerate() {
            if (index == 0 && start > 0) || (index + 1 == words_in_window.len() && end < document.byte_count()) { continue; }
            if word.len() > prefix.len() && word.len() <= 1024 && word.starts_with(&prefix) {
                *words.entry(word.to_owned()).or_insert(0_usize) += 1;
            }
        }
        let mut ranked: Vec<_> = words.into_iter().collect();
        ranked.sort_by(|(left, a), (right, b)| b.cmp(a).then_with(|| left.cmp(right)));
        let candidates: Vec<_> = ranked.into_iter().take(64).map(|(word, _)| word).collect();
        if candidates.is_empty() { return Err(CompletionError::NoCandidates); }
        Ok(Self { document: document.id(), revision: document.revision(), cursors: cursors.clone(), prefix, candidates })
    }

    /// The ordered menu, highest-frequency words first.
    #[must_use]
    pub fn candidates(&self) -> &[String] { &self.candidates }

    /// Builds one reversible replacement at every cursor without changing state.
    ///
    /// # Errors
    /// Rejects stale state, invalid indices, or invalid document coordinates.
    pub fn accept(&self, index: usize, document: &Document, cursors: &CursorState) -> Result<Command, CompletionError> {
        if document.id() != self.document || document.revision() != self.revision || *cursors != self.cursors {
            return Err(CompletionError::Stale);
        }
        let word = self.candidates.get(index).ok_or(CompletionError::InvalidCandidate)?;
        let mut edits = Vec::with_capacity(cursors.cursor_count());
        for selection in cursors.all_selections() {
            let end = document.position_to_offset(selection.head).ok_or(CompletionError::InvalidPrefix)?;
            let start = document.offset_to_position(end - self.prefix.len()).ok_or(CompletionError::InvalidPrefix)?;
            edits.push((CursorEdit::replace(Range::new(start, selection.head), word.clone()), CaretPlacement::collapsed(word.len())));
        }
        editing::build_multi_cursor_command_placed(document, cursors, edits).ok_or(CompletionError::InvalidCandidate)
    }
}

fn is_word(character: char) -> bool { character.is_alphanumeric() || character == '_' }

fn prefix_at(document: &Document, position: Position) -> Result<String, CompletionError> {
    let end = document.position_to_offset(position).ok_or(CompletionError::InvalidPrefix)?;
    let mut start = end.saturating_sub(1024);
    while start < end && !document.rope().is_char_boundary(start) { start += 1; }
    let text = document.rope().slice(start..end).to_string();
    let boundary = text.char_indices().rev().find(|(_, c)| !is_word(*c)).map_or(0, |(byte, c)| byte + c.len_utf8());
    if boundary == 0 && start != 0 { return Err(CompletionError::InvalidPrefix); }
    Ok(text[boundary..].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_candidates_are_atomic_and_stale_cursors_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let mut document = Document::new("café caf");
        let mut cursors = CursorState::at(Position::new(0, 8));
        let session = CompletionSession::new(&document, &cursors)?;
        assert_eq!(session.candidates(), &["café"]);
        let command = session.accept(0, &document, &cursors)?;
        command.apply(&mut document, &mut cursors)?;
        assert_eq!(document.text(), "café café");
        assert!(matches!(session.accept(0, &document, &cursors), Err(CompletionError::Stale)));
        command.inverse().apply(&mut document, &mut cursors)?;
        assert_eq!(document.text(), "café caf");
        Ok(())
    }

    #[test]
    fn different_prefixes_are_not_replaced_by_the_primary_word() -> Result<(), Box<dyn std::error::Error>> {
        let document = Document::new("alphabet al z");
        let mut cursors = CursorState::at(Position::new(0, 11));
        cursors.add_cursor(crate::Selection::collapsed(Position::new(0, 13)));
        assert!(matches!(CompletionSession::new(&document, &cursors), Err(CompletionError::InvalidPrefix)));
        Ok(())
    }
}
