//! Multi-cursor edit construction.
//!
//! This module turns per-cursor edit intents into a single reversible
//! [`Command`], with exact position accounting across cursors.
//!
//! # Algorithm
//!
//! Each cursor contributes one [`CursorEdit`]: a range to remove (empty for a
//! pure insertion) plus text to insert at the range start (empty for a pure
//! deletion). [`build_multi_cursor_command`] then:
//!
//! 1. Orders the edits by document position and clamps overlapping ranges
//!    (e.g. two word-deletions reaching into the same word) so each byte is
//!    deleted exactly once.
//! 2. Emits `Delete`/`Insert` commands from the **last** edit to the
//!    **first**, so every command's coordinates stay valid in the original
//!    document while the compound is applied front to back.
//! 3. Computes each cursor's post-edit position in **byte offsets**: the new
//!    caret offset is the edit's original start offset, plus the cursor's
//!    [`CaretPlacement`] into its inserted text (the text's end by default),
//!    plus the cumulative byte delta of all edits at earlier offsets. This
//!    accounts for same-line column drift, newline insertions shifting later
//!    lines, and selection replacements alike. The offsets are converted back
//!    to line/column positions against a scratch copy of the document with
//!    all edits applied, so the mapping is exact for any mix of multi-byte
//!    characters and line endings.
//! 4. Merges cursors that converge on the same position and appends a
//!    trailing `SetSelection` carrying the full old and new multi-cursor
//!    states, which makes the compound fully undoable (text *and* cursors).
//!
//! [`build_line_edits_command`] is the second entry point, for line-based
//! block edits (indent/outdent) where the edits do not correspond to cursors
//! one-to-one: it applies arbitrary non-overlapping edits and *remaps* every
//! existing cursor and selection endpoint through them.
//!
//! [`build_command_with_offsets`] is the third entry point, for whole-line
//! operations (move, duplicate, delete, and join in [`super::line_ops`])
//! whose caret rules fit neither of the above: the caller supplies every
//! cursor's post-edit anchor and head as byte offsets into the edited
//! document.
//!
//! The per-cursor edit *intents* those entry points consume — what backspace,
//! forward-delete, cut, delete-to-line-start and the rest each want removed
//! and inserted — live in [`intents`], and are re-exported here so callers see
//! one `editing::` surface.

mod intents;

pub use intents::{
    backspace_edits, copy_text, cut_edits, delete_forward_edits, delete_to_line_end_edits,
    delete_to_line_start_edits, replace_all_edits,
};

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::history::Command;

/// How a cursor or selection endpoint sitting exactly at a pure insertion's
/// position relates to the inserted text during endpoint remapping.
///
/// `After` (the default) shifts the endpoint past the inserted text — an
/// endpoint at an indent insertion keeps covering the same characters.
/// `Before` leaves the endpoint in place — an endpoint at a closing comment
/// marker's insertion point stays with its text, inside the comment, rather
/// than jumping past the marker. Only pure insertions consult the bias;
/// deletions and replacements always shift endpoints at or past their end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InsertBias {
    /// Endpoints at the insertion position shift past the inserted text.
    #[default]
    After,
    /// Endpoints at the insertion position stay before the inserted text.
    Before,
}

/// A single cursor's contribution to a multi-cursor edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorEdit {
    /// Range of text to remove (may be empty for a pure insertion).
    pub range: Range,
    /// Text to insert at `range.start` (may be empty for a pure deletion).
    pub text: String,
    /// Endpoint affinity for pure insertions (see [`InsertBias`]).
    pub bias: InsertBias,
}

impl CursorEdit {
    /// Creates an edit that replaces `range` with `text`.
    pub const fn replace(range: Range, text: String) -> Self {
        Self {
            range,
            text,
            bias: InsertBias::After,
        }
    }

    /// Creates an edit that deletes `range` without inserting anything.
    pub const fn delete(range: Range) -> Self {
        Self {
            range,
            text: String::new(),
            bias: InsertBias::After,
        }
    }

    /// Creates a pure insertion that endpoints at the insertion position do
    /// NOT shift past (see [`InsertBias::Before`]).
    pub fn insert_before(at: Position, text: String) -> Self {
        Self {
            range: Range::new(at, at),
            text,
            bias: InsertBias::Before,
        }
    }
}

