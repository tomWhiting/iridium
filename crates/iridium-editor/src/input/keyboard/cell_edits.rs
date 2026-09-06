//! Local grapheme boundaries and command preflight, using persistent rope shares.

use unicode_segmentation::UnicodeSegmentation;

use crate::cell_layout::CellLayoutError;
use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::editor::CellInputError;
use crate::history::Command;

use super::editing::CursorEdit;
use super::{ClipboardOperation, KeyResult, motions};

/// Boundary pair enclosing a scalar position. Reads only that logical line.
fn boundaries(
    document: &Document,
    position: Position,
) -> Result<(Position, Position), CellLayoutError> {
    let text = document
        .line(position.line)
        .ok_or(CellLayoutError::InvalidPosition {
            line: position.line,
            column: position.column,
        })?;
    let mut start = 0;
    for grapheme in text.graphemes(true) {
        let end = start + grapheme.chars().count();
        if position.column == start {
            return Ok((position, position));
        }
        if position.column < end {
            return Ok((
                Position::new(position.line, start),
                Position::new(position.line, end),
            ));
        }
        start = end;
    }
    if position.column == start {
        Ok((position, position))
    } else {
        Err(CellLayoutError::InvalidPosition {
            line: position.line,
            column: position.column,
        })
    }
}

pub(super) fn boundary(
    document: &Document,
    position: Position,
    forward: bool,
) -> Result<Position, CellLayoutError> {
    let (start, end) = boundaries(document, position)?;
    Ok(if forward { end } else { start })
}

fn validate_position(document: &Document, position: Position) -> Result<(), CellLayoutError> {
    let (start, end) = boundaries(document, position)?;
    if start != end {
        return Err(CellLayoutError::NonGraphemeBoundary {
            line: position.line,
            column: position.column,
        });
    }
    Ok(())
}

pub fn validate_cursor(document: &Document, cursor: &CursorState) -> Result<(), CellLayoutError> {
    for selection in cursor.all_selections() {
        validate_position(document, selection.anchor)?;
        validate_position(document, selection.head)?;
    }
    Ok(())
}

pub(super) fn expand_edits(
    document: &Document,
    edits: &mut [CursorEdit],
) -> Result<(), CellLayoutError> {
    for edit in edits {
        edit.range = Range::new(
            boundary(document, edit.range.start, false)?,
            boundary(document, edit.range.end, true)?,
        );
    }
    Ok(())
}

/// A scalar motion supplies word/line policy; rounding makes its edge atomic.
pub(super) fn step(
    document: &Document,
    position: Position,
    forward: bool,
    word: bool,
) -> Result<Position, CellLayoutError> {
    let next = match (forward, word) {
        (true, true) => motions::word_right(document, position),
        (false, true) => motions::word_left(document, position),
        (true, false) => motions::char_right(document, position),
        (false, false) => motions::char_left(document, position),
    };
    boundary(document, next, forward)
}

fn normalized_selection(
    document: &Document,
    selection: Selection,
) -> Result<Selection, CellLayoutError> {
    if selection.is_collapsed() {
        return Ok(Selection::collapsed(boundary(
            document,
            selection.head,
            true,
        )?));
    }
    let start = boundary(document, selection.start(), false)?;
    let end = boundary(document, selection.end(), true)?;
    Ok(if selection.is_forward() {
        Selection::new(start, end)
    } else {
        Selection::new(end, start)
    })
}

fn normalize_cursor(document: &Document, cursor: &mut CursorState) -> Result<(), CellLayoutError> {
    let mut normalized = CursorState::new(normalized_selection(document, cursor.primary)?);
    for selection in &cursor.secondary {
        normalized.add_cursor(normalized_selection(document, *selection)?);
    }
    *cursor = normalized;
    Ok(())
}

/// Validates every edit before applying the result to the live document. The
/// clone shares rope nodes; it never materializes, compares or hashes full text.
/// Post-insert grapheme joining adjusts the recorded selection in this command,
/// so undo and redo restore exact bytes and the corresponding selection together.
pub fn prepare_result(
    mut result: KeyResult,
    document: &Document,
    cursor: &CursorState,
) -> Result<KeyResult, CellInputError> {
    match &mut result {
        KeyResult::Command(command)
        | KeyResult::Clipboard(ClipboardOperation::Cut { command, .. }) => {
            if let Command::SetSelection { new_state, .. } = command {
                normalize_cursor(document, new_state)?;
            } else {
                let mut scratch = document.clone();
                let mut scratch_cursor = cursor.clone();
                preflight(command, &mut scratch, &mut scratch_cursor)?;
            }
        },
        KeyResult::Clipboard(_)
        | KeyResult::Search(_)
        | KeyResult::History(_)
        | KeyResult::Ast(_)
        | KeyResult::HostCommand { .. }
        | KeyResult::Handled
        | KeyResult::Ignored => {},
    }
    Ok(result)
}

