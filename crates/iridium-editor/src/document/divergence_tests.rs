#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;

use super::*;
use crate::document::Position;

/// A small deterministic generator, so a failure replays exactly.
struct Steps(u64);

impl Steps {
    fn next(&mut self, below: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % below as u64).unwrap()
    }
}

fn position(document: &Document, offset: usize) -> Position {
    document.offset_to_position(offset).unwrap()
}

/// Replaces `range` of bytes the way the editor does: a delete, then an insert.
fn replace(document: &mut Document, range: std::ops::Range<usize>, text: &str) {
    let start = position(document, range.start);
    let end = position(document, range.end);
    document.delete(crate::Range::new(start, end)).unwrap();
    document.insert(start, text).unwrap();
}

/// A char-boundary byte offset in `text`, at or before `near`.
fn boundary(text: &str, near: usize) -> usize {
    let mut at = near.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn sample_text(lines: usize) -> String {
    let mut text = String::new();
    for index in 0..lines {
        let _ = writeln!(text, "line {index}: café, naïve {}", index % 7);
    }
    text
}

/// ASCII only, so any byte offset is a character boundary.
fn ascii_text(lines: usize) -> String {
    let mut text = String::new();
    for index in 0..lines {
        let _ = writeln!(text, "    let value_{index} = compute({index});");
    }
    text
}

#[test]
fn every_answer_matches_a_full_comparison_through_random_edits() {
    let reference = sample_text(40);
    let mut document = Document::new(&reference);
    let divergence = Divergence::new();
    let mut steps = Steps(0x9e37_79b9_7f4a_7c15);
    for _ in 0..4_000 {
        let text = document.text();
        let start = boundary(&text, steps.next(text.len() + 1));
        let end = boundary(&text, start + steps.next(12)).max(start);
        match steps.next(6) {
            // Type over: same length when the selection is one ASCII byte.
            0 => replace(&mut document, start..end, &"x".repeat(end - start)),
            // Restore the reference's bytes over the same span, when they
            // line up: the edit that can make the buffer clean again.
            1 if end <= reference.len()
                && reference.is_char_boundary(start)
                && reference.is_char_boundary(end) =>
            {
                replace(&mut document, start..end, &reference[start..end]);
            },
            2 => document.insert(position(&document, start), "é").unwrap(),
            3 => {
                document
                    .delete(crate::Range::new(
                        position(&document, start),
                        position(&document, end),
                    ))
                    .unwrap();
            },
            // Move a line down past its neighbour.
            4 => {
                let line = steps.next(document.line_count().saturating_sub(2).max(1));
                let first = document.line(line).unwrap();
                let second = document.line(line + 1).unwrap_or_default();
                let from = document.line_to_byte_offset(line).unwrap();
                let to = from + first.len() + second.len() + 1;
                if to < document.byte_count() {
                    replace(&mut document, from..to, &format!("{second}\n{first}"));
                }
            },
            // Back to the reference outright: clean, by a different route.
            _ => {
                let text = document.text();
                let all = text.len();
                replace(&mut document, 0..all, &reference);
            },
        }
        assert_eq!(
            divergence.differs(&document, &reference, 1),
            document.text() != reference,
            "after revision {}",
            document.revision(),
        );
    }
}

#[test]
fn a_new_reference_or_document_is_compared_afresh() {
    let mut document = Document::new("hello");
    let divergence = Divergence::new();
    assert!(!divergence.differs(&document, "hello", 1));
    // Same stamp would be a caller's lie; a new one must be believed.
    assert!(divergence.differs(&document, "jello", 2));
    replace(&mut document, 0..1, "j");
    assert!(!divergence.differs(&document, "jello", 2));
    let replaced = Document::continuing_from("jello!", &document);
    assert!(divergence.differs(&replaced, "jello", 2));
    assert!(!divergence.differs(&replaced, "jello!", 3));
}

#[test]
fn typing_over_a_selection_reads_the_edit_not_the_file() {
    let reference = ascii_text(50_000);
    let mut document = Document::new(&reference);
    let divergence = Divergence::new();
    assert!(!divergence.differs(&document, &reference, 1));
    let opened = divergence.bytes_read();
    assert_eq!(opened, reference.len() as u64, "a clean open reads it once");

    // Mid-file, one character at a time, each the same length as before.
    let middle = document.line_to_byte_offset(25_000).unwrap();
    for offset in middle..middle + 500 {
        replace(&mut document, offset..offset + 1, "#");
        assert!(divergence.differs(&document, &reference, 1));
    }
    let typed = divergence.bytes_read() - opened;
    assert!(typed <= 1_000, "500 keystrokes read {typed} bytes");

    // Moving a line twice is the identity: clean again, by reading lines.
    let mut moved = Document::new(&reference);
    let divergence = Divergence::new();
    assert!(!divergence.differs(&moved, &reference, 1));
    let read = divergence.bytes_read();
    for _ in 0..2 {
        let line = 30_000;
        let first = moved.line(line).unwrap();
        let second = moved.line(line + 1).unwrap();
        let from = moved.line_to_byte_offset(line).unwrap();
        let to = from + first.len() + second.len() + 1;
        replace(&mut moved, from..to, &format!("{second}\n{first}"));
        let expected = moved.text() != reference;
        assert_eq!(divergence.differs(&moved, &reference, 1), expected);
    }
    assert!(!divergence.differs(&moved, &reference, 1));
    let lines = divergence.bytes_read() - read;
    assert!(lines <= 400, "two line moves read {lines} bytes");
}

#[test]
fn undoing_the_first_difference_keeps_the_last_as_witness() {
    let reference = ascii_text(50_000);
    let mut document = Document::new(&reference);
    let divergence = Divergence::new();
    assert!(!divergence.differs(&document, &reference, 1));
    let near = 100;
    let far = reference.len() - 100;
    replace(&mut document, near..near + 1, "#");
    assert!(divergence.differs(&document, &reference, 1));
    replace(&mut document, far..far + 1, "#");
    assert!(divergence.differs(&document, &reference, 1));
    let read = divergence.bytes_read();
    // Toggling the near edit back and forth never reads across to the far
    // one: the far difference stays witnessed from the end.
    for _ in 0..100 {
        replace(&mut document, near..near + 1, &reference[near..=near]);
        assert!(divergence.differs(&document, &reference, 1));
        replace(&mut document, near..near + 1, "#");
        assert!(divergence.differs(&document, &reference, 1));
    }
    let toggled = divergence.bytes_read() - read;
    assert!(toggled <= 1_000, "200 toggles read {toggled} bytes");
    // And undoing both is clean.
    replace(&mut document, near..near + 1, &reference[near..=near]);
    replace(&mut document, far..far + 1, &reference[far..=far]);
    assert!(!divergence.differs(&document, &reference, 1));
}
