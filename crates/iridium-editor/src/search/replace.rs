//! Replace functionality.
//!
//! This module provides replace operations that work with the search module
//! to replace matched text. All replacements are performed through commands,
//! enabling proper undo/redo support.
//!
//! # Replacement templates
//!
//! When the active search is in regex mode, the replacement string is treated
//! as an expansion template with the semantics of [`regex::Captures::expand`]:
//! `$1`/`${1}` refer to numbered capture groups, `$name`/`${name}` refer to
//! named capture groups, `$0` is the whole match, and `$$` inserts a literal
//! `$`. Group references that do not exist in the pattern expand to the empty
//! string. When the search is in literal (non-regex) mode, the replacement
//! string is inserted verbatim — a `$1` in literal mode stays `$1`.
//!
//! # Stale matches
//!
//! In regex mode every match is re-verified against the document before it is
//! replaced: the pattern must still match exactly the recorded range at the
//! recorded offset. A match that fails verification means the document changed
//! after the find (stale search state); such matches are **skipped, never
//! guessed at** — replacing unverified text would silently corrupt the
//! document. `ReplaceResult::count` reflects only the replacements actually
//! performed, and the replace functions return `None` when every candidate
//! match was stale.
//!
//! # Example
//!
//! ```
//! use iridium_editor::search::{SearchOptions, SearchState, ReplaceResult};
//! use iridium_editor::Document;
//!
//! let mut doc = Document::new("foo bar foo");
//! let mut state = SearchState::new();
//! state.find_all("foo", &SearchOptions::default(), &doc).unwrap();
//!
//! // Replace operations are done through the Editor or via commands
//! ```

use regex::{Regex, RegexBuilder};

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::history::Command;

use super::SearchState;

/// Context required to verify a regex-mode match and expand capture-group
/// references in its replacement template.
///
/// Holds the compiled search pattern together with a snapshot of the full
/// document text so that context-sensitive assertions (`^`, `$`, `\b`)
/// resolve exactly as they did during the original find, and so that every
/// match in a replace-all pass expands against the same pre-replacement text.
struct ExpansionContext {
    /// The compiled search pattern, built with the same options as the find.
    regex: Regex,
    /// Full document text captured before any replacement is applied.
    text: String,
    /// Whether matches must additionally sit on word boundaries, mirroring
    /// `SearchOptions::whole_word` (which the compiled pattern cannot encode).
    whole_word: bool,
}

/// How replacement templates are interpreted for the active search.
enum ExpansionMode {
    /// Literal (non-regex) search: templates are inserted verbatim.
    Literal,
    /// Regex search: matches are verified and templates expanded per match.
    Regex(ExpansionContext),
}

/// Determines the expansion mode for the given search state.
///
/// Returns `None` when the search claims regex mode but its pattern cannot be
/// compiled (or its query is empty despite recorded matches). That state is
/// only reachable by mutating the public `SearchState` fields after a
/// successful find — a find validates the pattern — and the safe response is
/// to refuse the whole replace operation rather than insert templates
/// verbatim against unverifiable matches.
fn expansion_mode(search_state: &SearchState, document: &Document) -> Option<ExpansionMode> {
    if !search_state.options.regex {
        return Some(ExpansionMode::Literal);
    }

    if search_state.query.is_empty() {
        return None;
    }

    let regex = RegexBuilder::new(&search_state.query)
        .case_insensitive(!search_state.options.case_sensitive)
        .build()
        .ok()?;

    Some(ExpansionMode::Regex(ExpansionContext {
        regex,
        text: document.text(),
        whole_word: search_state.options.whole_word,
    }))
}