fn preflight(
    command: &mut Command,
    document: &mut Document,
    cursor: &mut CursorState,
) -> Result<(), CellInputError> {
    match command {
        Command::Compound { commands } => {
            for child in commands {
                preflight(child, document, cursor)?;
            }
            return Ok(());
        },
        Command::SetSelection { new_state, .. } => normalize_cursor(document, new_state)?,
        Command::Delete { range, .. } | Command::Replace { range, .. } => {
            validate_position(document, range.start)?;
            validate_position(document, range.end)?;
        },
        // A replacement's delete may temporarily join the two adjacent
        // graphemes. The following insertion uses that original scalar edge;
        // it must not be rounded into somebody else's text mid-command.
        Command::Insert { position, .. } => {
            boundaries(document, *position)?;
        },
    }
    command.apply(document, cursor)?;
    Ok(())
}

/// Private prepared state, consumed immediately by the editor's synchronous call.
pub struct PreparedCellEdit {
    pub command: Command,
    pub document: Document,
    pub cursor: CursorState,
    pub span: Option<crate::document::EditSpan>,
}

impl PreparedCellEdit {
    /// Attach the exact final selection to the same reversible history command.
    pub fn select(mut self, new_state: CursorState) -> Result<Self, CellInputError> {
        validate_cursor(&self.document, &new_state)?;
        let selection = Command::SetSelection {
            old_state: self.cursor.clone(),
            new_state: new_state.clone(),
        };
        self.cursor = new_state;
        if self.command.modifies_content() || !selection.is_empty() {
            if self.command.is_empty() {
                self.command = selection;
            } else {
                self.command = Command::Compound {
                    commands: vec![self.command, selection],
                };
            }
        }
        Ok(self)
    }
}

/// Validate a host-supplied range before any clamping or content access.
pub fn validate_range(document: &Document, range: Range) -> Result<(), CellInputError> {
    if range.start > range.end {
        return Err(CellInputError::InvalidRange { range });
    }
    validate_position(document, range.start)?;
    validate_position(document, range.end)?;
    Ok(())
}

/// An inserted end may join a grapheme or sit between newly adjacent CR and LF.
pub fn canonical_end(document: &Document, mut offset: usize) -> Result<Position, CellInputError> {
    if offset
        .checked_sub(1)
        .and_then(|index| document.rope().get_byte(index))
        == Some(b'\r')
        && document.rope().get_byte(offset) == Some(b'\n')
    {
        offset = offset
            .checked_add(1)
            .ok_or(CellInputError::InvalidOffset { offset })?;
    }
    let position = document
        .offset_to_position(offset)
        .ok_or(CellInputError::InvalidOffset { offset })?;
    Ok(boundary(document, position, true)?)
}

/// Share the existing preflight normalizer, but retain its checked scratch state.
pub fn prepare_command(
    mut command: Command,
    document: &Document,
    cursor: &CursorState,
) -> Result<PreparedCellEdit, CellInputError> {
    validate_cursor(document, cursor)?;
    let span = crate::document::compute_edit_span(document, &command)?;
    let mut scratch = document.clone();
    let mut scratch_cursor = cursor.clone();
    prepare_isolated(&mut command, document, &mut scratch, &mut scratch_cursor)?;
    Ok(PreparedCellEdit {
        command,
        document: scratch,
        cursor: scratch_cursor,
        span,
    })
}

fn prepare_isolated(
    command: &mut Command,
    original: &Document,
    document: &mut Document,
    cursor: &mut CursorState,
) -> Result<(), CellInputError> {
    match command {
        Command::Compound { commands } => {
            for child in commands {
                prepare_isolated(child, original, document, cursor)?;
            }
            return Ok(());
        },
        Command::SetSelection { .. } => return preflight(command, document, cursor),
        Command::Insert { position, .. } => {
            validate_position(original, *position)?;
            let offset = original.position_to_offset(*position).ok_or_else(|| {
                CellInputError::InvalidRange {
                    range: Range::empty(*position),
                }
            })?;
            *position = position_at_byte(document, offset)?;
        },
        Command::Delete { range, .. } | Command::Replace { range, .. } => {
            validate_range(original, *range)?;
            let start = original
                .position_to_offset(range.start)
                .ok_or(CellInputError::InvalidRange { range: *range })?;
            let end = original
                .position_to_offset(range.end)
                .ok_or(CellInputError::InvalidRange { range: *range })?;
            *range = Range::new(
                position_at_byte(document, start)?,
                position_at_byte(document, end)?,
            );
        },
    }
    // Commands are prepared in descending original byte order. Earlier edits
    // can join a grapheme/CRLF at a shared boundary; those original byte edges
    // remain authoritative until the complete transaction is normalized.
    preserve_line_endings(command, document)?;
    command.apply(document, cursor)?;
    Ok(())
}

