//! Whole-line editing operations: move, duplicate, delete, and join.
//!
//! Each operation produces a single reversible [`Command`] via
//! [`editing::build_command_with_offsets`]: the operation computes its edits
//! plus every cursor's post-edit anchor and head as byte offsets, because
//! none of the generic caret rules fit line operations — carets must *ride
//! with* moved text, land inside duplicated copies, keep their column
//! through a line deletion, or jump to a join point.
//!
//! All operations are multi-cursor correct and undo as one step (text and
//! all cursors together):
//!
//! - **Move** ([`move_lines`]): each cursor's touched line block swaps with
//!   the adjacent line, selections and carets riding with their text.
//!   Cursors sharing a block move it once, and contiguous blocks — which
//!   would collide while moving — merge and move as one, per VS Code. Blocks
//!   at the document edge stay put without blocking the others.
//! - **Duplicate** ([`duplicate_lines`]): collapsed cursors duplicate their
//!   line (once per line, however many cursors share it); selections
//!   duplicate the selected text inline. Duplicating down puts each cursor
//!   on the copy; duplicating up keeps it on the original, matching
//!   VS Code's copy-line commands.
//! - **Delete** ([`delete_lines`]): removes each cursor's line block with
//!   its line ending (the last line consumes the preceding ending, mirroring
//!   line-cut semantics); carets keep their column on the line that takes
//!   the block's place, clamped to its length.
//! - **Join** ([`join_lines`]): joins each cursor's line with the next (a
//!   multi-line selection joins every line it spans), replacing the line
//!   ending with a single space and stripping the joined line's leading
//!   whitespace. Collapsed carets land on the join point; selections keep
//!   covering the same text. Cursors on the last line contribute nothing.

use std::collections::BTreeMap;

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::history::Command;

use super::editing::{self, CursorEdit};
use super::motions::VerticalDirection;

// ========== Line blocks ==========

/// The inclusive range of lines a selection touches for line-block
/// operations (move and delete).
///
/// A multi-line selection ending at column 0 does not touch its final line
/// (the line under a line-wise selection's trailing caret is not part of the
/// selected block, matching VS Code and the indent behaviors); collapsed
/// cursors touch exactly their own line.
fn selection_block(sel: &Selection) -> (usize, usize) {
    let start = sel.start();
    let end = sel.end();
    let mut last = end.line;
    if end.line > start.line && end.column == 0 {
        last -= 1;
    }
    (start.line, last)
}

/// A maximal run of contiguous line blocks and the cursors that own them.
struct BlockGroup {
    /// First line of the merged block.
    first: usize,
    /// Last line of the merged block (inclusive).
    last: usize,
    /// Indices (in [`CursorState::all_selections`] order) of the cursors
    /// whose blocks merged into this group.
    members: Vec<usize>,
}

/// Groups every cursor's line block into document-ordered, merged blocks.
///
/// Blocks that overlap or touch are merged: cursors sharing a line act on it
/// once, and adjacent blocks — which would collide while moving — move (or
/// delete) as a single unit, matching VS Code.
fn merged_blocks(selections: &[Selection]) -> Vec<BlockGroup> {
    let mut order: Vec<usize> = (0..selections.len()).collect();
    order.sort_by_key(|&index| selections.get(index).map(Selection::start));

    let mut groups: Vec<BlockGroup> = Vec::new();
    for index in order {
        let Some(selection) = selections.get(index) else {
            continue;
        };
        let (first, last) = selection_block(selection);
        match groups.last_mut() {
            Some(group) if first <= group.last + 1 => {
                group.last = group.last.max(last);
                group.members.push(index);
            },
            _ => groups.push(BlockGroup {
                first,
                last,
                members: vec![index],
            }),
        }
    }
    groups
}

/// Byte offset of `position` shifted by `delta` bytes.
fn shifted_offset(document: &Document, position: Position, delta: i64) -> Option<usize> {
    let offset = i64::try_from(document.position_to_offset(position)?).ok()?;
    usize::try_from(offset + delta).ok()
}

/// Byte offset of the character at `column` within `text` (the text's length
/// when `column` is at or past the end).
fn byte_column(text: &str, column: usize) -> usize {
    text.char_indices()
        .nth(column)
        .map_or(text.len(), |(index, _)| index)
}

// ========== Move (Alt+Up / Alt+Down) ==========

