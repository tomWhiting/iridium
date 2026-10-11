//! Pure byte-span edits over immutable stores; no file data is copied here.
use std::io;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Original,
    Added,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub store: Store,
    pub offset: u64,
    pub len: u64,
}

pub fn length(pieces: &[Piece]) -> io::Result<u64> {
    pieces.iter().try_fold(0_u64, |sum, piece| {
        sum.checked_add(piece.len)
            .ok_or_else(|| io::Error::other("document size exceeds u64"))
    })
}

pub fn splice(
    pieces: &[Piece],
    range: Range<u64>,
    insertion: &[Piece],
) -> io::Result<(Vec<Piece>, Vec<Piece>)> {
    if range.start > range.end || range.end > length(pieces)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "edit range is outside the document",
        ));
    }
    let inserted_length = length(insertion)?;
    length(pieces)?
        .checked_sub(range.end - range.start)
        .and_then(|remaining| remaining.checked_add(inserted_length))
        .ok_or_else(|| io::Error::other("document size exceeds u64"))?;
    let mut before = Vec::new();
    let mut removed = Vec::new();
    let mut after = Vec::new();
    let mut cursor = 0;
    for &piece in pieces {
        let end = cursor + piece.len;
        segment(&mut before, piece, cursor, cursor..end.min(range.start));
        segment(
            &mut removed,
            piece,
            cursor,
            cursor.max(range.start)..end.min(range.end),
        );
        segment(&mut after, piece, cursor, cursor.max(range.end)..end);
        cursor = end;
    }
    for &piece in insertion {
        push(&mut before, piece);
    }
    for piece in after {
        push(&mut before, piece);
    }
    length(&before)?;
    Ok((before, removed))
}
fn segment(out: &mut Vec<Piece>, piece: Piece, start: u64, range: Range<u64>) {
    if range.start < range.end {
        push(
            out,
            Piece {
                offset: piece.offset + (range.start - start),
                len: range.end - range.start,
                ..piece
            },
        );
    }
}
fn push(pieces: &mut Vec<Piece>, piece: Piece) {
    if piece.len == 0 {
        return;
    }
    if let Some(last) = pieces.last_mut()
        && last.store == piece.store
        && last.offset.checked_add(last.len) == Some(piece.offset)
    {
        last.len += piece.len;
        return;
    }
    pieces.push(piece);
}
