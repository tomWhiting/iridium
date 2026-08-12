//! The undo-tree panel: every branch of the history, reachable.
//!
//! This is the desktop face of the kernel's `history.togglePanel` host
//! command, and it keeps the terminal face's semantics exactly; the
//! terminal's test suite is the specification.
//!
//! # The tree is the kernel's and none of it is repeated here
//!
//! Everything shown comes from one call — `Editor::history_snapshot` — and
//! so does the panel's *shape*: the linearized rows and the selection
//! browsing them are [`tree_view`](iridium_editor::history::tree_view), the
//! same module the terminal face draws from, so the two panels cannot drift.
//! Jumping is `Editor::jump_to_history_node`, the kernel's own multi-edge
//! replay. What is here is a key map and rows for [`crate::overlay`] to paint.
//!
//! # ⭐ The keys are data, and that is what #117 changed
//!
//! Every chord below used to be an arm of a `match (Chord, KeyCode)`, so none
//! of them could be rebound and none appeared in the generated `[keys]` block.
//! They are now a [`Keymap`](iridium_editor::Keymap) — [`keymap::default_keymap`]
//! — resolved through the kernel's own stack, with the user's `[keys]` layer
//! pushed on top of it.
//!
//! - [`keymap`] — the default bindings, and why shift is `Any` on all of them.
//! - [`verb`] — the eight things the panel can be asked to do, one per kernel
//!   command id, so a verb the kernel names and this panel forgot is a compile
//!   error.
//! - [`resolve`] — turning a keypress into a verb, and the filtering that keeps
//!   a user's document bindings out of this panel's stack.
//! - [`keys`] — what each verb does.
//! - [`panel`] — the state and the composition that turns it into rows.
//!
//! # The panel is modal
//!
//! Every key is consumed while it is open. Unlike the palette this panel has no
//! field, so an unclaimed key is simply swallowed — there is nothing for it to
//! fall through *to*.
//!
//! | Key | Action |
//! |---|---|
//! | `Up` / `Down` | move the selection, clamping at the ends |
//! | `PageUp` / `PageDown` | hop by one windowful |
//! | `Home` / `End` | the root / the newest row |
//! | `Enter` | jump the document to the selected state — the panel stays open |
//! | `Escape`, `Ctrl+Alt+H`, `⌘⌥H` | close |
//!
//! ⚠️ **This table is the default, not the law.** It is what
//! [`keymap::default_keymap`] binds, and a user's `[keys]` layer can say
//! otherwise — which is the whole point of the conversion.
//!
//! `Enter` deliberately keeps the panel open: hopping between two states and
//! watching the document change underneath is what a tree is *for*, and the
//! recomposed panel shows the `*` moving. `Escape` is how you put it away.

mod keymap;
mod keys;
mod panel;
mod resolve;
mod verb;

#[cfg(test)]
mod keymap_tests;

#[cfg(test)]
mod tests;

pub use keymap::{DEFAULT_BINDING_COUNT, KEYMAP_NAME, default_keymap};
pub use panel::{HistoryOutcome, HistoryPanel};
pub use resolve::USER_LAYER_NAME;
