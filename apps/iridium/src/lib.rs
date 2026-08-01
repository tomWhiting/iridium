//! The Iridium terminal editor: the binary that ties the face to the kernel.
//!
//! Step 6 of `docs/TERMINAL-FACE-PLAN.md`.
//!
//! ```text
//! apps/iridium          this crate: open/save, go-to-line, theme, folds
//!   └── iridium-tui     the face: cell buffer, diff repaint, input, driver
//!         └── iridium-editor   the kernel: document, history, layout, commands
//! ```
//!
//! # Why there is a library here at all
//!
//! A `main.rs` that holds the logic cannot be tested, and "it is a binary" is
//! not an excuse for a program that writes people's files. Everything except
//! the event loop is in this library and runs on the host with no terminal:
//!
//! * [`cli`] parses the command line and nothing else;
//! * [`file`] reads and writes files, and decides whether one changed
//!   underneath us;
//! * [`theme`] loads a theme through the kernel's theme module;
//! * [`app`] is the whole editor as a state machine — keys in, editor state and
//!   a cell buffer out.
//!
//! What is left is [`run`], which opens a terminal, blocks on it, and feeds
//! what it gets to [`app::App`]. That is the only part that cannot run in CI,
//! and it is a page long.
//!
//! # The kernel owns the verbs
//!
//! Nothing here edits a document. Every editing action reaches the kernel
//! through [`Editor::handle_key`](iridium_editor::Editor::handle_key), through
//! a kernel verb named by a command binding, or through
//! [`Editor::apply_command`](iridium_editor::Editor::apply_command) for the
//! clipboard the kernel hands back. Three separate bugs this year were a face
//! reimplementing something the kernel already had — multi-cursor,
//! `deleteToLineStart` and undo — so it is the standing suspicion.
//!
//! Where this crate adds a *command* rather than an implementation — save,
//! reload, quit, go-to-line, the fold verbs — it registers it as a **host
//! command** in the kernel's registry and binds it in a keymap layer, which is
//! the mechanism the kernel provides for exactly this. See [`app::commands`].
//!
//! # A save must never be able to truncate a file
//!
//! [`file::write_atomically`] writes to a temporary file beside the target,
//! fsyncs it, and renames over the target. A failure at any point before the
//! rename leaves the original untouched, and the temporary file is removed. A
//! save that could half-write is the worst thing this binary can do, so it is
//! the one thing here with no fallback path.

/// The whole editor as a state machine: keys in, cell buffer out.
pub mod app;
/// The command line.
pub mod cli;
/// Reading and writing files, and noticing when one changed underneath us.
pub mod file;
/// The terminal event loop.
pub mod run;
/// Loading a theme through the kernel's theme module.
pub mod theme;

pub use app::App;
pub use cli::Invocation;
pub use run::{ExitStatus, main};