/// Moves every cursor's line block one line up or down.
///
/// Each merged block swaps with the line adjacent to it in `direction`: the
/// adjacent line is removed together with its line ending and reinserted on
/// the block's other side, so every selection and caret in the block keeps
/// its column and relative position while shifting exactly one line. Blocks
/// already at the document edge stay put (their cursors do not move) without
/// blocking other blocks.
///
/// Returns `None` when nothing can move (every block sits at the edge).
pub(super) fn move_lines(
    document: &Document,
    cursor: &CursorState,
    direction: VerticalDirection,
) -> Option<Command> {
    let last_line = document.line_count().saturating_sub(1);
    let selections: Vec<Selection> = cursor.all_selections().copied().collect();
    let groups = merged_blocks(&selections);

    let mut edits: Vec<CursorEdit> = Vec::new();
    let mut deltas: Vec<i64> = vec![0; selections.len()];
    for group in &groups {
        // Every swap removes exactly one separator (line-ending bytes) with
        // the displaced line and reinserts those same bytes on the block's
        // other side: on mixed-ending documents the separator actually
        // present need not match the document's dominant ending, so both the
        // reinserted text and the cursor delta must use the real bytes, not
        // `document.line_ending()`.
        let displaced_delta = match direction {
            VerticalDirection::Up => {
                if group.first == 0 {
                    continue; // Block already at the top: it stays put.
                }
                let displaced_line = group.first - 1;
                let displaced = document.line(displaced_line)?;
                // The displaced line's own ending: the exact bytes between
                // its content and the block's first line.
                let separator = document.slice(Range::new(
                    Position::new(displaced_line, document.line_len(displaced_line)?),
                    Position::new(group.first, 0),
                ));
                // Remove the line above together with its ending...
                edits.push(CursorEdit::delete(Range::new(
                    Position::new(displaced_line, 0),
                    Position::new(group.first, 0),
                )));
                // ...and reinsert it directly below the block.
                if group.last < last_line {
                    // Interior block: reinsert `content + separator` *after*
                    // the block's own trailing ending (at the start of the
                    // line below), so the displaced line keeps its own
                    // ending and the block keeps its own — the exact mirror
                    // of the Down branch. On mixed-ending documents this
                    // makes Alt+Up the byte-for-byte inverse of Alt+Down
                    // instead of swapping which line owns which ending.
                    edits.push(CursorEdit::replace(
                        Range::new(
                            Position::new(group.last + 1, 0),
                            Position::new(group.last + 1, 0),
                        ),
                        format!("{displaced}{separator}"),
                    ));
                } else {
                    // The block ends on the last line, which has no trailing
                    // ending to preserve: the displaced line becomes the new
                    // (ending-less) last line, so its separator bytes move
                    // above it as the block's new trailing ending.
                    let end_column = document.line_len(group.last)?;
                    edits.push(CursorEdit::replace(
                        Range::new(
                            Position::new(group.last, end_column),
                            Position::new(group.last, end_column),
                        ),
                        format!("{separator}{displaced}"),
                    ));
                }
                // The block shifts up by exactly the bytes removed above it.
                -i64::try_from(displaced.len() + separator.len()).ok()?
            },
            VerticalDirection::Down => {
                if group.last >= last_line {
                    continue; // Block already at the bottom: it stays put.
                }
                let displaced_line = group.last + 1;
                let displaced = document.line(displaced_line)?;
                let separator = if displaced_line == last_line {
                    // The displaced line is the last line: it has no
                    // trailing ending, so remove the block's own trailing
                    // ending with it (the block's new last line must not
                    // keep one) and reuse those exact bytes as the
                    // separator above the block.
                    let end_column = document.line_len(group.last)?;
                    let separator = document.slice(Range::new(
                        Position::new(group.last, end_column),
                        Position::new(displaced_line, 0),
                    ));
                    edits.push(CursorEdit::delete(Range::new(
                        Position::new(group.last, end_column),
                        Position::new(displaced_line, document.line_len(displaced_line)?),
                    )));
                    separator
                } else {
                    // The displaced line's own ending rides down with it.
                    let separator = document.slice(Range::new(
                        Position::new(displaced_line, document.line_len(displaced_line)?),
                        Position::new(displaced_line + 1, 0),
                    ));
                    edits.push(CursorEdit::delete(Range::new(
                        Position::new(displaced_line, 0),
                        Position::new(displaced_line + 1, 0),
                    )));
                    separator
                };
                // Reinsert the displaced line directly above the block.
                edits.push(CursorEdit::replace(
                    Range::new(Position::new(group.first, 0), Position::new(group.first, 0)),
                    format!("{displaced}{separator}"),
                ));
                // The block shifts down by exactly the bytes removed below
                // it.
                i64::try_from(displaced.len() + separator.len()).ok()?
            },
        };
        for &member in &group.members {
            if let Some(slot) = deltas.get_mut(member) {
                *slot = displaced_delta;
            }
        }
    }

    if edits.is_empty() {
        return None;
    }

    // Every endpoint of a moved block shifts by exactly the displaced line's
    // byte length (other blocks' edits are delete/reinsert pairs with a net
    // delta of zero, so they never affect it); unmoved blocks stay put.
    let mut offsets: Vec<(usize, usize)> = Vec::with_capacity(selections.len());
    for (sel, delta) in selections.iter().zip(&deltas) {
        let anchor = shifted_offset(document, sel.anchor, *delta)?;
        let head = shifted_offset(document, sel.head, *delta)?;
        offsets.push((anchor, head));
    }
    editing::build_command_with_offsets(document, cursor, edits, offsets)
}

