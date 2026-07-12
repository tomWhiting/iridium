//! Configuration-driven editing behaviors.
//!
//! This module computes per-cursor edit intents for the behaviors that depend
//! on [`EditorConfig`]:
//!
//! - **Tab / Shift+Tab**: tab-stop-aware insertion for collapsed cursors,
//!   whole-line indent for selections, and outdent
//!   ([`tab_insert_edits`], [`indent_edits`], [`outdent_edits`]).
//! - **Enter**: auto-indent inheritance, bracket-block expansion, and
//!   code-fence expansion ([`enter_edits`]).
//! - **Auto-pairs**: pair insertion, closer skip-over, pair backspace, and
//!   selection wrapping ([`auto_pair_char_edits`],
//!   [`backspace_edits_with_pairs`]).
//!
//! Every function returns plain edit intents; the reversible multi-cursor
//! command is always built by [`super::editing`], so each behavior is a
//! single undoable step that restores text and all cursors together.

use std::collections::BTreeSet;

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::editor::EditorConfig;

use super::editing::{CaretPlacement, CursorEdit};
use super::motions;

// ========== Character classes and pair tables ==========

/// Returns the closing character for an auto-pair opener.
///
/// Openers are the three brackets and the three quotes (which close with
/// themselves). Returns `None` for every other character.
const fn pair_close(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' | '\'' | '`' => Some(c),
        _ => None,
    }
}

