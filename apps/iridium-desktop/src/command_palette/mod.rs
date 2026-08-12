//! The command palette: a floating panel that runs any command by name.
//!
//! This is the desktop face of the kernel's `palette.open` host command — the
//! UI the kernel names but cannot draw — and it keeps the terminal face's
//! semantics exactly; the terminal's test suite is the specification.
//!
//! # The engine is the kernel's and none of it is repeated here
//!
//! Nothing in this module matches, scores, ranks or remembers. Every
//! keystroke re-runs
//! [`palette::search_text`](iridium_editor::commands::palette::search_text)
//! against the kernel's own registry — the same matcher, the same recency
//! bonus, the same total order every face uses, so the same query can never
//! put a different command first here than in the terminal. What is here is a
//! text field, a selection, a scroll window and rows for [`crate::overlay`]
//! to paint.
//!
//! Recency lives in a [`CommandMru`](iridium_editor::commands::palette::CommandMru)
//! the *host* owns and records into after a command actually runs; the panel
//! only reads it. A panel that recorded on `Enter` would remember commands
//! whose execution then failed.
//!
//! # ⭐ The keys are data, and that is what #117 changed
//!
//! Every chord below used to be an arm of a `match (Chord, KeyCode)` inside
//! this file, so none of them could be rebound and none of them appeared in
//! the generated `[keys]` block. They are now a [`Keymap`](iridium_editor::Keymap)
//! — [`keymap::default_keymap`] — resolved through the kernel's own stack, with
//! the user's `[keys]` layer pushed on top of it.
//!
//! - [`keymap`] — the default bindings, and why shift is `Any` on all of them.
//! - [`verb`] — the twelve things the palette can be asked to do, one per
//!   kernel command id, so a verb the kernel names and this panel forgot is a
//!   compile error.
//! - [`resolve`] — turning a keypress into a verb, and the filtering that keeps
//!   a user's document bindings out of this panel's stack.
//! - [`keys`] — what each verb does, and the printable fall-through that is
//!   deliberately not a binding.
//! - [`panel`] — the state: the query, the selection, the scroll window, and
//!   the composition that turns them into rows.
//!
//! # The panel is modal
//!
//! Every key is consumed while it is open, exactly like a prompt: the palette
//! exists to run any command by name, and a chord falling through to the
//! document while the user is aiming at a command list would edit text they
//! are not looking at. `Escape` closes it; so do the `Ctrl+K` and `⌘K` that
//! open it, because a toggle is what the finger expects.
//!
//! | Key | Action |
//! |---|---|
//! | any printable | insert into the query |
//! | `Left` `Right` `Home` `End` `Backspace` `Delete` | edit the query |
//! | `Down`, `Ctrl+N` / `Up`, `Ctrl+P` | move the selection, clamping |
//! | `PageDown` / `PageUp` | hop by one windowful |
//! | `Enter` | run the selected command |
//! | `Escape`, `Ctrl+K`, `⌘K` | close |
//!
//! The selection **clamps** at both ends rather than wrapping: wrapping
//! overshoots on key repeat, and a list whose ends are walls can be leaned
//! on.
//!
//! ⚠️ **This table is the default, not the law.** It is what
//! [`keymap::default_keymap`] binds, and a user's `[keys]` layer can say
//! otherwise — which is the whole point of the conversion.
//!
//! # What a row shows
//!
//! The command's title; the key that runs it, right-aligned in the mac glyph
//! spelling ([`KeyLabelStyle::MacGlyphsCommandAsMeta`](iridium_editor::KeyLabelStyle)
//! — this face's chords really are ⌘ chords) and dropped before it would
//! collide with the title; and, when the query matched something other than
//! the title, that matched text as a quiet annotation after it — highlighting
//! match positions inside a string that is not shown would underline nothing.
//! Match positions are recoloured on exactly the characters the kernel
//! reported, mapped through the text's own grapheme clusters so a joined emoji
//! recolours whole, never mid-cluster.

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
pub use panel::{CommandPalette, PaletteOutcome};
pub use resolve::USER_LAYER_NAME;
