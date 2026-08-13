//! The right-click context menu: the verbs the pointer can reach.
//!
//! Drawn, not native. `docs/design/CONTEXT-MENU-MAP.md` priced both ways and
//! the owner ruled D-1 for the drawn overlay: it rides the same
//! `RoundedQuadRenderer` chrome as the palette ([`crate::overlay`]), moves the
//! lockfile not at all, stays inside the winit event model the face's latency
//! identity depends on, and is a pattern every future GPU face inherits. The
//! stated price is that macOS's own Services, Look Up and dictation rows are
//! not available.
//!
//! # The panel is modal, and is a poorer palette
//!
//! Everything here is the palette's shape with the search engine removed: a
//! fixed list of rows, a selection that **clamps** at both ends rather than
//! wrapping, `Enter` running the highlighted row, `Escape` closing, and every
//! other key swallowed — a chord falling through to the document while the
//! user is aiming at a verb list would edit text they are not looking at.
//!
//! | Key | Action |
//! |---|---|
//! | `Down`, `Ctrl+N` / `Up`, `Ctrl+P` | move the highlight, clamping, over separators and disabled rows |
//! | `Home` / `End` | the first / last row that can run |
//! | `Enter` | run the highlighted verb |
//! | `Escape` | close |
//!
//! Those are **defaults**, not the whole of it: every one is a
//! [`KeyBinding`](iridium_editor::KeyBinding) in [`keymap::default_keymap`],
//! scoped to
//! [`CONTEXT_MENU_MODE`](iridium_editor::commands::builtin::CONTEXT_MENU_MODE),
//! and a `[keys]` line naming one of the six `contextMenu.*` commands rebinds
//! it — #117. [`verb`] is the vocabulary and [`resolve`] is the seam.
//!
//! # The verbs
//!
//! D-2 ruled the buildable floor: Cut, Copy, Paste, Select All and a
//! Command Palette… row. Every one of them is a command **already in the
//! kernel's registry** — this module registers nothing and implements no verb;
//! it resolves a [`CommandId`] and the host dispatches it down the same
//! kernel-first path a palette entry takes. A verb the registry does not carry
//! is dropped from the menu rather than offered and refused, and the test
//! module proves each id against a real registry.
//!
//! Labels are the menu's own, in the platform's menu vocabulary; for the four
//! editing verbs they agree with the registry's titles (a test pins that), and
//! the palette row is named for the panel it opens rather than for the
//! command's palette title.
//!
//! # What is greyed, and what deliberately is not
//!
//! A row is disabled when the command it runs mutates the document
//! ([`CommandMeta::mutates_document`](iridium_editor::commands::CommandMeta::mutates_document))
//! and the document is read-only — the one honest disable this face can state.
//! Cut and Copy stay **enabled** with a collapsed selection (D-5): the kernel's
//! clipboard verbs copy the caret lines when nothing is selected, so greying
//! them the way macOS does would misstate what they do.

mod keymap;
mod menu;
mod resolve;
mod verb;

#[cfg(test)]
mod keymap_tests;
#[cfg(test)]
mod tests;

/// ⚠️ Re-exported so the face can hand this layer to `iridium_config` when it
/// writes `config.toml`.
///
/// `iridium_config` reads the kernel's and the file explorer's keymaps itself,
/// but it cannot reach a face's — the dependency edge runs the other way — so a
/// panel whose layer is not handed over is a panel whose chords are absent from
/// a file that claims to list every binding. See `crate::commands::template`.
pub use keymap::default_keymap;
pub use menu::{ContextMenu, MenuOutcome};
pub use resolve::USER_LAYER_NAME;