/// Expands the replacement template for a single match, verifying the match
/// is still valid.
///
/// In literal mode the template is returned verbatim. In regex mode the
/// pattern is re-run against the full document text at the match's byte
/// offset to recover the capture groups, and the template is expanded with
/// [`regex::Captures::expand`] semantics — but only when the pattern still
/// matches *exactly* the recorded range at that offset, including the
/// whole-word boundary requirement when the search options demand it.
///
/// Returns `None` when the recorded range cannot be verified (out-of-range
/// offsets, non-character boundaries, the pattern no longer matching the
/// recorded range, or a violated whole-word boundary). That means the search
/// state is stale — the document changed after the find — and the only safe
/// behavior is to skip the match entirely. Guessing (e.g. expanding an
/// interior match, or inserting the template verbatim) would silently replace
/// text the user never matched.
fn expand_replacement(
    mode: &ExpansionMode,
    document: &Document,
    range: Range,
    replacement: &str,
) -> Option<String> {
    let ExpansionMode::Regex(context) = mode else {
        return Some(replacement.to_string());
    };

    let start = document.position_to_offset(range.start)?;
    let end = document.position_to_offset(range.end)?;

    if start > end
        || end > context.text.len()
        || !context.text.is_char_boundary(start)
        || !context.text.is_char_boundary(end)
    {
        return None;
    }

    // Re-run the pattern anchored at the match's byte offset within the full
    // document so capture groups and assertions see their original context.
    // The whole match must reproduce the recorded range exactly; anything
    // else means the search state is stale.
    let captures = context.regex.captures_at(&context.text, start)?;
    let whole = captures.get(0)?;
    if whole.start() != start || whole.end() != end {
        return None;
    }

    // The compiled pattern cannot encode the whole-word option; re-apply the
    // same boundary check the find used, against the snapshot.
    if context.whole_word && !SearchState::is_word_boundary(&context.text, start, end) {
        return None;
    }

    let mut expanded = String::with_capacity(replacement.len());
    captures.expand(replacement, &mut expanded);
    Some(expanded)
}

/// Result of a replace operation.
#[derive(Debug, Clone)]
pub struct ReplaceResult {
    /// Number of replacements made
    pub count: usize,
    /// Text that was replaced
    pub replaced_text: Vec<String>,
}

impl ReplaceResult {
    /// Creates a new replace result.
    #[must_use]
    pub const fn new(count: usize, replaced_text: Vec<String>) -> Self {
        Self {
            count,
            replaced_text,
        }
    }

    /// Creates an empty result (no replacements).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            count: 0,
            replaced_text: Vec::new(),
        }
    }
}

/// Creates a command to replace the current match with the replacement text.
///
/// This function creates a compound command that:
/// 1. Deletes the current match
/// 2. Inserts the replacement text
/// 3. Updates the cursor position
///
/// The caller is responsible for applying the command and updating the search state.
///
/// # Arguments
///
/// * `search_state` - The current search state with matches
/// * `replacement` - The replacement template; capture-group references are
///   expanded in regex mode (see the module docs), verbatim in literal mode
/// * `document` - The document (for reading the text to be replaced)
/// * `cursor` - The current cursor state
///
/// # Returns
///
/// Returns `Some(Command)` if there is a current match to replace,
/// or `None` if there are no matches or no current match.
#[must_use]
pub fn replace_current(
    search_state: &SearchState,
    replacement: &str,
    document: &Document,
    cursor: &CursorState,
) -> Option<Command> {
    let range = search_state.current_range()?;

    let mode = expansion_mode(search_state, document)?;
    let expanded = expand_replacement(&mode, document, *range, replacement)?;

    let old_text = document.slice(*range);
    let new_pos = compute_position_after_replace(range.start, &expanded);
    let new_cursor = CursorState::at(new_pos);

    let commands = vec![
        Command::Replace {
            range: *range,
            old_text,
            new_text: expanded,
        },
        Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_cursor,
        },
    ];

    Some(Command::Compound { commands })
}

