//! Extended grapheme boundaries over rope chunks, keeping scalar-based positions.
use ropey::Rope;
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete, UnicodeSegmentation};

use crate::document::{Document, Position};

#[derive(Clone, Copy)]
pub(super) enum Boundary {
    Previous,
    Next,
    Floor,
    Ceiling,
}

/// Step or snap at a Unicode grapheme boundary without copying the document.
pub(super) fn move_to(document: &Document, position: Position, direction: Boundary) -> Position {
    let Some(offset) = document.position_to_offset(position) else {
        tracing::warn!(?position, "grapheme motion received an invalid position");
        return document.clamp_position(position);
    };
    let target = match boundary(document.rope(), offset, direction) {
        Ok(target) => target,
        Err(error) => {
            // A chunk adapter defect must neither split a character nor become a
            // silent no-op. Use the contiguous oracle and report the broken invariant.
            tracing::error!(
                error,
                offset,
                "grapheme chunk traversal failed; using contiguous text"
            );
            contiguous_boundary(&document.text(), offset, direction)
        },
    };
    // Both offsets are valid boundaries. Count only the traversed slice on the
    // common same-line path, avoiding a scan from the start of a very long line.
    let start = offset.min(target);
    let end = offset.max(target);
    let slice = document.rope().slice(start..end);
    if slice.bytes().any(|byte| byte == b'\r' || byte == b'\n') {
        return document.offset_to_position(target).unwrap_or(position);
    }
    let distance = slice.len_chars();
    let column = if target < offset {
        position.column.saturating_sub(distance)
    } else {
        position.column.saturating_add(distance)
    };
    Position::new(position.line, column)
}

fn boundary(rope: &Rope, offset: usize, direction: Boundary) -> Result<usize, &'static str> {
    let mut cursor = GraphemeCursor::new(offset, rope.len(), true);
    if matches!(direction, Boundary::Floor | Boundary::Ceiling)
        && query(rope, &mut cursor, |c, text, start| {
            c.is_boundary(text, start)
        })?
    {
        return Ok(offset);
    }
    let result = query(rope, &mut cursor, |c, text, start| match direction {
        Boundary::Next | Boundary::Ceiling => c.next_boundary(text, start),
        Boundary::Previous | Boundary::Floor => c.prev_boundary(text, start),
    })?;
    Ok(result.unwrap_or(offset))
}

fn query<T>(
    rope: &Rope,
    cursor: &mut GraphemeCursor,
    mut operation: impl FnMut(&mut GraphemeCursor, &str, usize) -> Result<T, GraphemeIncomplete>,
) -> Result<T, &'static str> {
    let (mut chunk, mut start) = rope
        .get_chunk(cursor.cur_cursor())
        .ok_or("invalid initial chunk")?;
    loop {
        match operation(cursor, chunk, start) {
            Ok(value) => return Ok(value),
            Err(GraphemeIncomplete::PreContext(end)) => {
                let index = end.checked_sub(1).ok_or("context before document start")?;
                let (context, context_start) =
                    rope.get_chunk(index).ok_or("missing context chunk")?;
                let text = context
                    .get(..end - context_start)
                    .ok_or("invalid context boundary")?;
                cursor.provide_context(text, context_start);
            },
            Err(GraphemeIncomplete::PrevChunk) => {
                let index = start.checked_sub(1).ok_or("chunk before document start")?;
                (chunk, start) = rope.get_chunk(index).ok_or("missing previous chunk")?;
            },
            Err(GraphemeIncomplete::NextChunk) => {
                let index = start + chunk.len();
                if index >= rope.len() {
                    return Err("chunk beyond document end");
                }
                (chunk, start) = rope.get_chunk(index).ok_or("missing next chunk")?;
            },
            Err(GraphemeIncomplete::InvalidOffset) => return Err("cursor outside supplied chunk"),
        }
    }
}

fn contiguous_boundary(text: &str, offset: usize, direction: Boundary) -> usize {
    let mut boundaries = text
        .grapheme_indices(true)
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()));
    match direction {
        Boundary::Next => boundaries
            .into_iter()
            .find(|&index| index > offset)
            .unwrap_or(offset),
        Boundary::Previous => boundaries
            .take_while(|&index| index < offset)
            .last()
            .unwrap_or(0),
        Boundary::Floor => boundaries
            .take_while(|&index| index <= offset)
            .last()
            .unwrap_or(0),
        Boundary::Ceiling => boundaries.find(|&index| index >= offset).unwrap_or(offset),
    }
}

#[cfg(test)]
mod tests;