// ========== Duplicate (Shift+Alt+Up / Shift+Alt+Down) ==========

/// One duplication insertion, shared by every collapsed cursor on a line for
/// line duplications.
struct DupEdit {
    /// Insertion point in the original document.
    position: Position,
    /// Byte offset of `position` in the original document.
    start_offset: usize,
    /// The full inserted text.
    text: String,
}

/// Where a cursor lands relative to its own duplication edit.
enum DupPlan {
    /// Collapsed cursor duplicating its line: when duplicating down it lands
    /// `head_in_text` bytes into the inserted copy.
    Line {
        /// Index of the cursor's duplication edit.
        edit: usize,
        /// Byte offset of the caret column within the inserted text.
        head_in_text: usize,
    },
    /// Selection duplicated inline: when duplicating down it covers the
    /// inserted copy, preserving the selection's orientation.
    Inline {
        /// Index of the cursor's duplication edit.
        edit: usize,
        /// True when the original selection is forward (anchor before head).
        forward: bool,
    },
}

/// Duplicates every cursor's line or selection.
///
/// Collapsed cursors duplicate their whole line: the copy is inserted after
/// the line (`ending + content`, which also covers the last line), and
/// cursors sharing a line duplicate it once. Non-collapsed selections
/// duplicate the selected text inline at the selection's end.
///
/// `direction` only affects where the cursors land — the duplicated text is
/// identical either way: duplicating **down** puts each cursor on the copy
/// (one line down for line duplications, covering the inserted copy for
/// selections); duplicating **up** keeps each cursor on the original,
/// matching VS Code's copy-line-up/down commands.
pub(super) fn duplicate_lines(
    document: &Document,
    cursor: &CursorState,
    direction: VerticalDirection,
) -> Option<Command> {
    let ending = document.line_ending().as_str();
    let selections: Vec<Selection> = cursor.all_selections().copied().collect();

    // One edit per duplicated unit, plus each cursor's landing plan.
    let mut edits: Vec<DupEdit> = Vec::new();
    let mut line_edits: BTreeMap<usize, usize> = BTreeMap::new();
    let mut plans: Vec<DupPlan> = Vec::with_capacity(selections.len());
    for sel in &selections {
        if sel.is_collapsed() {
            let line = sel.head.line;
            let edit = if let Some(&index) = line_edits.get(&line) {
                index
            } else {
                let content = document.line(line)?;
                let position = Position::new(line, document.line_len(line)?);
                let index = edits.len();
                edits.push(DupEdit {
                    position,
                    start_offset: document.position_to_offset(position)?,
                    text: format!("{ending}{content}"),
                });
                line_edits.insert(line, index);
                index
            };
            let content = edits.get(edit)?.text.get(ending.len()..)?;
            plans.push(DupPlan::Line {
                edit,
                head_in_text: ending.len() + byte_column(content, sel.head.column),
            });
        } else {
            let position = sel.end();
            plans.push(DupPlan::Inline {
                edit: edits.len(),
                forward: sel.is_forward(),
            });
            edits.push(DupEdit {
                position,
                start_offset: document.position_to_offset(position)?,
                text: document.slice(sel.range()),
            });
        }
    }

    // Document-ordered view of the edits with cumulative inserted bytes.
    // Ties keep creation order; the command builder's stable sort preserves
    // the same order, so the offset accounting below matches the applied
    // edits exactly.
    let mut sorted: Vec<usize> = (0..edits.len()).collect();
    sorted.sort_by_key(|&index| edits.get(index).map_or(0, |edit| edit.start_offset));
    let mut rank = vec![0usize; edits.len()];
    for (position, &index) in sorted.iter().enumerate() {
        if let Some(slot) = rank.get_mut(index) {
            *slot = position;
        }
    }
    let mut prefix: Vec<usize> = Vec::with_capacity(sorted.len() + 1);
    prefix.push(0);
    for &index in &sorted {
        let inserted = edits.get(index).map_or(0, |edit| edit.text.len());
        prefix.push(prefix.last().copied().unwrap_or(0) + inserted);
    }
    let starts: Vec<usize> = sorted
        .iter()
        .map(|&index| edits.get(index).map_or(0, |edit| edit.start_offset))
        .collect();
    // Bytes inserted by edits strictly before `offset` in the original
    // document. Insertions exactly at `offset` do not push it: a cursor
    // keeping its place must stay before a copy inserted at its position.
    let inserted_before = |offset: usize| -> usize {
        let index = starts.partition_point(|&start| start < offset);
        prefix.get(index).copied().unwrap_or(0)
    };

    let mut offsets: Vec<(usize, usize)> = Vec::with_capacity(selections.len());
    for (sel, plan) in selections.iter().zip(&plans) {
        let pair = match direction {
            // Duplicating up keeps every cursor (and selection) on the
            // original text; only insertions at earlier positions shift it.
            VerticalDirection::Up => {
                let anchor = document.position_to_offset(sel.anchor)?;
                let head = document.position_to_offset(sel.head)?;
                (
                    anchor + inserted_before(anchor),
                    head + inserted_before(head),
                )
            },
            // Duplicating down puts each cursor on its own copy: the copy
            // starts at the edit's insertion point plus everything inserted
            // by edits ordered before it.
            VerticalDirection::Down => match *plan {
                DupPlan::Line { edit, head_in_text } => {
                    let start =
                        edits.get(edit)?.start_offset + prefix.get(*rank.get(edit)?).copied()?;
                    let head = start + head_in_text;
                    (head, head)
                },
                DupPlan::Inline { edit, forward } => {
                    let start =
                        edits.get(edit)?.start_offset + prefix.get(*rank.get(edit)?).copied()?;
                    let end = start + edits.get(edit)?.text.len();
                    if forward { (start, end) } else { (end, start) }
                },
            },
        };
        offsets.push(pair);
    }

    let cursor_edits: Vec<CursorEdit> = sorted
        .iter()
        .filter_map(|&index| edits.get(index))
        .map(|edit| {
            CursorEdit::replace(Range::new(edit.position, edit.position), edit.text.clone())
        })
        .collect();
    editing::build_command_with_offsets(document, cursor, cursor_edits, offsets)
}