/// Creates a command to replace all matches with the replacement text.
///
/// This function creates a single compound command containing all replacements,
/// which means the entire replace-all operation can be undone in one step
/// (per FR-030).
///
/// Replacements are applied from the end of the document to the beginning
/// to ensure that byte offsets remain valid throughout the operation.
///
/// # Arguments
///
/// * `search_state` - The current search state with matches
/// * `replacement` - The replacement template; capture-group references are
///   expanded per match in regex mode (see the module docs), verbatim in
///   literal mode
/// * `document` - The document (for reading the text to be replaced)
/// * `cursor` - The current cursor state
///
/// # Returns
///
/// Returns `Some((Command, ReplaceResult))` if there are matches to replace,
/// or `None` if there are no matches.
#[must_use]
pub fn replace_all(
    search_state: &SearchState,
    replacement: &str,
    document: &Document,
    cursor: &CursorState,
) -> Option<(Command, ReplaceResult)> {
    if search_state.matches.is_empty() {
        return None;
    }

    let mode = expansion_mode(search_state, document)?;

    let mut commands = Vec::new();
    let mut replaced_text = Vec::new();
    let mut earliest_replacement: Option<(Position, String)> = None;

    // Process matches in reverse order (end to start) so positions remain valid.
    // Expansion always runs against the pre-replacement document snapshot, so
    // capture offsets stay correct even as replacements change text length.
    // Matches that can no longer be verified against the document (stale
    // search state) are skipped rather than guessed at.
    for range in search_state.matches.iter().rev() {
        let Some(expanded) = expand_replacement(&mode, document, *range, replacement) else {
            continue;
        };

        // Iterating in reverse, so the last assignment is the earliest match.
        earliest_replacement = Some((range.start, expanded.clone()));

        let old_text = document.slice(*range);
        replaced_text.push(old_text.clone());

        commands.push(Command::Replace {
            range: *range,
            old_text,
            new_text: expanded,
        });
    }

    // Every match was stale: nothing to replace.
    let (first_start, first_expanded) = earliest_replacement?;

    // Reverse replaced_text to match original order
    replaced_text.reverse();

    // Move cursor to end of the earliest replacement
    let new_pos = compute_position_after_replace(first_start, &first_expanded);
    let new_cursor = CursorState::at(new_pos);

    commands.push(Command::SetSelection {
        old_state: cursor.clone(),
        new_state: new_cursor,
    });

    let count = replaced_text.len();
    let result = ReplaceResult::new(count, replaced_text);

    Some((Command::Compound { commands }, result))
}

/// Creates commands to replace a specific match by index.
///
/// # Arguments
///
/// * `search_state` - The current search state with matches
/// * `index` - The index of the match to replace
/// * `replacement` - The replacement template; capture-group references are
///   expanded in regex mode (see the module docs), verbatim in literal mode
/// * `document` - The document
/// * `cursor` - The current cursor state
///
/// # Returns
///
/// Returns `Some(Command)` if the index is valid, or `None` otherwise.
#[must_use]
pub fn replace_at_index(
    search_state: &SearchState,
    index: usize,
    replacement: &str,
    document: &Document,
    cursor: &CursorState,
) -> Option<Command> {
    let range = search_state.matches.get(index)?;

    let mode = expansion_mode(search_state, document)?;
    let expanded = expand_replacement(&mode, document, *range, replacement)?;

    let old_text = document.slice(*range);
    let new_pos = compute_position_after_replace(range.start, &expanded);
    let new_cursor = CursorState::at(new_pos);

    let commands = vec![
        Command::Replace {
            range: *range,
            old_text,
            new_text: expanded,
        },
        Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_cursor,
        },
    ];

    Some(Command::Compound { commands })
}