fn position_at_byte(document: &Document, offset: usize) -> Result<Position, CellInputError> {
    let position = document
        .offset_to_position(offset)
        .ok_or(CellInputError::InvalidOffset { offset })?;
    if document.position_to_offset(position) != Some(offset) {
        return Err(CellInputError::InvalidOffset { offset });
    }
    Ok(position)
}

/// Include adjacent CR/LF in the recorded splice: inverse coordinates derived
/// from inserted text must not accidentally consume an existing line ending.
fn preserve_line_endings(command: &mut Command, document: &Document) -> Result<(), CellInputError> {
    let (range, text) = match command {
        Command::Replace {
            range, new_text, ..
        } => (*range, new_text.as_str()),
        Command::Delete { range, .. } => (*range, ""),
        Command::Insert { position, text } => (Range::empty(*position), text.as_str()),
        Command::SetSelection { .. } | Command::Compound { .. } => return Ok(()),
    };
    let start = document
        .position_to_offset(range.start)
        .ok_or(CellInputError::InvalidRange { range })?;
    let end = document
        .position_to_offset(range.end)
        .ok_or(CellInputError::InvalidRange { range })?;
    let prefix = start
        .checked_sub(1)
        .filter(|index| document.rope().get_byte(*index) == Some(b'\r'));
    let suffix = document.rope().get_byte(end) == Some(b'\n');
    if prefix.is_none() && !suffix {
        return Ok(());
    }
    let expanded_start = prefix.unwrap_or(start);
    let expanded_end = if suffix {
        end.checked_add(1)
            .ok_or(CellInputError::InvalidOffset { offset: end })?
    } else {
        end
    };
    let expanded = Range::new(
        document
            .offset_to_position(expanded_start)
            .ok_or(CellInputError::InvalidOffset {
                offset: expanded_start,
            })?,
        document
            .offset_to_position(expanded_end)
            .ok_or(CellInputError::InvalidOffset {
                offset: expanded_end,
            })?,
    );
    let mut new_text = String::new();
    if prefix.is_some() {
        new_text.push('\r');
    }
    new_text.push_str(text);
    if suffix {
        new_text.push('\n');
    }
    *command = Command::Replace {
        range: expanded,
        old_text: document.slice(expanded),
        new_text,
    };
    Ok(())
}

/// Typed cell paste avoids the legacy builder's clamping/Option failure seam.
/// Complete per-selection replacements keep CRLF joins stable between edits.
pub fn prepare_paste(
    text: &str,
    document: &Document,
    cursor: &CursorState,
) -> Result<PreparedCellEdit, CellInputError> {
    validate_cursor(document, cursor)?;
    let mut effective = cursor.clone();
    normalize_cursor(document, &mut effective)?;
    let mut edits: Vec<_> = effective
        .all_selections()
        .enumerate()
        .map(|(index, selection)| (index, selection.range()))
        .collect();
    edits.sort_by_key(|(_, range)| (range.start, range.end));
    let mut removed = 0usize;
    let mut inserted = 0usize;
    let mut offsets = vec![0; effective.cursor_count()];
    let mut commands = Vec::with_capacity(edits.len());
    for (index, range) in edits {
        validate_range(document, range)?;
        let start = document
            .position_to_offset(range.start)
            .ok_or(CellInputError::InvalidRange { range })?;
        let end = document
            .position_to_offset(range.end)
            .ok_or(CellInputError::InvalidRange { range })?;
        let offset = start
            .checked_sub(removed)
            .and_then(|value| value.checked_add(inserted))
            .and_then(|value| value.checked_add(text.len()))
            .ok_or(CellInputError::InvalidOffset { offset: start })?;
        let slot = offsets
            .get_mut(index)
            .ok_or(CellInputError::InvalidOffset { offset: index })?;
        *slot = offset;
        removed = removed
            .checked_add(end - start)
            .ok_or(CellInputError::InvalidOffset { offset: removed })?;
        inserted = inserted
            .checked_add(text.len())
            .ok_or(CellInputError::InvalidOffset { offset: inserted })?;
        let old_text = document.slice(range);
        if old_text != text {
            commands.push(Command::Replace {
                range,
                old_text,
                new_text: text.to_owned(),
            });
        }
    }
    commands.reverse();
    let prepared = prepare_command(Command::Compound { commands }, document, cursor)?;
    let mut positions = offsets
        .into_iter()
        .map(|offset| canonical_end(&prepared.document, offset));
    let primary = positions
        .next()
        .ok_or(CellInputError::InvalidOffset { offset: 0 })??;
    let mut final_cursor = CursorState::at(primary);
    for position in positions {
        final_cursor.add_cursor(Selection::collapsed(position?));
    }
    prepared.select(final_cursor)
}
