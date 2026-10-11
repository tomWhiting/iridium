//! Word motions classify complete graphemes, retaining scalar-based columns.
use unicode_segmentation::UnicodeSegmentation;

use crate::document::{Document, Position, Selection};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Space,
    Punctuation,
}

struct Unit {
    start: usize,
    end: usize,
    kind: Kind,
}

/// Whether a complete cluster belongs to a code word.
pub fn is_word_cluster(cluster: &str) -> bool {
    cluster.chars().any(super::motions::is_word_char)
}

fn units(text: &str, bytes: bool) -> Vec<Unit> {
    let mut column = 0;
    text.graphemes(true)
        .map(|cluster| {
            let start = column;
            column += if bytes {
                cluster.len()
            } else {
                cluster.chars().count()
            };
            let kind = if is_word_cluster(cluster) {
                Kind::Word
            } else if cluster.chars().all(char::is_whitespace) {
                Kind::Space
            } else {
                Kind::Punctuation
            };
            Unit {
                start,
                end: column,
                kind,
            }
        })
        .collect()
}

/// Finds the start of the preceding word or punctuation run, skipping spaces.
pub fn word_left(document: &Document, position: Position) -> Position {
    if position.column == 0 {
        return if position.line == 0 {
            position
        } else {
            Position::new(
                position.line - 1,
                document.line_len(position.line - 1).unwrap_or(0),
            )
        };
    }
    let text = document.line(position.line).unwrap_or_default();
    let units = units(&text, false);
    Position::new(position.line, left_column(&units, position.column))
}

/// Moves past the current word or punctuation run and its following spaces.
pub fn word_right(document: &Document, position: Position) -> Position {
    let text = document.line(position.line).unwrap_or_default();
    let units = units(&text, false);
    if position.column >= units.last().map_or(0, |unit| unit.end) {
        return if position.line < document.line_count().saturating_sub(1) {
            Position::new(position.line + 1, 0)
        } else {
            position
        };
    }
    Position::new(position.line, right_column(&units, position.column))
}

/// Selects the word containing the position, including its combining marks.
pub fn select_word_at(document: &Document, position: Position) -> Selection {
    select_run(document, position, true)
}

/// Mouse selection also selects complete whitespace and punctuation runs.
pub fn select_run_at(document: &Document, position: Position) -> Selection {
    select_run(document, position, false)
}

fn select_run(document: &Document, position: Position, word_only: bool) -> Selection {
    let text = document.line(position.line).unwrap_or_default();
    let units = units(&text, false);
    let index = units
        .partition_point(|unit| unit.end <= position.column)
        .min(units.len().saturating_sub(1));
    let Some(unit) = units
        .get(index)
        .filter(|unit| !word_only || unit.kind == Kind::Word)
    else {
        return Selection::collapsed(position);
    };
    let mut start = unit.start;
    let mut end = unit.end;
    for unit in units[..index]
        .iter()
        .rev()
        .take_while(|other| other.kind == unit.kind)
    {
        start = unit.start;
    }
    for unit in units[index + 1..]
        .iter()
        .take_while(|other| other.kind == unit.kind)
    {
        end = unit.end;
    }
    Selection::new(
        Position::new(position.line, start),
        Position::new(position.line, end),
    )
}

/// Finds a preceding code-word boundary in a standalone field, in UTF-8 bytes.
/// Spaces (including newlines) are skipped; results are grapheme boundaries.
pub fn word_left_byte(text: &str, byte: usize) -> usize {
    left_column(&units(text, true), byte.min(text.len()))
}

/// Finds the next code-word boundary in a standalone field, in UTF-8 bytes.
/// Moves past the current run and following whitespace, retaining punctuation.
pub fn word_right_byte(text: &str, byte: usize) -> usize {
    right_column(&units(text, true), byte.min(text.len()))
}

fn left_column(units: &[Unit], column: usize) -> usize {
    let mut index = units.partition_point(|unit| unit.start < column);
    while index > 0 && units[index - 1].kind == Kind::Space {
        index -= 1;
    }
    if index > 0 {
        let kind = units[index - 1].kind;
        while index > 0 && units[index - 1].kind == kind {
            index -= 1;
        }
    }
    units.get(index).map_or(0, |unit| unit.start)
}

fn right_column(units: &[Unit], column: usize) -> usize {
    let mut index = units.partition_point(|unit| unit.end <= column);
    let Some(current) = units.get(index) else {
        return units.last().map_or(0, |unit| unit.end);
    };
    let kind = current.kind;
    while index < units.len() && units[index].kind == kind {
        index += 1;
    }
    while index < units.len() && units[index].kind == Kind::Space {
        index += 1;
    }
    units.get(index).map_or_else(
        || units.last().map_or(0, |unit| unit.end),
        |unit| unit.start,
    )
}
