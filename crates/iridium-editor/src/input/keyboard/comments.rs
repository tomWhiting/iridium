//! Comment toggling: the per-language comment-syntax table and the edit
//! builders behind toggle-line-comment (Ctrl+/) and toggle-block-comment
//! (Shift+Alt+A).
//!
//! The comment syntax is resolved from the document's language identifier
//! ([`Document::language`], matched via `Language::from_id`), falling back to
//! [`EditorConfig::line_comment_token`] when the language is unknown or has
//! no comment syntax of its own. When neither source provides a token the
//! toggles are no-ops.
//!
//! Semantics follow VS Code:
//!
//! - **Line toggle**: each cursor's touched lines (selection block or caret
//!   line) form a group; groups sharing any line are merged into one
//!   connected component so the toggle decision never depends on cursor
//!   order. If every non-blank line of a component is already commented, all
//!   are uncommented; otherwise every non-blank line is commented at the
//!   component's minimum indentation. Blank lines are skipped. Languages
//!   with only a block pair (CSS, Markdown) wrap each line in the pair
//!   instead.
//! - **Block toggle**: each selection is wrapped in the block pair, or
//!   unwrapped when it is exactly wrapped already (tolerating surrounding
//!   whitespace and one space of padding inside the markers). A collapsed
//!   cursor inserts an empty pair with the caret inside. Languages without a
//!   block pair fall back to the line toggle.
//!
//! Every function returns plain edit intents; the reversible multi-cursor
//! command is built by [`super::editing`] (`build_line_edits_command` for the
//! line toggle, `build_multi_cursor_command_placed` for the block toggle), so
//! each toggle is a single undoable step restoring text and all cursors.

use std::ops::RangeInclusive;

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::editor::EditorConfig;

use iridium_lang::Language;

use super::editing::{CaretPlacement, CursorEdit};

// ========== Comment-syntax table ==========

/// The comment tokens available for a document: an optional line-comment
/// token and an optional block-comment pair.
///
/// At least one of the two is always present in a resolved instance (see
/// [`resolve_comment_syntax`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommentSyntax {
    /// Line-comment token (e.g. `//`, `#`), without trailing space.
    pub line: Option<String>,
    /// Block-comment pair (e.g. `/*` and `*/`), without inner padding.
    pub block: Option<(String, String)>,
}

/// Resolves the comment syntax for `document`.
///
/// The document's language identifier wins when it names a language whose
/// vendored manifest gives it comment syntax. Otherwise — unknown language, no
/// language, or a language whose manifest declares neither token — the
/// configuration's `line_comment_token` is used as a bare line token (trimmed;
/// ignored when empty or containing line breaks, which could never form a valid
/// line comment). Returns `None` when no comment syntax is available at all, in
/// which case toggling must be a no-op.
///
/// # Where the tokens come from
///
/// `Manifest::line_comment` and `Manifest::block_comment`, read from the
/// `config.toml` vendored beside each language's tree-sitter queries. This
/// replaced a hand-written `match` over the thirteen languages; the two were
/// asserted to agree, language by language, before the match was deleted (see
/// `comment_manifest_tests`).
///
/// Markdown and CSS have no line token; their line toggle wraps each line in
/// the block pair instead. No language currently reaches the third case — every
/// one of the thirteen declares at least one token — but the branch is real:
/// the manifests describe languages that declare neither, and the registry step
/// brings them in.
///
/// # Not gated on the `syntax` feature, and it used to be
///
/// Which token comments a language is *pure data*, parsing nothing. Gating it
/// meant the stub build answered "no comment syntax" for every language, so
/// `Ctrl+/` on a Rust file inserted nothing — or silently fell back to
/// [`EditorConfig::line_comment_token`], which is worse, because a `#` in a
/// Rust file looks like a decision somebody made.
///
/// The gate survived because a comment on the stub asserted the path was
/// unreachable — "`Language::from_id` always returns `None`" — which was simply
/// untrue: the stub's `from_id` resolves every canonical id. Nothing caught it
/// because the language tests were gated on the same feature, so the
/// configuration that was broken was the one nothing exercised. That history is
/// why the manifests were moved into `iridium-lang`, which is always compiled:
/// reading them from the optional crate would have rebuilt the same defect.
pub(super) fn resolve_comment_syntax(
    document: &Document,
    config: &EditorConfig,
) -> Option<CommentSyntax> {
    if let Some(manifest) = document
        .language()
        .and_then(Language::from_id)
        .and_then(Language::manifest)
    {
        let line = manifest.line_comment();
        let block = manifest.block_comment();
        if line.is_some() || block.is_some() {
            return Some(CommentSyntax {
                line: line.map(str::to_owned),
                block: block.map(|(open, close)| (open.to_owned(), close.to_owned())),
            });
        }
    }

    let token = config.line_comment_token.as_deref()?.trim();
    if token.is_empty() || token.contains(['\n', '\r']) {
        return None;
    }
    Some(CommentSyntax {
        line: Some(token.to_owned()),
        block: None,
    })
}

