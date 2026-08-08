//! Backspace's half of the auto-pair rules: what one Backspace takes back.
//!
//! Split from [`super::behaviors`], which keeps the insertion half. The two
//! stopped being one subject at ruling B-13 (`docs/design/AUTO-PAIR-MAP.md`
//! §10.12): insertion reads the manifest and the parse tree, while this side
//! reads [`AutoPairInsertion`] and asks neither. `behaviors.rs` also stands well
//! over this codebase's module cap, and this was the piece of it that had its
//! own subject.
//!
//! ⭐ The single-character collapse comes with it and keeps its buffer-based
//! rule, because the two rules living side by side is what makes the asymmetry
//! between them visible to a reader rather than scattered across two files.

use crate::document::{CursorState, Document, Position, Range};

use super::auto_pair_record::AutoPairInsertion;
use super::behaviors::{PairRules, char_at, char_before};
use super::editing::CursorEdit;
use super::{motions, multi_char_pairs};

/// Builds one backspace edit per cursor, deleting a closer this editor wrote or
/// both halves of an empty single-character pair.
///
/// Cursors with a selection delete the selection. A collapsed cursor takes the
/// multi-character closer the editor wrote there, when `record` still describes
/// this exact document and cursor state — `r#"|"#` leaves `r#`, `/*| */` leaves
/// `/`, via [`multi_char_pairs::written_closer_around`] — otherwise the empty
/// single-character pair around it (`(|)`, `"|"`, …), and failing both the
/// single character before the caret like [`super::editing::backspace_edits`].
///
/// ⭐⭐ **The two collapses answer to different evidence, deliberately.** The
/// multi-character one fires only where [`super::AutoPairInsertion`] remembers
/// writing that closer at that caret; the single-character one keeps its
/// buffer-based rule and collapses any `(|)` it finds, whoever put it there. The
/// asymmetry is sized by the harm — over-deleting one character is a nuisance
/// the user fixes by retyping it, and over-deleting four is data loss, since at
/// `/* keep/*|` ` */` the four-character answer takes the enclosing comment's
/// closer and comments out the rest of the file. Only the destructive rule has
/// to be certain. Ruling B-13, §10.12.
pub(super) fn backspace_edits_with_pairs(
    document: &Document,
    cursor: &CursorState,
    record: Option<&AutoPairInsertion>,
) -> Vec<CursorEdit> {
    // The same set the insertion used, so a pair this language never inserts
    // is never collapsed by one backspace either: in Rust `'a'` is a character
    // literal the user typed, and eating both quotes would delete input.
    let pairs = PairRules::for_document(document);
    // Validated once for the whole keystroke: one keystroke wrote one command,
    // so the record's claim is about the edit, not about a caret in it.
    let record = record.filter(|written| written.describes(document, cursor));
    cursor
        .all_selections()
        .map(|sel| {
            if !sel.is_collapsed() {
                return CursorEdit::delete(sel.range());
            }
            // The closer this editor wrote here goes back with the one
            // character that triggered it: `r#"|"#` leaves `r#` (B-8).
            //
            // ⚠️ Asked before the single-character rule below and not after:
            // that one sees a quote on each side of `r#"|"#` and would match
            // too, taking the quotes alone and leaving a mangled `r##`.
            let multi = record
                .and_then(|written| written.closer_at(sel.head))
                .and_then(|closer| {
                    multi_char_pairs::written_closer_around(document, sel.head, closer)
                });
            if let Some(range) = multi {
                return CursorEdit::delete(range);
            }
            let between_pair = char_before(document, sel.head)
                .and_then(|before| pairs.close(before))
                .is_some_and(|close| char_at(document, sel.head) == Some(close));
            if between_pair {
                CursorEdit::delete(Range::new(
                    Position::new(sel.head.line, sel.head.column - 1),
                    Position::new(sel.head.line, sel.head.column + 1),
                ))
            } else {
                CursorEdit::delete(Range::new(motions::char_left(document, sel.head), sel.head))
            }
        })
        .collect()
}