// ========== Delete lines (Ctrl+Shift+K) ==========

/// Deletes every cursor's line block, including its line ending.
///
/// Blocks that share or touch lines are removed once. Interior blocks are
/// removed together with their trailing line endings; a block ending on the
/// last line consumes the preceding ending instead (mirroring line-cut
/// semantics), and a block covering the whole document leaves a single empty
/// line. Every cursor collapses to its own column — clamped to the new
/// line's length — on the line that takes the block's place: the line below
/// it, or the line above when the block reached the end of the document.
///
/// Returns `None` only when nothing changes (deleting the sole, empty line).
pub(super) fn delete_lines(document: &Document, cursor: &CursorState) -> Option<Command> {
    let last_line = document.line_count().saturating_sub(1);
    let selections: Vec<Selection> = cursor.all_selections().copied().collect();
    let groups = merged_blocks(&selections);

    let mut edits: Vec<CursorEdit> = Vec::with_capacity(groups.len());
    let mut offsets: Vec<(usize, usize)> = vec![(0, 0); selections.len()];
    let mut removed_before: usize = 0;
    for group in &groups {
        // The deleted range, the line whose start replaces the block, and
        // whether that line sits after the deleted range in the original
        // document (and therefore absorbs the group's own removed bytes).
        let (range, target_line, target_after_range) = if group.last < last_line {
            (
                Range::new(
                    Position::new(group.first, 0),
                    Position::new(group.last + 1, 0),
                ),
                Some(group.last + 1),
                true,
            )
        } else if group.first > 0 {
            let previous = group.first - 1;
            (
                Range::new(
                    Position::new(previous, document.line_len(previous)?),
                    Position::new(group.last, document.line_len(group.last)?),
                ),
                Some(previous),
                false,
            )
        } else {
            // The block covers the whole document: an empty line remains.
            (
                Range::new(
                    Position::zero(),
                    Position::new(group.last, document.line_len(group.last)?),
                ),
                None,
                false,
            )
        };

        let start_offset = document.position_to_offset(range.start)?;
        let end_offset = document.position_to_offset(range.end)?;
        let removed = end_offset.checked_sub(start_offset)?;

        for &member in &group.members {
            let offset = match target_line {
                Some(line) => {
                    let column = selections
                        .get(member)?
                        .head
                        .column
                        .min(document.line_len(line)?);
                    let base = document.position_to_offset(Position::new(line, column))?;
                    let own = if target_after_range { removed } else { 0 };
                    base.checked_sub(removed_before + own)?
                },
                None => 0,
            };
            if let Some(slot) = offsets.get_mut(member) {
                *slot = (offset, offset);
            }
        }

        removed_before += removed;
        edits.push(CursorEdit::delete(range));
    }

    editing::build_command_with_offsets(document, cursor, edits, offsets)
}

