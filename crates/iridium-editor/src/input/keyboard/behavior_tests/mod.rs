//! Document-state-level tests for the configuration-driven editing
//! behaviors: Tab/indent/outdent, auto-indent on Enter (including
//! bracket-block and code-fence expansion), and auto-closing pairs.
//!
//! Every test applies the produced command to a real document and asserts
//! both the resulting text and every cursor position.
//!
//! # One file per behaviour
//!
//! ⚠️ This was a single 1,690-line file against a 1,000-line hard limit (#92).
//! The seams it is split on are the ones it already had — the
//! `// ========== … ==========` rules the author wrote — so no test moved
//! between neighbours and the suite reads in the same order it always did.
//!
//! What stays here is what more than one of them needs: the harness, the two
//! document builders, and the four chords. [`press`], [`ch`] and [`doc_in`] are
//! `pub(super)` because three *sibling* modules import them —
//! `multi_char_pair_tests`, `multi_char_skip_tests` and
//! `multi_char_backspace_tests` — which is why they cannot live in one of the
//! files below.
//!
//! # ⛔ A file missing from the list above is silently ignored
//!
//! Measured, not assumed: commenting out one `mod` line here left the suite
//! **green** — 1,368 passed instead of 1,376, with eight tests simply gone and
//! nothing failing. `cargo` does not read a directory; it reads this list, so
//! an orphaned `.rs` beside it compiles clean and takes its assertions with it.
//! Neither the test gate nor clippy nor `fmt` says a word.
//!
//! So a green suite is not evidence that a split preserved anything. The only
//! witness is a count: `cargo test -p iridium-editor --lib -- --list`, compared
//! against the same count taken before the move. Take it whenever a file here
//! is added, removed or renamed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod auto_indent;
mod auto_pairs;
mod autoclose_before;
mod integration;
mod outdent;
mod tab;

use iridium_lang::Language;

use super::tests::{cursors_at, cursors_with, heads};
use super::*;
use crate::editor::CaretScopes;
use crate::editor::Editor;

/// All cursor selections in `all_selections` order, as
/// `((anchor.line, anchor.column), (head.line, head.column))` pairs.
fn selections(cursor: &CursorState) -> Vec<((usize, usize), (usize, usize))> {
    cursor
        .all_selections()
        .map(|sel| {
            (
                (sel.anchor.line, sel.anchor.column),
                (sel.head.line, sel.head.column),
            )
        })
        .collect()
}

/// Handles a key event with the given config and applies the resulting
/// command (if any), returning the command for undo assertions.
///
/// ⚠️ `CaretScopes::none()` resolves nothing, so the `not_in` branch of
/// `auto_pair_edit_for` is unreachable on this harness for every character in
/// every language. `multi_char_pair_tests` shares this helper and depends on
/// that; see its module docs.
pub(super) fn press(
    handler: &mut KeyboardHandler,
    event: &KeyEvent,
    document: &mut Document,
    cursor: &mut CursorState,
    config: &EditorConfig,
) -> Option<Command> {
    let result = handler.handle_key(
        event,
        document,
        cursor,
        &UndoTree::new(),
        config,
        &CaretScopes::none(),
    );
    match result {
        KeyResult::Command(cmd) => {
            cmd.apply(document, cursor).expect("command must apply");
            Some(cmd)
        },
        KeyResult::Handled => None,
        other => panic!("expected a command or Handled, got {other:?}"),
    }
}

/// Config with spaces disabled (literal tab characters).
fn tabs_config() -> EditorConfig {
    EditorConfig {
        insert_spaces: false,
        ..EditorConfig::default()
    }
}

const TAB: KeyEvent = KeyEvent::new(KeyCode::Tab, Modifiers::none());
const SHIFT_TAB: KeyEvent = KeyEvent::new(KeyCode::Tab, Modifiers::shift());
const ENTER: KeyEvent = KeyEvent::new(KeyCode::Enter, Modifiers::none());
const BACKSPACE: KeyEvent = KeyEvent::new(KeyCode::Backspace, Modifiers::none());

pub(super) fn ch(c: char) -> KeyEvent {
    KeyEvent::simple(KeyCode::Char(c))
}

/// A document holding `text` and tagged with `language`'s identifier, the way
/// a face tags one after resolving a file's extension.
pub(super) fn doc_in(language: Language, text: &str) -> Document {
    let mut doc = Document::new(text);
    doc.set_language(Some(language.id().to_owned()));
    doc
}
