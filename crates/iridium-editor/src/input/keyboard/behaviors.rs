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
//! - **Auto-pairs**: pair insertion, closer skip-over and selection wrapping
//!   ([`auto_pair_char_edits`]). Backspace's side of the same rule lives in
//!   [`super::backspace_pairs`], next to the record it reads.
//!
//! Every function returns plain edit intents; the reversible multi-cursor
//! command is always built by [`super::editing`], so each behavior is a
//! single undoable step that restores text and all cursors together.

use std::collections::BTreeSet;

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::editor::{CaretScopes, EditorConfig};

use super::editing::{CaretPlacement, CursorEdit};
use super::motions;
use super::multi_char_pairs;

pub(super) use super::pair_rules::{PAIRS, PairRules};

/// Quote classification is independent of language pairing rules.
const fn is_quote(c: char) -> bool {
    matches!(c, '"' | '\'' | '`')
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
pub(super) fn char_at(document: &Document, position: Position) -> Option<char> {
    document.line(position.line)?.chars().nth(position.column)
}

/// Character immediately before `position` on the same line (`None` at
/// column 0; a preceding line ending is never returned).
pub(super) fn char_before(document: &Document, position: Position) -> Option<char> {
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
    // The document's, not the cursor's, and one manifest lookup rather than
    // one per cursor.
    let pairs = PairRules::for_document(document);
    cursor
        .all_selections()
        .map(|sel| {
            if config.auto_indent {
                enter_edit_for(
                    document,
                    sel,
                    config,
                    line_ending,
                    if document.language() == Some("markdown") {
                        PairRules::for_position(document, sel.head)
                    } else {
                        pairs
                    },
                )
            } else {
                let text = line_ending.to_string();
                let placement = CaretPlacement::collapsed(text.len());
                (CursorEdit::replace(sel.range(), text), placement)
            }
        })
        .collect()
}

/// Computes a single cursor's auto-indented Enter edit.
///
/// `pairs` decides which openers expand into a block — the language's
/// `newline` flags, not a fixed three.
fn enter_edit_for(
    document: &Document,
    sel: &Selection,
    config: &EditorConfig,
    line_ending: &str,
    pairs: PairRules,
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
    if let Some(close) = prefix.chars().last().and_then(|c| pairs.block_close(c)) {
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
/// - **Skip-over**: a collapsed cursor typing a character that already sits
///   directly after the caret moves over it without inserting, when the text
///   before the caret plus that character ends a closer the language declares
///   ([`multi_char_pairs::steps_over`]). A one-character closer satisfies that
///   with an empty prefix, so `)` and `"` behave exactly as they always have;
///   `#` closing a Rust raw string satisfies it in two keystrokes.
/// - **Multi-character pair insertion**: a collapsed cursor typing the final
///   character of an opener the language declares — `"""`, `f"`, `r#"`, `/*` —
///   inserts that row's closer, longest match first
///   ([`multi_char_pairs::insertion_for`]). Tried **before** the apostrophe
///   rule, which is what makes Python's twelve string-prefix rows reachable.
/// - **Pair insertion**: a collapsed cursor typing an opener inserts the pair
///   with the caret between the halves. Quotes do not pair when the caret is
///   directly after a word character (apostrophes inside words), inserting a
///   single quote instead; nothing pairs in front of a character the
///   language's `autoclose_before` does not list
///   ([`PairRules::permits_close_before`]); and nothing pairs inside a syntax
///   scope the language's `not_in` names ([`PairRules::suppressed_in`]).
///
/// ⚠️ **Both manifest rules gate pair insertion and nothing else here**, and
/// each omission is deliberate:
///
/// - **Selection wrap** is not gated. The character after the selection is
///   whatever the selection happened to end before, and the user asked for a
///   wrap by selecting; refusing it would turn an explicit request into a
///   typed-over selection, which destroys the text. A wrap inside a string is
///   still an explicit request.
/// - **Skip-over** is not gated. It fires on typing a *closer*, and the
///   question there is whether the character is already present, not what
///   follows it or what surrounds it.
/// - **Backspace's single-character collapse**
///   ([`super::backspace_pairs::backspace_edits_with_pairs`]) is not gated, for the same reason: it
///   acts on a pair that already exists. A pair inside a string got there
///   somehow — pasted, or typed before the string was one — and one backspace
///   must still collapse it.
///
/// That is ruling B-7: `not_in` suppresses the *close*, not the pairing
/// machinery. See `docs/design/AUTO-PAIR-MAP.md` §8.
///
/// ⚠️ **Backspace's multi-character collapse is not on that list and asks
/// `not_in` nothing**: it is gated on [`super::AutoPairInsertion`], which this
/// function's closers feed. Ruling B-13, §10.12.
///
/// `pairs` is the document language's set, built once by the caller — see
/// [`PairRules::for_document`].
///
/// `scopes` answers what each caret is syntactically inside, and is consulted
/// **per cursor** rather than once. Two carets in one edit routinely sit in
/// different scopes — one inside a string literal, one in code — and a single
/// answer for both would suppress a pair the language permits, which is the
/// fail-closed direction ruling B-6 rejected.
///
/// ⭐ The second half of the return is the closer written at each cursor, or
/// `None` where none was, aligned with the edits in
/// [`CursorState::all_selections`] order — see [`super::AutoPairInsertion`].
pub(super) fn auto_pair_char_edits(
    document: &Document,
    cursor: &CursorState,
    c: char,
    pairs: PairRules,
    scopes: &CaretScopes<'_>,
) -> (Vec<(CursorEdit, CaretPlacement)>, Vec<Option<&'static str>>) {
    cursor
        .all_selections()
        .map(|sel| {
            let local = if document.language() == Some("markdown") {
                PairRules::for_position(document, sel.head)
            } else {
                pairs
            };
            let (edit, placement, closer) = auto_pair_edit_for(document, sel, c, local, scopes);
            ((edit, placement), closer)
        })
        .unzip()
}

/// One cursor's edit for typing `c`, with the closer it writes if it writes one.
fn auto_pair_edit_for(
    document: &Document,
    sel: &Selection,
    c: char,
    pairs: PairRules,
    scopes: &CaretScopes<'_>,
) -> (CursorEdit, CaretPlacement, Option<&'static str>) {
    if !sel.is_collapsed() {
        if let Some(close) = pairs.close(c) {
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
            return (CursorEdit::replace(sel.range(), text), placement, None);
        }
        // A bare closer replaces the selection like any typed character.
        return plain_char_edit(sel, c);
    }

    if c == '>' {
        if let Some(text) = super::tag_pairs::closing_edit(document, sel.head) {
            return (
                CursorEdit::replace(sel.range(), text),
                CaretPlacement::collapsed(1),
                None,
            );
        }
    }

    // Skip over an existing closer instead of inserting a duplicate — §10.4's
    // generalisation of the single-character rule, which it contains as the
    // empty-prefix case. `"#` needs it; `"""` must not be changed by it.
    if multi_char_pairs::steps_over(pairs, document, sel.head, c) {
        let target = Position::new(sel.head.line, sel.head.column + 1);
        return (
            CursorEdit::replace(Range::new(target, target), String::new()),
            CaretPlacement::collapsed(0),
            None,
        );
    }

    // A multi-character opener whose final character is `c`, longest match —
    // Python's `"""` and `f"`, Rust's `r#"`, the shared `/*`.
    //
    // ⚠️ **Ordered here, before the apostrophe rule, and that ordering is the
    // whole of Python's payload.** `f` is a word character, so the rule that
    // keeps `don't` single is exactly what has kept the twelve prefix rows
    // dead; behind it they would stay dead. Subject to `autoclose_before` and
    // `not_in` all the same, which [`multi_char_pairs::insertion_for`] applies
    // in the same order for the same reasons. See `AUTO-PAIR-MAP.md` §10.6.
    if let Some((edit, placement, closer)) =
        multi_char_pairs::insertion_for(pairs, document, sel, c, scopes)
    {
        return (edit, placement, Some(closer));
    }

    if let Some(close) = pairs.close(c) {
        // Quotes stay single directly after a word character (don't, it's).
        if is_quote(c) && char_before(document, sel.head).is_some_and(motions::is_word_char) {
            return plain_char_edit(sel, c);
        }
        // …and no pair at all in front of a character the language did not
        // list. Typing `(` before an identifier gives `(identifier`, not
        // `()identifier`.
        if !pairs.permits_close_before(char_at(document, sel.head)) {
            return plain_char_edit(sel, c);
        }
        // …and none inside a scope the language switched this pair off in — a
        // `{` typed inside a string literal is a `{`, not the start of a block.
        //
        // ⚠️ Ordered after the two cheap tests and behind `has_any_suppression`
        // because this is the only branch here that can cost a document-sized
        // allocation and a tree-sitter query.
        if pairs.has_any_suppression(c)
            && let Some(byte) = document.position_to_offset(sel.head)
            && let Some(highlight) = scopes.scope_at(byte)
            && pairs.suppressed_in(c, highlight)
        {
            return plain_char_edit(sel, c);
        }
        let text = format!("{c}{close}");
        let placement = CaretPlacement::collapsed(c.len_utf8());
        // Not recorded: the single-character collapse keeps the buffer-based
        // rule, where the two answers are byte-identical anyway.
        return (CursorEdit::replace(sel.range(), text), placement, None);
    }

    plain_char_edit(sel, c)
}

/// The ordinary typing edit: replace the selection with `c`, caret after it.
fn plain_char_edit(sel: &Selection, c: char) -> (CursorEdit, CaretPlacement, Option<&'static str>) {
    let text = c.to_string();
    let placement = CaretPlacement::collapsed(text.len());
    (CursorEdit::replace(sel.range(), text), placement, None)
}