/// Creates a command to replace all matches in a selection with replacement text.
///
/// Only replaces matches whose ranges are fully contained within the selection.
///
/// # Arguments
///
/// * `search_state` - The current search state with matches
/// * `selection` - The selection range to restrict replacements to
/// * `replacement` - The replacement template; capture-group references are
///   expanded per match in regex mode (see the module docs), verbatim in
///   literal mode
/// * `document` - The document
/// * `cursor` - The current cursor state
///
/// # Returns
///
/// Returns `Some((Command, ReplaceResult))` if there are matches in the selection,
/// or `None` if there are no matches in the selection.
#[must_use]
pub fn replace_in_selection(
    search_state: &SearchState,
    selection: &Selection,
    replacement: &str,
    document: &Document,
    cursor: &CursorState,
) -> Option<(Command, ReplaceResult)> {
    let sel_range = selection.range();

    // Filter matches that are fully within the selection
    let matches_in_selection: Vec<&Range> = search_state
        .matches
        .iter()
        .filter(|r| r.start >= sel_range.start && r.end <= sel_range.end)
        .collect();

    if matches_in_selection.is_empty() {
        return None;
    }

    let mode = expansion_mode(search_state, document)?;

    let mut commands = Vec::new();
    let mut replaced_text = Vec::new();

    // Process matches in reverse order (end to start), skipping matches that
    // can no longer be verified against the document (stale search state).
    for range in matches_in_selection.iter().rev() {
        let Some(expanded) = expand_replacement(&mode, document, **range, replacement) else {
            continue;
        };

        let old_text = document.slice(**range);
        replaced_text.push(old_text.clone());

        commands.push(Command::Replace {
            range: **range,
            old_text,
            new_text: expanded,
        });
    }

    // Every match in the selection was stale: nothing to replace.
    if replaced_text.is_empty() {
        return None;
    }

    // Reverse replaced_text to match original order
    replaced_text.reverse();

    // Cursor stays at its current position
    commands.push(Command::SetSelection {
        old_state: cursor.clone(),
        new_state: cursor.clone(),
    });

    let count = replaced_text.len();
    let result = ReplaceResult::new(count, replaced_text);

    Some((Command::Compound { commands }, result))
}

