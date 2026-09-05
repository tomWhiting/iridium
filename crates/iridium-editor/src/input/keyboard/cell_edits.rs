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
