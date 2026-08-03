//! The Iridium native desktop shell.
//!
//! A window over the kernel beside it. Everything on screen is composed by
//! [`iridium_editor::render::FrameCompositor`] — the same per-frame assembly
//! the browser demo renders through — so this crate owns only what is
//! genuinely native: a winit window and its event loop ([`app`]), a wgpu
//! surface on that window ([`surface`]), the winit-to-kernel key translation
//! ([`keys`]), and the scroll offset the compositor is driven with. Editing
//! reaches the kernel through [`Editor::handle_key`](iridium_editor::Editor),
//! exactly as the terminal face's does; no verb is implemented here.
//!
//! Rendering is event-driven: a frame is composed on `RedrawRequested` and a
//! redraw is requested after input and resize, never from a loop. An idle
//! editor costs nothing.
//!
//! # THIS IS A SCAFFOLD — IT CANNOT SAVE
//!
//! This is slice 1 of the desktop shell (`docs/DESKTOP-SHELL-PLAN.md`,
//! step 2): the window, the surface, the compositor, the keyboard. There is
//! **no save path of any kind** — edits live in memory and are discarded when
//! the window closes. The program says so on standard error at startup, and
//! any host command a key resolves to (save included, once bound) surfaces in
//! the window title rather than pretending to run. Do not point this build at
//! a file you intend to keep editing.

pub mod app;
pub mod keys;
pub mod run;
pub mod surface;
pub mod units;

pub use run::{ExitStatus, main};
