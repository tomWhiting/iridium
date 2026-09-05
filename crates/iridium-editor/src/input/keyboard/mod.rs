//! Keyboard input handling.
//!
//! # Bindings are data, not branches
//!
//! A keypress is never matched against a `(KeyCode, Modifiers)` pattern here.
//! [`KeyboardHandler::handle_key`] feeds it to a [`KeymapResolver`] over a
//! [`KeymapStack`], which yields a [`CommandId`](crate::CommandId); the command
//! is then run by [`dispatch`] through the table in [`actions`]. Three
//! consequences follow, and they are the reason for the indirection:
//!
//! - **rebindable** — a host pushes a user [`Keymap`] onto the stack
//!   ([`KeyboardHandler::push_keymap`]) and every key it names changes meaning,
//!   including being unbound entirely, without touching this crate;
//! - **discoverable** — the same command ids populate a command palette and a
//!   keybinding-help view, because [`crate::commands::builtin`] describes each
//!   one;
//! - **modal-capable** — a Vim-grammar keymap is another [`Keymap`] whose
//!   bindings name modes, stacked on top of the non-modal default. The kernel
//!   grows no mode enum; see [`KeymapResolver`].
//!
//! Multi-key sequences work from the first keypress: `Ctrl+K` alone is a live
//! prefix of `Ctrl+K Ctrl+D`, so it is consumed and reported as
//! [`KeyResult::Handled`] while the sequence stays pending.
//!
//! # What the commands do
//!
//! Implementations are grouped by concern: caret motions and the sticky-column
//! machinery in [`navigation`], text/clipboard/comment/history commands in
//! [`edits`], multi-cursor verbs in [`multi_cursor_verbs`]. The per-cursor
//! primitives they build on live in [`editing`] and [`motions`] so other input
//! paths (e.g. paste handling in the editor core) can reuse them.
//!
//! All editing and navigation commands are multi-cursor aware: every cursor
//! (primary and secondary) is edited or moved independently, and cursors that
//! converge on the same position are merged.
//!
//! Configuration-dependent behaviors — tab width and spaces-vs-tabs,
//! indent/outdent, auto-indent on Enter (including bracket-block and code-fence
//! expansion), and auto-pair insertion — live in [`behaviors`] and are driven by
//! the [`EditorConfig`] passed to [`KeyboardHandler::handle_key`]. Backspace's
//! half of the auto-pair rules is [`backspace_pairs`], beside the
//! [`auto_pair_record`] it reads and no longer beside the manifest.
//!
//! Comment toggling lives in [`comments`]: the comment syntax is resolved from
//! the document's language identifier ([`Document::language`]), falling back to
//! [`EditorConfig::line_comment_token`]; when neither is available the toggle
//! commands are acknowledged without editing.

mod actions;
mod auto_pair_record;
mod backspace_pairs;
mod behaviors;
mod cell_actions;
mod cell_edits;
pub(crate) mod cell_input;
mod cell_navigation;
#[cfg(test)]
mod cell_tests;
mod comments;
mod dispatch;
pub mod editing;
mod edits;
mod handler;
mod keymap_api;
mod line_ops;
pub mod motions;
mod multi_char_pairs;
mod multi_cursor;
mod multi_cursor_verbs;
mod navigation;
mod transform;
mod types;

#[cfg(test)]
mod behavior_tests;
#[cfg(test)]
mod comment_manifest_tests;
#[cfg(test)]
mod comment_tests;
#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod line_boundary_tests;
#[cfg(test)]
mod line_ops_tests;
#[cfg(test)]
mod multi_char_backspace_tests;
#[cfg(test)]
mod multi_char_pair_tests;
#[cfg(test)]
mod multi_char_skip_tests;
#[cfg(test)]
mod multi_cursor_tests;
#[cfg(all(test, feature = "syntax"))]
mod scope_suppression_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transform_tests;

pub use handler::KeyboardHandler;
pub use types::{
    AstRequest, ClipboardOperation, CommandRunError, HistoryRequest, KeyCode, KeyEvent, KeyResult,
    Modifiers, SearchAction,
};

use crate::commands::{
    CommandArgs, KeyHintIndex, KeyPress, Keymap, KeymapError, KeymapResolver, KeymapStack,
    ModeName, default_keymap_stack,
};
use crate::document::{CursorState, Document, Selection};
use crate::editor::{CaretScopes, EditorConfig};
use crate::history::{Command, UndoTree};

/// Re-exported for the `use super::*` in the module's test files, which build
/// cursor states from raw coordinates.
#[cfg(test)]
pub(super) use crate::document::Position;
