//! The Iridium native desktop shell.
//!
//! A window over the kernel beside it. Everything on screen is composed by
//! [`iridium_editor::render::FrameCompositor`] — the same per-frame assembly
//! the browser demo renders through — so this crate owns only what is
//! genuinely native: a winit window and its event loop ([`app`]), a wgpu
//! surface on that window ([`surface`]), the winit-to-kernel key and mouse
//! translation ([`keys`], [`mouse`]), the prompt strip painted over the
//! composed frame ([`overlay`], [`prompt`]), and the scroll offset the
//! compositor is driven with. Editing reaches the kernel through
//! [`Editor::handle_key`](iridium_editor::Editor) and the kernel's own mouse
//! machinery, exactly as the terminal face's does; no verb is implemented
//! here.
//!
//! Rendering is event-driven: a frame is composed on `RedrawRequested` and a
//! redraw is requested after input, resize and actual mouse effects, never
//! from a loop. An idle editor costs nothing.
//!
//! # What slice 2 delivers
//!
//! This is slice 2 of the desktop shell (`docs/DESKTOP-SHELL-PLAN.md`,
//! step 2), on top of slice 1's window, surface, compositor and keyboard:
//!
//! - **Mouse, per Tom's v1 ruling**: click places the caret, shift-click
//!   extends, drag selects, double-click selects the word and triple-click
//!   the line, ⌘-click adds a cursor, the wheel scrolls — all through the
//!   kernel's `MouseHandler`, with positions resolved by the compositor's
//!   wrap- and fold-aware hit test ([`mouse`] documents the seam).
//! - **The system clipboard**: copy, cut and paste hit the macOS pasteboard
//!   through `arboard`, and a pasteboard failure is reported on the prompt
//!   strip, never swallowed.
//! - **A real save path**: the `iridium-file` crate — the terminal face's
//!   atomic temp-fsync-rename writer and byte-comparison staleness check,
//!   extracted so both faces share one save path. `Ctrl+S`/`⌘S` save through
//!   the kernel's host-command route, a stale file refuses with the forcing
//!   chord named, an unnamed buffer prompts for a path, the title carries
//!   the document's name and ` [+]` dirty marker, and closing the window
//!   over unsaved changes asks first.
//! - **The first painted overlay**: a one-line prompt strip at the bottom of
//!   the window — save-as entry, quit confirmation, notices — drawn by the
//!   kernel's own `QuadRenderer` and `TextRenderer` in a second `LoadOp::Load`
//!   render pass. [`overlay`] documents the pass structure the remaining
//!   overlays will reuse.
//!
//! # What still is not here
//!
//! The search panel, the command palette and the undo-tree panel are not
//! painted yet — their chords resolve and report themselves on the prompt
//! strip rather than dying silently. Tree-sitter highlighting runs the
//! compositor's built-in fallback (the face does not feed the highlight seam
//! yet), IME composition is not wired, and there is no `.app` bundle — the
//! binary runs from the command line.

pub mod app;
pub mod commands;
pub mod keys;
pub mod mouse;
pub mod overlay;
pub mod prompt;
pub mod run;
pub mod surface;
pub mod units;

pub use run::{ExitStatus, main};