/// Where a cursor lands relative to its own edit, expressed as byte offsets
/// into the edit's inserted text.
///
/// `0` is the start of the inserted text (the edit range's start for a pure
/// deletion) and `text.len()` is the end of the inserted text. Offsets must
/// lie on character boundaries of the inserted text; they are clamped to the
/// text length. When `anchor != head` the cursor keeps a selection covering
/// that span of the inserted text (auto-pair selection wrapping); when they
/// are equal the cursor collapses there (e.g. between an auto-closed pair, or
/// on the middle line of a bracket-block Enter expansion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaretPlacement {
    /// Byte offset of the selection anchor within the inserted text.
    pub anchor: usize,
    /// Byte offset of the selection head within the inserted text.
    pub head: usize,
}

impl CaretPlacement {
    /// Collapsed caret at `offset` bytes into the inserted text.
    #[must_use]
    pub const fn collapsed(offset: usize) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }

    /// Selection from `anchor` to `head` (bytes into the inserted text).
    #[must_use]
    pub const fn selection(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }
}

/// Builds a single reversible command from one edit per cursor.
///
/// `edits` must contain exactly one entry per cursor, in
/// [`CursorState::all_selections`] order (primary first). Each cursor
/// collapses to the end of its inserted text (or the start of its deleted
/// range for pure deletions), and cursors that converge on the same position
/// are merged.
///
/// Returns `None` when the edits change neither the document nor the cursor
/// state (for example backspace with every cursor at the document start).
pub fn build_multi_cursor_command(
    document: &Document,
    cursor: &CursorState,
    edits: Vec<CursorEdit>,
) -> Option<Command> {
    let placed = edits
        .into_iter()
        .map(|edit| {
            let placement = CaretPlacement::collapsed(edit.text.len());
            (edit, placement)
        })
        .collect();
    build_multi_cursor_command_placed(document, cursor, placed)
}

/// Builds a single reversible command from one edit per cursor, with an
/// explicit [`CaretPlacement`] per cursor.
///
/// Identical to [`build_multi_cursor_command`] except that each cursor lands
/// where its placement says (relative to its own inserted text) instead of
/// collapsing to the end of the insertion. This is what lets auto-pairs put
/// the caret between the two halves, selection wrapping keep the wrapped text
/// selected, and bracket-block Enter land on the middle line.
///
/// Returns `None` when the edits change neither the document nor the cursor
/// state.
pub fn build_multi_cursor_command_placed(
    document: &Document,
    cursor: &CursorState,
    edits: Vec<(CursorEdit, CaretPlacement)>,
) -> Option<Command> {
    // One edit per cursor is required for the caret bookkeeping below;
    // anything else indicates a caller bug, and doing nothing is the safe
    // response.
    if edits.len() != cursor.cursor_count() {
        return None;
    }

    // Pair each edit with whether it belongs to the primary cursor, then
    // order by document position so overlap clamping and offset accounting
    // can run front to back. At equal (start, end), edits that insert
    // nothing (pure cursor moves such as auto-pair skip-over, and no-op
    // deletes) sort BEFORE insertions: a caret targeting position P refers
    // to the document before a same-position insertion pushes text right,
    // so it must not absorb that insertion's byte delta. Without this
    // tie-break the outcome would depend on cursor enumeration order
    // (which cursor happens to be primary).
    let mut sorted: Vec<(CursorEdit, CaretPlacement, bool)> = edits
        .into_iter()
        .enumerate()
        .map(|(index, (edit, placement))| (edit, placement, index == 0))
        .collect();
    sorted.sort_by_key(|(edit, _, _)| (edit.range.start, edit.range.end, !edit.text.is_empty()));

    let mut ordered: Vec<CursorEdit> = Vec::with_capacity(sorted.len());
    let mut placements: Vec<(CaretPlacement, bool)> = Vec::with_capacity(sorted.len());
    for (edit, placement, is_primary) in sorted {
        ordered.push(edit);
        placements.push((placement, is_primary));
    }
    clamp_edit_ranges(document, &mut ordered);

    let offsets = edit_byte_offsets(document, &ordered)?;
    let (commands, scratch) = emit_content_commands(document, &ordered)?;

    // Compute post-edit caret positions front to back: original start offset
    // + placement offset into the inserted text + cumulative byte delta of
    // all earlier edits, then convert against the scratch document (which
    // already contains every edit).
    let mut new_selections: Vec<Selection> = Vec::with_capacity(ordered.len());
    let mut primary_index = 0usize;
    let mut delta: i64 = 0;
    for (index, ((edit, (placement, is_primary)), (start_off, end_off))) in ordered
        .iter()
        .zip(placements.iter())
        .zip(offsets.iter())
        .enumerate()
    {
        let resolve = |offset_in_text: usize| -> Option<Position> {
            let clamped = offset_in_text.min(edit.text.len());
            let offset = i64::try_from(start_off + clamped).ok()? + delta;
            scratch.offset_to_position(usize::try_from(offset).ok()?)
        };
        let anchor = resolve(placement.anchor)?;
        let head = resolve(placement.head)?;

        if *is_primary {
            primary_index = index;
        }
        new_selections.push(Selection::new(anchor, head));

        delta += i64::try_from(edit.text.len()).ok()?;
        delta -= i64::try_from(end_off - start_off).ok()?;
    }

    let new_state = cursor_state_from(&new_selections, primary_index)?;
    finalize_command(cursor, new_state, commands)
}