// ========== Line helpers ==========

/// Number of leading whitespace characters (columns) on a line.
fn leading_ws_chars(text: &str) -> usize {
    text.chars().take_while(|c| c.is_whitespace()).count()
}

/// The block of lines touched by one selection: the caret line for a
/// collapsed cursor, otherwise every line the selection covers. A multi-line
/// selection ending at column 0 does not touch its final line (matching
/// indent/outdent semantics: the line under a trailing line-wise caret is
/// not part of the selected block).
fn selection_line_block(document: &Document, sel: &Selection) -> RangeInclusive<usize> {
    let last_line = document.line_count().saturating_sub(1);
    let start = sel.start();
    let end = sel.end();
    let mut last = end.line;
    if !sel.is_collapsed() && end.line > start.line && end.column == 0 {
        last -= 1;
    }
    let first = start.line.min(last_line);
    first..=last.min(last_line).max(first)
}

// ========== Toggle line comment ==========

/// Builds the line-toggle edits for every cursor.
///
/// Each cursor contributes its touched lines; blocks sharing any line are
/// merged into connected components (see [`merged_line_components`]), and
/// each component toggles independently, VS Code style:
///
/// - A component whose non-blank lines are **all** commented is uncommented:
///   the token (plus one following space, when present) is removed from each
///   line. Token-without-space is tolerated.
/// - Any other component is commented: every non-blank line gets `token +
///   " "` inserted at the component's minimum indentation
///   (already-commented lines in a mixed component are commented again,
///   exactly like VS Code).
/// - Blank lines never receive or lose a token; a component with only blank
///   lines contributes nothing.
///
/// Languages with only a block pair (CSS, Markdown) use the same component
/// decision but wrap/unwrap each line in the pair (see
/// [`block_line_group_edits`]).
///
/// Because components are disjoint and each line is decided exactly once,
/// the result is independent of which cursor is primary, and the edits are
/// non-overlapping as required by
/// [`super::editing::build_line_edits_command`], which remaps every cursor
/// and selection endpoint through the edits. An empty return (no comment
/// syntax applicable, all lines blank) yields no command.
pub(super) fn toggle_line_comment_edits(
    document: &Document,
    cursor: &CursorState,
    syntax: &CommentSyntax,
) -> Vec<CursorEdit> {
    let mut edits: Vec<CursorEdit> = Vec::new();

    for component in merged_line_components(document, cursor) {
        let group: Vec<(usize, String)> = component
            .filter_map(|line| document.line(line).map(|text| (line, text)))
            .filter(|(_, text)| !text.trim().is_empty())
            .collect();
        if group.is_empty() {
            continue;
        }

        match (&syntax.line, &syntax.block) {
            (Some(token), _) => {
                line_token_group_edits(&group, token, &mut edits);
            },
            (None, Some((open, close))) => {
                block_line_group_edits(&group, open, close, &mut edits);
            },
            (None, None) => {},
        }
    }

    edits
}