/// Computes the cursor position after a replacement.
fn compute_position_after_replace(start: Position, replacement: &str) -> Position {
    let mut line = start.line;
    let mut column = start.column;

    for ch in replacement.chars() {
        if ch == '\n' {
            line += 1;
            column = 0;
        } else if ch == '\r' {
            // Skip CR in CRLF
        } else {
            column += 1;
        }
    }

    Position::new(line, column)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::search::SearchOptions;

    fn setup_search(doc: &Document, query: &str) -> SearchState {
        let mut state = SearchState::new();
        state
            .find_all(query, &SearchOptions::default(), doc)
            .unwrap();
        state
    }

    fn setup_regex_search(doc: &Document, pattern: &str) -> SearchState {
        let mut state = SearchState::new();
        state
            .find_all(pattern, &SearchOptions::regex_mode(), doc)
            .unwrap();
        state
    }

    fn apply_to(doc: &Document, cursor: &CursorState, cmd: &Command) -> (Document, CursorState) {
        let mut new_doc = doc.clone();
        let mut new_cursor = cursor.clone();
        cmd.apply(&mut new_doc, &mut new_cursor).unwrap();
        (new_doc, new_cursor)
    }

    #[test]
    fn replace_current_basic() {
        let doc = Document::new("foo bar");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_current(&search, "baz", &doc, &cursor);
        assert!(cmd.is_some());

        let cmd = cmd.unwrap();
        if let Command::Compound { commands } = cmd {
            assert_eq!(commands.len(), 2);
            // First should be Replace
            assert!(matches!(&commands[0], Command::Replace { .. }));
        } else {
            panic!("Expected Compound command");
        }
    }

    #[test]
    fn replace_current_no_matches() {
        let doc = Document::new("foo bar");
        let search = setup_search(&doc, "xyz");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_current(&search, "baz", &doc, &cursor);
        assert!(cmd.is_none());
    }

    #[test]
    fn replace_all_basic() {
        let doc = Document::new("foo bar foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let result = replace_all(&search, "baz", &doc, &cursor);
        assert!(result.is_some());

        let (cmd, replace_result) = result.unwrap();
        assert_eq!(replace_result.count, 2);
        assert_eq!(replace_result.replaced_text.len(), 2);

        if let Command::Compound { commands } = cmd {
            // 2 Replace commands + 1 SetSelection
            assert_eq!(commands.len(), 3);
        } else {
            panic!("Expected Compound command");
        }
    }

    #[test]
    fn replace_all_no_matches() {
        let doc = Document::new("foo bar");
        let search = setup_search(&doc, "xyz");
        let cursor = CursorState::at(Position::zero());

        let result = replace_all(&search, "baz", &doc, &cursor);
        assert!(result.is_none());
    }

    #[test]
    fn replace_all_single_undo() {
        // Verify that replace_all creates a single compound command
        let doc = Document::new("a a a");
        let search = setup_search(&doc, "a");
        let cursor = CursorState::at(Position::zero());

        let (cmd, _) = replace_all(&search, "b", &doc, &cursor).unwrap();

        // The command should be a single Compound, not nested
        assert!(matches!(cmd, Command::Compound { .. }));

        // Apply the command
        let mut new_doc = doc;
        let mut new_cursor = cursor;
        cmd.apply(&mut new_doc, &mut new_cursor).unwrap();

        assert_eq!(new_doc.text(), "b b b");

        // Apply inverse (undo)
        cmd.inverse().apply(&mut new_doc, &mut new_cursor).unwrap();
        assert_eq!(new_doc.text(), "a a a");
    }

    #[test]
    fn replace_at_index_valid() {
        let doc = Document::new("foo bar foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_at_index(&search, 1, "baz", &doc, &cursor);
        assert!(cmd.is_some());
    }

    #[test]
    fn replace_at_index_invalid() {
        let doc = Document::new("foo bar foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_at_index(&search, 10, "baz", &doc, &cursor);
        assert!(cmd.is_none());
    }

    #[test]
    fn replace_in_selection_filters_correctly() {
        let doc = Document::new("foo bar foo baz foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        // Select only the middle part: "bar foo baz"
        let selection = Selection::new(Position::new(0, 4), Position::new(0, 15));

        let result = replace_in_selection(&search, &selection, "xxx", &doc, &cursor);
        assert!(result.is_some());

        let (_, replace_result) = result.unwrap();
        // Only the middle "foo" should be replaced
        assert_eq!(replace_result.count, 1);
    }

    #[test]
    fn replace_preserves_order() {
        // Verify replacements happen in correct order for undo
        let doc = Document::new("aaa");
        let search = setup_search(&doc, "a");
        let cursor = CursorState::at(Position::zero());

        let (cmd, result) = replace_all(&search, "bb", &doc, &cursor).unwrap();

        assert_eq!(result.count, 3);
        assert_eq!(result.replaced_text, vec!["a", "a", "a"]);

        // Apply
        let mut new_doc = doc;
        let mut new_cursor = cursor;
        cmd.apply(&mut new_doc, &mut new_cursor).unwrap();
        assert_eq!(new_doc.text(), "bbbbbb");
    }

    #[test]
    fn replace_all_regex_capture_groups_swap() {
        let doc = Document::new("one-two three-four five-six");
        let search = setup_regex_search(&doc, r"(\w+)-(\w+)");
        let cursor = CursorState::at(Position::zero());

        let (cmd, result) = replace_all(&search, "$2-$1", &doc, &cursor).unwrap();
        assert_eq!(result.count, 3);
        assert_eq!(
            result.replaced_text,
            vec!["one-two", "three-four", "five-six"]
        );

        let (new_doc, _) = apply_to(&doc, &cursor, &cmd);
        assert_eq!(new_doc.text(), "two-one four-three six-five");
    }

    #[test]
    fn replace_regex_dollar_escaping() {
        let doc = Document::new("price 42 and 7");
        let search = setup_regex_search(&doc, r"(\d+)");
        let cursor = CursorState::at(Position::zero());

        let (cmd, _) = replace_all(&search, "$$$1", &doc, &cursor).unwrap();

        let (new_doc, _) = apply_to(&doc, &cursor, &cmd);
        assert_eq!(new_doc.text(), "price $42 and $7");
    }

    #[test]
    fn replace_regex_named_group() {
        let doc = Document::new("x xx y xxx");
        let search = setup_regex_search(&doc, r"(?P<a>x+)");
        let cursor = CursorState::at(Position::zero());

        let (cmd, result) = replace_all(&search, "[$a]", &doc, &cursor).unwrap();
        assert_eq!(result.count, 3);

        let (new_doc, _) = apply_to(&doc, &cursor, &cmd);
        assert_eq!(new_doc.text(), "[x] [xx] y [xxx]");
    }

    #[test]
    fn replace_literal_mode_keeps_dollar_verbatim() {
        let doc = Document::new("foo bar foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let (cmd, _) = replace_all(&search, "$1", &doc, &cursor).unwrap();

        let (new_doc, _) = apply_to(&doc, &cursor, &cmd);
        assert_eq!(new_doc.text(), "$1 bar $1");
    }

    #[test]
    fn replace_all_regex_undo_restores_original() {
        let original = "one-two three-four five-six";
        let doc = Document::new(original);
        let search = setup_regex_search(&doc, r"(\w+)-(\w+)");
        let cursor = CursorState::at(Position::zero());

        let (cmd, _) = replace_all(&search, "$2-$1", &doc, &cursor).unwrap();

        // Replace-all must remain a single undoable compound command
        assert!(matches!(cmd, Command::Compound { .. }));

        let (mut new_doc, mut new_cursor) = apply_to(&doc, &cursor, &cmd);
        assert_eq!(new_doc.text(), "two-one four-three six-five");

        cmd.inverse().apply(&mut new_doc, &mut new_cursor).unwrap();
        assert_eq!(new_doc.text(), original);
    }

    #[test]
    fn replace_current_regex_expands_captures() {
        let doc = Document::new("one-two three-four");
        let mut search = setup_regex_search(&doc, r"(\w+)-(\w+)");
        search.next_match();

        let cursor = CursorState::at(Position::zero());
        let cmd = replace_current(&search, "$2-$1", &doc, &cursor).unwrap();

        let (new_doc, _) = apply_to(&doc, &cursor, &cmd);
        assert_eq!(new_doc.text(), "one-two four-three");
    }

    #[test]
    fn replace_at_index_regex_expands_captures() {
        let doc = Document::new("one-two three-four");
        let search = setup_regex_search(&doc, r"(\w+)-(\w+)");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_at_index(&search, 0, "$2-$1", &doc, &cursor).unwrap();

        let (new_doc, _) = apply_to(&doc, &cursor, &cmd);
        assert_eq!(new_doc.text(), "two-one three-four");
    }

    #[test]
    fn replace_in_selection_regex_expands_captures() {
        let doc = Document::new("one-two three-four five-six");
        let search = setup_regex_search(&doc, r"(\w+)-(\w+)");
        let cursor = CursorState::at(Position::zero());

        // Select only the middle match: "three-four"
        let selection = Selection::new(Position::new(0, 8), Position::new(0, 18));

        let (cmd, result) =
            replace_in_selection(&search, &selection, "$2-$1", &doc, &cursor).unwrap();
        assert_eq!(result.count, 1);

        let (new_doc, _) = apply_to(&doc, &cursor, &cmd);
        assert_eq!(new_doc.text(), "one-two four-three five-six");
    }

    #[test]
    fn replace_all_regex_missing_group_expands_empty() {
        let doc = Document::new("ab ab");
        let search = setup_regex_search(&doc, r"(a)(b)");
        let cursor = CursorState::at(Position::zero());

        let (cmd, _) = replace_all(&search, "$1$9", &doc, &cursor).unwrap();

        let (new_doc, _) = apply_to(&doc, &cursor, &cmd);
        assert_eq!(new_doc.text(), "a a");
    }

    #[test]
    fn multiline_replacement() {
        let doc = Document::new("foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_current(&search, "bar\nbaz", &doc, &cursor).unwrap();

        let mut new_doc = doc;
        let mut new_cursor = cursor;
        cmd.apply(&mut new_doc, &mut new_cursor).unwrap();

        assert_eq!(new_doc.text(), "bar\nbaz");
        assert_eq!(new_doc.line_count(), 2);

        // Cursor should be at end of replacement
        assert_eq!(new_cursor.primary.head, Position::new(1, 3));
    }

    /// Regression test (Norn review, 2026-07-12): a stale regex match must be
    /// skipped, not replaced. Previously a fallback matched the pattern
    /// anywhere *inside* the recorded slice and used those captures to
    /// replace the whole recorded range: searching `a+` in "aaa" (recording
    /// 0..3), editing the document to "xaa", then replacing with `$0`
    /// expanded the interior match "aa" and replaced all of 0..3 with it —
    /// silently deleting the "x".
    #[test]
    fn replace_all_skips_stale_regex_matches() {
        let doc = Document::new("aaa");
        let search = setup_regex_search(&doc, "a+");

        // Document changes after the find; the recorded 0..3 match is stale.
        let edited = Document::new("xaa");
        let cursor = CursorState::at(Position::zero());

        // The stale match fails verification, so there is nothing to replace.
        assert!(replace_all(&search, "$0", &edited, &cursor).is_none());
        assert!(replace_current(&search, "$0", &edited, &cursor).is_none());
        assert!(replace_at_index(&search, 0, "$0", &edited, &cursor).is_none());
    }

    /// Regression test (Norn follow-up, 2026-07-12): whole-word validity must
    /// be re-verified at replace time. The compiled pattern cannot encode the
    /// whole-word option, so a stale range can reproduce the exact regex match
    /// while no longer sitting on a word boundary: search `foo` (regex +
    /// whole-word) in "foo", edit to "foox", and 0..3 still matches `foo`
    /// exactly — but it is no longer a whole word and must be skipped.
    #[test]
    fn replace_skips_matches_that_lost_whole_word_boundaries() {
        let doc = Document::new("foo");
        let mut search = SearchState::new();
        let options = SearchOptions {
            regex: true,
            whole_word: true,
            ..SearchOptions::default()
        };
        search.find_all("foo", &options, &doc).unwrap();
        assert_eq!(search.matches.len(), 1);

        let edited = Document::new("foox");
        let cursor = CursorState::at(Position::zero());

        assert!(replace_all(&search, "bar", &edited, &cursor).is_none());
        assert!(replace_current(&search, "bar", &edited, &cursor).is_none());
    }

    /// Regression test (Norn follow-up, 2026-07-12): an uncompilable pattern
    /// in regex mode must refuse the whole replace operation, not fall back
    /// to verbatim insertion against unverifiable matches. Reachable only by
    /// mutating the public `SearchState` fields after a successful find.
    #[test]
    fn replace_refuses_invalid_regex_state() {
        let doc = Document::new("aaa");
        let mut search = setup_regex_search(&doc, "a+");
        let cursor = CursorState::at(Position::zero());

        search.query = "[".to_string();
        assert!(replace_all(&search, "$0", &doc, &cursor).is_none());
        assert!(replace_current(&search, "$0", &doc, &cursor).is_none());
        assert!(replace_at_index(&search, 0, "$0", &doc, &cursor).is_none());

        search.query = String::new();
        assert!(replace_all(&search, "$0", &doc, &cursor).is_none());
    }

    /// Stale and still-valid regex matches can coexist: only the verified
    /// matches are replaced, and the count reflects that.
    #[test]
    fn replace_all_partial_staleness_replaces_only_verified_matches() {
        let doc = Document::new("a b a");
        let search = setup_regex_search(&doc, "a");

        // Editing the first character invalidates the 0..1 match; the 4..5
        // match still verifies against the new document.
        let edited = Document::new("x b a");
        let cursor = CursorState::at(Position::zero());

        let (cmd, result) = replace_all(&search, "Z", &edited, &cursor).unwrap();
        assert_eq!(result.count, 1);

        let (new_doc, _) = apply_to(&edited, &cursor, &cmd);
        assert_eq!(new_doc.text(), "x b Z");
    }
}