/// Builds a single reversible command from line-based block edits (indent and
/// outdent), remapping every existing cursor and selection through the edits.
///
/// Unlike [`build_multi_cursor_command`], `edits` need not correspond to
/// cursors one-to-one: indenting a three-line selection contributes three
/// insertions for a single cursor. Every endpoint of every existing selection
/// is remapped through the edits:
///
/// - an endpoint at or after an edit shifts by that edit's byte delta, so
///   selections keep covering the same text;
/// - an endpoint inside a removed span floors at the span's start (outdent
///   never pushes a cursor past the indentation boundary).
///
/// Returns `None` when the edits change neither the document nor the cursor
/// state (for example outdenting lines that have no leading indentation).
pub fn build_line_edits_command(
    document: &Document,
    cursor: &CursorState,
    mut edits: Vec<CursorEdit>,
) -> Option<Command> {
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    clamp_edit_ranges(document, &mut edits);
    let offsets = edit_byte_offsets(document, &edits)?;
    let (commands, scratch) = emit_content_commands(document, &edits)?;

    let mut new_selections: Vec<Selection> = Vec::with_capacity(cursor.cursor_count());
    for sel in cursor.all_selections() {
        let resolve = |position: Position| -> Option<Position> {
            let offset = document.position_to_offset(document.clamp_position(position))?;
            scratch.offset_to_position(remap_offset(offset, &edits, &offsets)?)
        };
        let anchor = resolve(sel.anchor)?;
        let head = resolve(sel.head)?;
        new_selections.push(Selection::new(anchor, head));
    }

    let new_state = cursor_state_from(&new_selections, 0)?;
    finalize_command(cursor, new_state, commands)
}

/// Builds a single reversible command from arbitrary non-overlapping edits
/// plus explicit post-edit cursor offsets.
///
/// This is the entry point for whole-line operations (move, duplicate,
/// delete, and join lines) where neither the per-cursor caret placement of
/// [`build_multi_cursor_command_placed`] nor the endpoint remapping of
/// [`build_line_edits_command`] matches where the cursors must land: carets
/// ride with moved text, land inside duplicated copies, keep their column
/// through a line deletion, or jump to a join point. `new_offsets` supplies
/// each cursor's post-edit anchor and head as byte offsets into the edited
/// document, in [`CursorState::all_selections`] order (primary first);
/// offsets are clamped to the edited document's length. Cursors that
/// converge on the same position are merged.
///
/// Edits with equal positions keep their order in `edits` (the sort is
/// stable), so callers can rely on the same ordering they used to compute
/// the offsets.
///
/// Returns `None` when the edits change neither the document nor the cursor
/// state, or when the inputs are inconsistent (wrong offset count, edits
/// that cannot be applied), in which case doing nothing is the safe
/// response.
pub fn build_command_with_offsets(
    document: &Document,
    cursor: &CursorState,
    mut edits: Vec<CursorEdit>,
    new_offsets: Vec<(usize, usize)>,
) -> Option<Command> {
    if new_offsets.len() != cursor.cursor_count() {
        return None;
    }

    edits.sort_by_key(|edit| (edit.range.start, edit.range.end, !edit.text.is_empty()));
    clamp_edit_ranges(document, &mut edits);
    let (commands, scratch) = emit_content_commands(document, &edits)?;

    let mut new_selections: Vec<Selection> = Vec::with_capacity(new_offsets.len());
    for (anchor_offset, head_offset) in new_offsets {
        let anchor = scratch.offset_to_position(anchor_offset.min(scratch.byte_count()))?;
        let head = scratch.offset_to_position(head_offset.min(scratch.byte_count()))?;
        new_selections.push(Selection::new(anchor, head));
    }

    let new_state = cursor_state_from(&new_selections, 0)?;
    finalize_command(cursor, new_state, commands)
}