/// Merges every cursor's touched-line block into connected components:
/// blocks sharing at least one line collapse into a single component
/// (blocks that are merely adjacent stay separate). The components are
/// returned in document order.
///
/// Deciding the toggle direction per component — rather than per cursor —
/// makes the result independent of `all_selections` order: two cursors
/// whose line ranges overlap always see the same combined group, no matter
/// which of them is primary.
fn merged_line_components(document: &Document, cursor: &CursorState) -> Vec<RangeInclusive<usize>> {
    let mut blocks: Vec<RangeInclusive<usize>> = cursor
        .all_selections()
        .map(|sel| selection_line_block(document, sel))
        .collect();
    blocks.sort_by_key(|block| (*block.start(), *block.end()));

    let mut components: Vec<RangeInclusive<usize>> = Vec::with_capacity(blocks.len());
    for block in blocks {
        match components.last_mut() {
            // Overlap (shared line) merges; adjacency does not.
            Some(prev) if *block.start() <= *prev.end() => {
                if *block.end() > *prev.end() {
                    *prev = *prev.start()..=*block.end();
                }
            },
            _ => components.push(block),
        }
    }
    components
}

/// Emits one component's toggle edits with a line-comment token.
///
/// `group` holds the component's non-blank lines with their text; lines are
/// unique within a component and components are disjoint, so every line is
/// edited at most once.
fn line_token_group_edits(group: &[(usize, String)], token: &str, edits: &mut Vec<CursorEdit>) {
    let token_chars = token.chars().count();
    let all_commented = group
        .iter()
        .all(|(_, text)| text.trim_start().starts_with(token));

    if all_commented {
        for (line, text) in group {
            let ws = leading_ws_chars(text);
            let mut end = ws + token_chars;
            // Remove the canonical single space after the token when present;
            // a bare token (no space) is removed as-is.
            if text.chars().nth(end) == Some(' ') {
                end += 1;
            }
            edits.push(CursorEdit::delete(Range::new(
                Position::new(*line, ws),
                Position::new(*line, end),
            )));
        }
        return;
    }

    // Comment every non-blank line at the component's minimum indentation.
    let min_col = group
        .iter()
        .map(|(_, text)| leading_ws_chars(text))
        .min()
        .unwrap_or(0);
    for (line, _) in group {
        let at = Position::new(*line, min_col);
        edits.push(CursorEdit::replace(Range::new(at, at), format!("{token} ")));
    }
}

/// Emits one component's toggle edits for a block-pair-only language (CSS,
/// Markdown): each line is wrapped in the pair, or unwrapped when every
/// non-blank line of the component is already wrapped.
fn block_line_group_edits(
    group: &[(usize, String)],
    open: &str,
    close: &str,
    edits: &mut Vec<CursorEdit>,
) {
    let all_wrapped = group
        .iter()
        .all(|(_, text)| is_block_wrapped(text.trim(), open, close));

    if all_wrapped {
        for (line, text) in group {
            unwrap_line_edits(*line, text, open, close, edits);
        }
        return;
    }

    let min_col = group
        .iter()
        .map(|(_, text)| leading_ws_chars(text))
        .min()
        .unwrap_or(0);
    for (line, text) in group {
        let at = Position::new(*line, min_col);
        edits.push(CursorEdit::replace(Range::new(at, at), format!("{open} ")));
        // The closing marker is inserted at the end of the line's content;
        // endpoints sitting there belong to that content and must stay
        // BEFORE the marker (inside the comment), not jump past it.
        let eol = Position::new(*line, text.chars().count());
        edits.push(CursorEdit::insert_before(eol, format!(" {close}")));
    }
}

/// Returns true when `trimmed` (a line or selection with surrounding
/// whitespace removed) is exactly wrapped in the block pair: it starts with
/// `open`, ends with `close`, and is long enough that the two markers do not
/// overlap.
fn is_block_wrapped(trimmed: &str, open: &str, close: &str) -> bool {
    trimmed.len() >= open.len() + close.len()
        && trimmed.starts_with(open)
        && trimmed.ends_with(close)
}

