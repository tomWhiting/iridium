//! Map byte anchors through each primitive in its actual sequential document.
use super::{Command, Document, Range, normalize};
use crate::{CursorState, IridiumError};

pub(super) fn map_ranges(
    ranges: &[Range<usize>],
    before: &Document,
    commands: &[Command],
) -> Result<Vec<Range<usize>>, IridiumError> {
    let mut document = before.clone();
    let mut ranges = ranges.to_vec();
    for command in commands {
        apply(command, &mut document, &mut ranges)?;
    }
    normalize(&mut ranges);
    Ok(ranges)
}

fn apply(
    command: &Command,
    document: &mut Document,
    ranges: &mut [Range<usize>],
) -> Result<(), IridiumError> {
    if let Command::Compound { commands } = command {
        for command in commands {
            apply(command, document, ranges)?;
        }
        return Ok(());
    }
    let edit = match command {
        Command::Insert { position, text } => Some((*position, *position, text.len())),
        Command::Delete { range, .. } => Some((range.start, range.end, 0)),
        Command::Replace {
            range, new_text, ..
        } => Some((range.start, range.end, new_text.len())),
        Command::SetSelection { .. } | Command::Compound { .. } => None,
    };
    if let Some((start, end, inserted)) = edit {
        let start = document
            .position_to_offset(start)
            .ok_or(IridiumError::InvalidPosition {
                line: start.line,
                column: start.column,
            })?;
        let end = document
            .position_to_offset(end)
            .ok_or(IridiumError::InvalidPosition {
                line: end.line,
                column: end.column,
            })?;
        for range in ranges {
            range.start = anchor(range.start, start, end, inserted, false);
            range.end = anchor(range.end, start, end, inserted, true);
        }
        command.apply(document, &mut CursorState::default())?;
    }
    Ok(())
}

// Start anchors stay before inserted text; end anchors stay after it. This
// keeps replacement text and typing at either selected edge inside the scope.
const fn anchor(offset: usize, start: usize, end: usize, inserted: usize, after: bool) -> usize {
    if offset < start || (offset == start && !after) {
        offset
    } else if offset > end || (offset == end && end > start) {
        offset - (end - start) + inserted
    } else if after {
        start + inserted
    } else {
        start
    }
}