// ========== Join lines (Ctrl+J) ==========

/// Joins every cursor's line with the next.
///
/// Each join replaces the line ending (and the next line's leading
/// whitespace) with a single space. A collapsed cursor or single-line
/// selection joins its line with the following one; a multi-line selection
/// joins every line it spans into one. Cursors sharing a join contribute it
/// once.
///
/// Collapsed carets land on the join point (the end of their line's original
/// content); selections keep covering the same text through the joins.
/// Cursors on the last line have nothing to join and stay put.
///
/// Returns `None` when no cursor has a line below it to join with.
pub(super) fn join_lines(document: &Document, cursor: &CursorState) -> Option<Command> {
    let last_line = document.line_count().saturating_sub(1);

    // The edits for each joined line, deduplicated across cursors. A join
    // is *two* adjacent edits — replace the line ending with a single
    // space, then delete the joined line's leading whitespace — rather than
    // one replacement spanning both: [`editing::remap_offset`] floors every
    // endpoint strictly inside a replaced range at its start, so a
    // selection endpoint at the joined line's column zero must sit *between*
    // the edits to remap after the inserted space (a single spanning
    // replacement would floor it before the space, shortening the
    // selection whenever the joined line is indented).
    let mut joins: BTreeMap<usize, Vec<CursorEdit>> = BTreeMap::new();
    for sel in cursor.all_selections() {
        let start = sel.start();
        let end = sel.end();
        let (first, last) = if end.line > start.line {
            (start.line, end.line)
        } else {
            (start.line, start.line + 1)
        };
        if first >= last_line {
            continue; // Nothing below this cursor's line to join with.
        }
        for line in first..last {
            if joins.contains_key(&line) {
                continue;
            }
            let column = document.line_len(line)?;
            let next = document.line(line + 1)?;
            let lead = next.chars().take_while(|c| c.is_whitespace()).count();
            let mut pair = vec![CursorEdit::replace(
                Range::new(Position::new(line, column), Position::new(line + 1, 0)),
                " ".to_string(),
            )];
            if lead > 0 {
                pair.push(CursorEdit::delete(Range::new(
                    Position::new(line + 1, 0),
                    Position::new(line + 1, lead),
                )));
            }
            joins.insert(line, pair);
        }
    }
    if joins.is_empty() {
        return None;
    }

    let edits: Vec<CursorEdit> = joins.into_values().flatten().collect();
    let mut edit_offsets: Vec<(usize, usize)> = Vec::with_capacity(edits.len());
    for edit in &edits {
        edit_offsets.push((
            document.position_to_offset(edit.range.start)?,
            document.position_to_offset(edit.range.end)?,
        ));
    }

    let mut offsets: Vec<(usize, usize)> = Vec::with_capacity(cursor.cursor_count());
    for sel in cursor.all_selections() {
        if sel.is_collapsed() {
            // The caret jumps to its join point: the end of its line's
            // original content (remapping the join edit's start floors at
            // the join point after all earlier joins). A caret on the last
            // line joined nothing and only rides earlier cursors' joins.
            let source = if sel.head.line < last_line {
                Position::new(sel.head.line, document.line_len(sel.head.line)?)
            } else {
                sel.head
            };
            let offset =
                editing::remap_offset(document.position_to_offset(source)?, &edits, &edit_offsets)?;
            offsets.push((offset, offset));
        } else {
            // Selections keep covering the same text through the joins: an
            // endpoint at the joined line's column zero lands just after
            // the space that replaced the line ending it selected, and
            // endpoints inside stripped whitespace floor there too.
            let anchor = editing::remap_offset(
                document.position_to_offset(sel.anchor)?,
                &edits,
                &edit_offsets,
            )?;
            let head = editing::remap_offset(
                document.position_to_offset(sel.head)?,
                &edits,
                &edit_offsets,
            )?;
            offsets.push((anchor, head));
        }
    }
    editing::build_command_with_offsets(document, cursor, edits, offsets)
}