/// Emits the two deletions that unwrap one block-wrapped line: the opening
/// marker (plus one following padding space, when present) and the closing
/// marker (plus one preceding padding space). Leading and trailing
/// whitespace outside the markers is preserved.
fn unwrap_line_edits(
    line: usize,
    text: &str,
    open: &str,
    close: &str,
    edits: &mut Vec<CursorEdit>,
) {
    let total = text.chars().count();
    let ws = leading_ws_chars(text);
    let trailing_ws = text.chars().rev().take_while(|c| c.is_whitespace()).count();

    let mut open_end = ws + open.chars().count();
    let close_end = total - trailing_ws;
    let mut close_start = close_end.saturating_sub(close.chars().count());

    // Tolerate one space of padding inside each marker, never consuming the
    // same character twice (`/* */` keeps its single shared space on the
    // opening side).
    if open_end < close_start && text.chars().nth(open_end) == Some(' ') {
        open_end += 1;
    }
    if close_start > open_end && text.chars().nth(close_start - 1) == Some(' ') {
        close_start -= 1;
    }

    edits.push(CursorEdit::delete(Range::new(
        Position::new(line, ws),
        Position::new(line, open_end),
    )));
    edits.push(CursorEdit::delete(Range::new(
        Position::new(line, close_start.max(open_end)),
        Position::new(line, close_end.max(open_end)),
    )));
}

// ========== Toggle block comment ==========

/// Builds one block-toggle edit per cursor (one entry per cursor in
/// `all_selections` order, as required by
/// [`super::editing::build_multi_cursor_command_placed`]).
///
/// Each cursor independently:
///
/// - **Unwraps** its selection when the selected text is exactly wrapped in
///   the pair (tolerating surrounding whitespace, which is preserved, and
///   one space of padding inside each marker, which is removed). The
///   selection keeps covering the unwrapped text with its orientation.
/// - **Wraps** any other non-collapsed selection as `open + " " + text +
///   " " + close`, with the selection covering the whole wrapped text (so
///   toggling again unwraps it — an exact roundtrip).
/// - **Inserts an empty pair** (`open + "  " + close`) at a collapsed
///   cursor, with the caret centered between the markers.
pub(super) fn toggle_block_comment_edits(
    document: &Document,
    cursor: &CursorState,
    open: &str,
    close: &str,
) -> Vec<(CursorEdit, CaretPlacement)> {
    cursor
        .all_selections()
        .map(|sel| block_toggle_edit_for(document, sel, open, close))
        .collect()
}

/// Computes a single cursor's block-toggle edit.
fn block_toggle_edit_for(
    document: &Document,
    sel: &Selection,
    open: &str,
    close: &str,
) -> (CursorEdit, CaretPlacement) {
    let text = document.slice(sel.range());

    if let Some(unwrapped) = unwrap_block_text(&text, open, close) {
        let placement = if sel.is_backward() {
            CaretPlacement::selection(unwrapped.len(), 0)
        } else {
            CaretPlacement::selection(0, unwrapped.len())
        };
        return (CursorEdit::replace(sel.range(), unwrapped), placement);
    }

    if sel.is_collapsed() {
        let inserted = format!("{open}  {close}");
        let caret = open.len() + 1;
        return (
            CursorEdit::replace(sel.range(), inserted),
            CaretPlacement::collapsed(caret),
        );
    }

    let wrapped = format!("{open} {text} {close}");
    let placement = if sel.is_backward() {
        CaretPlacement::selection(wrapped.len(), 0)
    } else {
        CaretPlacement::selection(0, wrapped.len())
    };
    (CursorEdit::replace(sel.range(), wrapped), placement)
}

/// When `text` is exactly wrapped in the block pair (tolerating surrounding
/// whitespace and one space of padding inside each marker), returns the
/// unwrapped replacement text (surrounding whitespace preserved, markers and
/// padding removed). Returns `None` when the text is not wrapped.
fn unwrap_block_text(text: &str, open: &str, close: &str) -> Option<String> {
    let core = text.trim();
    if !is_block_wrapped(core, open, close) {
        return None;
    }

    let inner = &core[open.len()..core.len() - close.len()];
    let inner = inner.strip_prefix(' ').unwrap_or(inner);
    let inner = inner.strip_suffix(' ').unwrap_or(inner);

    let lead = &text[..text.len() - text.trim_start().len()];
    let trail = &text[text.trim_end().len()..];
    let mut out = String::with_capacity(lead.len() + inner.len() + trail.len());
    out.push_str(lead);
    out.push_str(inner);
    out.push_str(trail);
    Some(out)
}
