//! The whole desktop session as an event-driven state machine: winit events
//! in, composed frames out.
//!
//! [`DesktopApp`] is this face's counterpart of the terminal face's `App`:
//! the kernel owns every verb, and a key reaches it through
//! [`Editor::handle_key`](iridium_editor::Editor::handle_key) after
//! [`crate::keys`] translates it. The verbs this face *adds* — save, and
//! save-anyway — are registered as host commands and bound in a keymap layer
//! ([`crate::commands`]), exactly the kernel-first dispatch pattern the
//! terminal face established: the kernel resolves the chord, reports
//! `EditorKeyResult::HostCommand`, and `dispatch_host_command` in
//! [`host_commands`] is the only place a host verb runs. A host command
//! nothing here implements is reported on the prompt strip rather than
//! silently dropped — a key that visibly names what it could not do is the
//! face being honest; a key that silently does nothing is a dead key.
//!
//! # Rendering is event-driven
//!
//! A frame is composed only on `RedrawRequested`, and a redraw is requested
//! only after input, resize or scale change — mouse effects included, and
//! only actual effects: a cursor merely gliding across the window repaints
//! nothing. There is no animation loop, so an idle editor costs nothing —
//! the same discipline as the terminal face, with the one visible consequence
//! that the caret does not blink while the keyboard is idle.
//!
//! # The mouse drives the kernel's machinery
//!
//! Clicks, drags and multi-clicks go through the kernel's `MouseHandler`
//! with positions resolved by the compositor's hit test — see
//! [`crate::mouse`] for the synthesized-grid seam between the two. The wheel
//! writes `scroll_y` directly under the same clamp every other scroll write
//! uses, because the scroll offset is the face's to own.
//!
//! # The clipboard is the system's
//!
//! Copy, cut and paste round-trip through the macOS pasteboard via `arboard`.
//! A pasteboard that cannot be reached is reported on the prompt strip, never
//! swallowed — and the delete half of a cut applies only after the pasteboard
//! holds the text, because a cut whose copy failed would destroy the only
//! copy.
//!
//! # Saving is real, and shared
//!
//! The file layer is `iridium-file` — the identical atomic-save, staleness-by
//! -byte-comparison code the terminal face runs. A save is refused when the
//! file changed on disk (the forced chord overwrites), an unnamed buffer asks
//! for a name on the prompt strip, and closing the window over unsaved
//! changes asks first. Dirtiness is content-based: the document is compared
//! against the bytes the file held when it and this program last agreed, so a
//! document undone back to its saved state is clean.
//!
//! # Scroll is owned here, in physical pixels
//!
//! Like the web face, this face owns `scroll_y` and the compositor answers
//! layout questions about it: `max_scroll_y` bounds it and `cursor_anchor_y`
//! drives scroll-to-caret, both wrap-aware from the last composed frame. The
//! kernel's own viewport is still synced on resize — not for painting, which
//! ignores it, but because `cursor.pageUp`/`pageDown` take their hop from
//! `viewport.visible_lines`, and a page key must hop what is actually on
//! screen. That sync goes through `state_mut`, which discards sticky columns
//! and pending chords; it runs only on resize and scale change, where the
//! terminal face pays the same price for the same reason.
//!
//! # Where the pieces live
//!
//! [`DesktopApp`] is one struct with one set of fields; the split below is by
//! *which input a method answers*, because that is how a defect here presents
//! itself — "the right-click menu does the wrong thing", "the wheel scrolls
//! past the end", "the title says the wrong file". A reader arriving with one
//! of those has one file to open.
//!
//! - [`state`] — the fields, their documentation, and the questions asked of
//!   them often enough to have no other home (dirtiness, the redraw request).
//! - [`config`] — the user's configuration file, brought into a session: the
//!   bindings pushed one refusal at a time, and everything that could not be
//!   honoured written into a tab.
//! - [`startup`] — bringing a session into existence: what the command line
//!   asked for, the window and GPU behind it, and the first tab.
//! - [`handler`] — the winit seam. Every entry point below is reached from
//!   here, and nothing here decides anything.
//! - [`keyboard`], [`pointer`], [`menu`] — the three input surfaces, each
//!   owning its own modal ladder.
//! - [`host_commands`], [`files`], [`clipboard`], [`tabs`] — what a resolved
//!   verb actually does.
//! - [`viewport`] — scroll, the clamp, and the window geometry that feeds it.
//! - [`paint`], [`title`] — what the session puts on screen and on the window.

mod clipboard;
mod config;
mod files;
mod handler;
mod host_commands;
mod keyboard;
mod menu;
mod paint;
mod pointer;
mod startup;
mod state;
mod tabs;
mod title;
mod viewport;

#[cfg(test)]
mod tests;

pub use startup::{Options, StartupError};
pub use state::{DesktopApp, Flow};