/// Returns the closing bracket for `(`, `[`, or `{` (quotes excluded).
///
/// Bracket-block Enter expansion applies only to brackets, never to quotes.
const fn bracket_close(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

/// Returns true for the three auto-paired quote characters.
const fn is_quote(c: char) -> bool {
    matches!(c, '"' | '\'' | '`')
}

/// Returns true for characters that can close an auto-pair (closing brackets
/// and quotes), i.e. the characters eligible for skip-over.
const fn is_pair_closer(c: char) -> bool {
    matches!(c, ')' | ']' | '}') || is_quote(c)
}

/// Returns true when typing `c` engages auto-pair handling at all (it is an
/// opener, a closer, or a quote).
pub(super) const fn is_auto_pair_trigger(c: char) -> bool {
    pair_close(c).is_some() || is_pair_closer(c)
}

// ========== Config helpers ==========

/// The configured tab width, floored at 1 so a zero width from a hostile or
/// corrupt configuration can never cause a division by zero or an empty
/// indent level.
const fn tab_width(config: &EditorConfig) -> usize {
    if config.tab_width == 0 {
        1
    } else {
        config.tab_width
    }
}

/// One level of indentation per the configuration: `tab_width` spaces when
/// `insert_spaces` is set, a single tab otherwise.
fn indent_unit(config: &EditorConfig) -> String {
    if config.insert_spaces {
        " ".repeat(tab_width(config))
    } else {
        "\t".to_string()
    }
}

// ========== Document inspection helpers ==========

/// Character at `position` on its own line (`None` at or past end of line;
/// line endings are never returned).
fn char_at(document: &Document, position: Position) -> Option<char> {
    document.line(position.line)?.chars().nth(position.column)
}

/// Character immediately before `position` on the same line (`None` at
/// column 0; a preceding line ending is never returned).
fn char_before(document: &Document, position: Position) -> Option<char> {
    if position.column == 0 {
        return None;
    }
    document
        .line(position.line)?
        .chars()
        .nth(position.column - 1)
}

/// The leading-whitespace prefix of `text`.
fn leading_whitespace(text: &str) -> &str {
    &text[..text.len() - text.trim_start().len()]
}

// ========== Tab / indent / outdent ==========

/// Builds one Tab-insertion edit per cursor (used when every cursor is
/// collapsed).
///
/// With `insert_spaces`, each cursor inserts enough spaces to reach the next
/// tab stop from its own column (column 2 with width 4 inserts 2 spaces);
/// otherwise each cursor inserts a literal tab. Columns are taken from the
/// pre-edit document; the command builder accounts for the byte shifts
/// between same-line cursors.
pub(super) fn tab_insert_edits(cursor: &CursorState, config: &EditorConfig) -> Vec<CursorEdit> {
    let width = tab_width(config);
    cursor
        .all_selections()
        .map(|sel| {
            let text = if config.insert_spaces {
                " ".repeat(width - (sel.head.column % width))
            } else {
                "\t".to_string()
            };
            CursorEdit::replace(sel.range(), text)
        })
        .collect()
}

/// Builds one indent insertion per line touched by any cursor or selection.
///
/// Each touched line gets one level of indentation prepended at column 0.
/// The command builder remaps all selections so they keep covering the same
/// text.
pub(super) fn indent_edits(
    document: &Document,
    cursor: &CursorState,
    config: &EditorConfig,
) -> Vec<CursorEdit> {
    let unit = indent_unit(config);
    touched_lines(document, cursor)
        .into_iter()
        .map(|line| {
            let start = Position::new(line, 0);
            CursorEdit::replace(Range::new(start, start), unit.clone())
        })
        .collect()
}

/// Builds one outdent deletion per line touched by any cursor or selection.
///
/// Each touched line loses up to one level of leading indentation: a single
/// leading tab, or up to `tab_width` leading spaces. Non-whitespace is never
/// removed; lines without leading indentation contribute no edit. Cursors on
/// an outdented line shift left by the amount actually removed, flooring at
/// the indentation boundary (column 0).
pub(super) fn outdent_edits(
    document: &Document,
    cursor: &CursorState,
    config: &EditorConfig,
) -> Vec<CursorEdit> {
    let width = tab_width(config);
    touched_lines(document, cursor)
        .into_iter()
        .filter_map(|line| {
            let text = document.line(line)?;
            let mut chars = text.chars();
            let removed = match chars.next() {
                Some('\t') => 1,
                Some(' ') => 1 + chars.take(width - 1).take_while(|&c| c == ' ').count(),
                _ => 0,
            };
            (removed > 0).then(|| {
                CursorEdit::delete(Range::new(
                    Position::new(line, 0),
                    Position::new(line, removed),
                ))
            })
        })
        .collect()
}

/// Every line touched by any cursor or selection, deduplicated and in
/// document order.
///
/// A multi-line selection ending at column 0 does not touch its final line
/// (the line under a line-wise selection's trailing caret is not part of the
/// selected block); collapsed cursors always touch their own line.
fn touched_lines(document: &Document, cursor: &CursorState) -> BTreeSet<usize> {
    let last_line = document.line_count().saturating_sub(1);
    let mut lines = BTreeSet::new();
    for sel in cursor.all_selections() {
        let start = sel.start();
        let end = sel.end();
        let mut last = end.line;
        if !sel.is_collapsed() && end.line > start.line && end.column == 0 {
            last -= 1;
        }
        for line in start.line..=last.min(last_line) {
            lines.insert(line);
        }
    }
    lines
}

// ========== Enter (auto-indent, bracket block, code fence) ==========

/// Builds one Enter edit per cursor.
///
/// With `auto_indent` disabled every cursor inserts the bare line ending.
/// Otherwise each cursor independently applies, in order of precedence:
///
/// 1. **Code-fence expansion**: when the text before the caret on its line is
///    an opening fence (`^\s*```\w*$`), Enter produces a blank line carrying
///    the fence's indentation plus a closing fence line, with the caret on
///    the middle line.
/// 2. **Bracket-block expansion**: when the character before the caret is an
///    opening bracket and the character after it is the matching closer,
///    Enter produces an indented body line plus the closer's line, with the
///    caret at the end of the body line. An opener without its closer adds
///    one extra indent level to the new line.
/// 3. **Indent inheritance**: the new line inherits the caret line's leading
///    whitespace (truncated at the caret when the caret sits inside the
///    indentation).
///
/// Cursors with a selection replace it; the characters around the selection
/// (before its start, after its end) drive the bracket rules, matching the
/// delete-then-break order of operations.
pub(super) fn enter_edits(
    document: &Document,
    cursor: &CursorState,
    config: &EditorConfig,
) -> Vec<(CursorEdit, CaretPlacement)> {
    let line_ending = document.line_ending().as_str();
    cursor
        .all_selections()
        .map(|sel| {
            if config.auto_indent {
                enter_edit_for(document, sel, config, line_ending)
            } else {
                let text = line_ending.to_string();
                let placement = CaretPlacement::collapsed(text.len());
                (CursorEdit::replace(sel.range(), text), placement)
            }
        })
        .collect()
}

/// Computes a single cursor's auto-indented Enter edit.
fn enter_edit_for(
    document: &Document,
    sel: &Selection,
    config: &EditorConfig,
    line_ending: &str,
) -> (CursorEdit, CaretPlacement) {
    let start = sel.start();
    let end = sel.end();
    let line_text = document.line(start.line).unwrap_or_default();
    let prefix: String = line_text.chars().take(start.column).collect();
    let indent = leading_whitespace(&prefix);

    // Code-fence expansion: the text before the caret is an unclosed opening
    // fence.
    if let Some(fence_indent) = unclosed_fence_indent(&prefix) {
        let mut text = String::new();
        text.push_str(line_ending);
        text.push_str(fence_indent);
        let caret = text.len();
        text.push_str(line_ending);
        text.push_str(fence_indent);
        text.push_str("```");
        return (
            CursorEdit::replace(sel.range(), text),
            CaretPlacement::collapsed(caret),
        );
    }

    // Bracket-block expansion and opener indentation.
    if let Some(close) = prefix.chars().last().and_then(bracket_close) {
        let unit = indent_unit(config);
        if char_at(document, end) == Some(close) {
            // Opener directly before, matching closer directly after: expand
            // to an indented body line and keep the closer on its own line.
            let mut text = String::new();
            text.push_str(line_ending);
            text.push_str(indent);
            text.push_str(&unit);
            let caret = text.len();
            text.push_str(line_ending);
            text.push_str(indent);
            return (
                CursorEdit::replace(sel.range(), text),
                CaretPlacement::collapsed(caret),
            );
        }
        // Opener without its closer: one extra indent level.
        let text = format!("{line_ending}{indent}{unit}");
        let placement = CaretPlacement::collapsed(text.len());
        return (CursorEdit::replace(sel.range(), text), placement);
    }

    // Plain auto-indent: inherit the leading whitespace.
    let text = format!("{line_ending}{indent}");
    let placement = CaretPlacement::collapsed(text.len());
    (CursorEdit::replace(sel.range(), text), placement)
}

/// When `prefix` (the text before the caret on its line) is an unclosed
/// opening code fence — leading whitespace, three backticks, and an optional
/// info string of alphanumeric characters or underscores — returns the
/// fence's leading whitespace. Returns `None` otherwise.
///
/// The info-string character class is deliberately `is_alphanumeric() || '_'`
/// (per [`motions::is_word_char`]), not full regex `\w` semantics: real fence
/// language tags (`rust`, `py`, `objective_c`) are alphanumeric, and
/// accepting combining marks or connector punctuation here would only widen
/// false positives.
fn unclosed_fence_indent(prefix: &str) -> Option<&str> {
    let trimmed = prefix.trim_start();
    let info = trimmed.strip_prefix("```")?;
    if info.chars().all(motions::is_word_char) {
        Some(leading_whitespace(prefix))
    } else {
        None
    }
}

// ========== Auto-pairs ==========

/// Builds per-cursor edits for typing the auto-pair character `c`.
///
/// Only call for characters where [`is_auto_pair_trigger`] is true and the
/// configuration enables auto-pairs. Each cursor independently applies:
///
/// - **Selection wrap**: a non-collapsed selection typed over with an opener
///   (bracket or quote) becomes `open + selection + close`, with the
///   selection preserved (same orientation) around the original text. A
///   closing bracket over a selection replaces it, as ordinary typing does.
/// - **Skip-over**: a collapsed cursor typing a closer that already sits
///   directly after the caret moves over it without inserting.
/// - **Pair insertion**: a collapsed cursor typing an opener inserts the pair
///   with the caret between the halves. Quotes do not pair when the caret is
///   directly after a word character (apostrophes inside words), inserting a
///   single quote instead.
pub(super) fn auto_pair_char_edits(
    document: &Document,
    cursor: &CursorState,
    c: char,
) -> Vec<(CursorEdit, CaretPlacement)> {
    cursor
        .all_selections()
        .map(|sel| auto_pair_edit_for(document, sel, c))
        .collect()
}

/// Computes a single cursor's edit for typing the auto-pair character `c`.
fn auto_pair_edit_for(
    document: &Document,
    sel: &Selection,
    c: char,
) -> (CursorEdit, CaretPlacement) {
    if !sel.is_collapsed() {
        if let Some(close) = pair_close(c) {
            // Wrap the selection, preserving the selected text and the
            // selection's orientation inside the pair.
            let inner = document.slice(sel.range());
            let open_len = c.len_utf8();
            let text = format!("{c}{inner}{close}");
            let placement = if sel.is_backward() {
                CaretPlacement::selection(open_len + inner.len(), open_len)
            } else {
                CaretPlacement::selection(open_len, open_len + inner.len())
            };
            return (CursorEdit::replace(sel.range(), text), placement);
        }
        // A bare closer replaces the selection like any typed character.
        return plain_char_edit(sel, c);
    }

    // Skip over an existing closer instead of inserting a duplicate.
    if is_pair_closer(c) && char_at(document, sel.head) == Some(c) {
        let target = Position::new(sel.head.line, sel.head.column + 1);
        return (
            CursorEdit::replace(Range::new(target, target), String::new()),
            CaretPlacement::collapsed(0),
        );
    }

    if let Some(close) = pair_close(c) {
        // Quotes stay single directly after a word character (don't, it's).
        if is_quote(c) && char_before(document, sel.head).is_some_and(motions::is_word_char) {
            return plain_char_edit(sel, c);
        }
        let text = format!("{c}{close}");
        let placement = CaretPlacement::collapsed(c.len_utf8());
        return (CursorEdit::replace(sel.range(), text), placement);
    }

    plain_char_edit(sel, c)
}

/// The ordinary typing edit: replace the selection with `c`, caret after it.
fn plain_char_edit(sel: &Selection, c: char) -> (CursorEdit, CaretPlacement) {
    let text = c.to_string();
    let placement = CaretPlacement::collapsed(text.len());
    (CursorEdit::replace(sel.range(), text), placement)
}

/// Builds one backspace edit per cursor, deleting both halves of an empty
/// auto-pair when the caret sits between them.
///
/// Cursors with a selection delete the selection; collapsed cursors delete
/// the empty pair around the caret (`(|)`, `"|"`, …) or, failing that, the
/// single character before the caret exactly like
/// [`super::editing::backspace_edits`].
pub(super) fn backspace_edits_with_pairs(
    document: &Document,
    cursor: &CursorState,
) -> Vec<CursorEdit> {
    cursor
        .all_selections()
        .map(|sel| {
            if !sel.is_collapsed() {
                return CursorEdit::delete(sel.range());
            }
            let between_pair = char_before(document, sel.head)
                .and_then(pair_close)
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