/// Clamps sorted edit ranges to the document and to each other so no byte is
/// deleted twice (overlaps happen when e.g. two cursors word-delete into the
/// same word). A fully-consumed range degenerates to an empty range at the
/// previous edit's end, which naturally merges the cursors afterwards.
fn clamp_edit_ranges(document: &Document, edits: &mut [CursorEdit]) {
    let mut prev_end: Option<Position> = None;
    for edit in edits {
        let mut start = document.clamp_position(edit.range.start);
        let end = document.clamp_position(edit.range.end).max(start);
        if let Some(prev) = prev_end {
            start = start.max(prev);
        }
        edit.range = Range::new(start, end.max(start));
        prev_end = Some(edit.range.end);
    }
}

/// Byte offsets of every edit range in the original document.
///
/// Clamped positions always convert; a failure means inconsistent state, in
/// which case the only safe command is none at all.
fn edit_byte_offsets(document: &Document, edits: &[CursorEdit]) -> Option<Vec<(usize, usize)>> {
    let mut offsets: Vec<(usize, usize)> = Vec::with_capacity(edits.len());
    for edit in edits {
        let start = document.position_to_offset(edit.range.start)?;
        let end = document.position_to_offset(edit.range.end)?;
        offsets.push((start, end));
    }
    Some(offsets)
}

/// Emits content commands from last edit to first so each command's
/// coordinates remain valid in the original document, and applies the same
/// edits to a scratch copy of the document for caret conversion.
fn emit_content_commands(
    document: &Document,
    edits: &[CursorEdit],
) -> Option<(Vec<Command>, Document)> {
    let mut commands: Vec<Command> = Vec::new();
    let mut scratch = document.clone();
    for edit in edits.iter().rev() {
        if !edit.range.is_empty() {
            let deleted_text = scratch.delete(edit.range).ok()?;
            commands.push(Command::Delete {
                range: edit.range,
                deleted_text,
            });
        }
        if !edit.text.is_empty() {
            scratch.insert(edit.range.start, &edit.text).ok()?;
            commands.push(Command::Insert {
                position: edit.range.start,
                text: edit.text.clone(),
            });
        }
    }
    Some((commands, scratch))
}

/// Remaps a byte offset in the original document through sorted,
/// non-overlapping edits to the corresponding offset in the edited document.
///
/// Offsets at or after an edit's end shift by the edit's byte delta (an
/// insertion exactly at the offset shifts it right, so selections keep
/// covering the same text after an indent). Offsets inside a removed span
/// floor at the span's start.
pub(super) fn remap_offset(
    offset: usize,
    edits: &[CursorEdit],
    offsets: &[(usize, usize)],
) -> Option<usize> {
    let mut delta: i64 = 0;
    for (edit, &(start, end)) in edits.iter().zip(offsets) {
        let before_biased_insertion =
            start == end && end == offset && edit.bias == InsertBias::Before;
        if before_biased_insertion {
            // The endpoint stays before this insertion's text; later edits
            // at the same position may still apply, so keep scanning.
            continue;
        }
        if end <= offset {
            delta += i64::try_from(edit.text.len()).ok()?;
            delta -= i64::try_from(end - start).ok()?;
        } else if start <= offset {
            // Inside a removed span: floor at the edit's start.
            return usize::try_from(i64::try_from(start).ok()? + delta).ok();
        } else {
            break;
        }
    }
    usize::try_from(i64::try_from(offset).ok()? + delta).ok()
}

/// Rebuilds a cursor state from post-edit selections, keeping the cursor at
/// `primary_index` primary and merging any cursors that converged on the same
/// position.
fn cursor_state_from(selections: &[Selection], primary_index: usize) -> Option<CursorState> {
    let primary = selections.get(primary_index).copied()?;
    let mut state = CursorState::new(primary);
    for (index, selection) in selections.iter().enumerate() {
        if index != primary_index {
            state.add_cursor(*selection);
        }
    }
    Some(state)
}

/// Appends the trailing `SetSelection` and wraps the commands into a single
/// reversible command.
///
/// The cursor transition is always recorded when content changed, even if no
/// caret moved (e.g. delete-forward leaves every caret in place): the
/// `SetSelection` is what lets the command's inverse restore the exact
/// multi-cursor state on undo. Without it, history replay has no cursor
/// context and must fall back to a single caret at the edit site.
fn finalize_command(
    cursor: &CursorState,
    new_state: CursorState,
    mut commands: Vec<Command>,
) -> Option<Command> {
    if !commands.is_empty() || new_state != *cursor {
        commands.push(Command::SetSelection {
            old_state: cursor.clone(),
            new_state,
        });
    }

    match commands.len() {
        0 => None,
        1 => commands.pop(),
        _ => Some(Command::Compound { commands }),
    }
}
