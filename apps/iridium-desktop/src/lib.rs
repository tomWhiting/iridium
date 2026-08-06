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
//! # What slices 2 and 3 deliver
//!
//! On top of slice 1's window, surface, compositor and keyboard
//! (`docs/DESKTOP-SHELL-PLAN.md`, step 2):
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
//! - **The painted overlays, the full set of Tom's D-C ruling**: the prompt
//!   strip ([`prompt`]), the search and replace panel ([`search`]), the
//!   command palette ([`command_palette`]) and the undo-tree panel
//!   ([`history_overlay`]) — all drawn by the kernel's own
//!   `RoundedQuadRenderer` and `TextRenderer` in a second `LoadOp::Load`
//!   render pass ([`overlay`]
//!   documents the pass structure), all keeping the terminal face's
//!   semantics exactly, with the terminal's test suites as the
//!   specification. `Ctrl+F`/`⌘F` searches, `Ctrl+K`/`⌘K`/`Ctrl+P` opens
//!   the palette, `Ctrl+Alt+H`/`⌘⌥H` toggles the undo tree.
//!
//! # What slice 4 delivers
//!
//! - **Real tree-sitter highlighting**, through the compositor's
//!   [`HighlightSource`](iridium_editor::render::HighlightSource) seam: the
//!   kernel's own parse tree — the one it already keeps current after every
//!   mutation — is read into an indexed span set once per parse generation
//!   and resolved into coloured text runs each frame ([`highlight`]
//!   documents the cache and its revision gate). This is the terminal face's
//!   highlight machinery (`iridium-tui`'s frame cache) driving a GPU frame:
//!   the same `Highlighter`, the same `SpanIndex`, the kernel's own
//!   `highlight_to_color` mapping, so the two native faces cannot drift.
//!   Nothing parses on the frame path.
//! - **The `.app` bundle**: `bundle/bundle.sh` builds the release binary and
//!   assembles `target/release/bundle/iridium.app` — hand-written
//!   `Info.plist`, the "77" icon (`bundle/icon.html`, regenerable via
//!   `bundle/make-icon.sh`), ad-hoc codesign — with no bundling dependency.
//!
//! # What step 3 delivers
//!
//! - **Keydown-to-present latency measurement** ([`latency`]): with
//!   `IRIDIUM_LATENCY` set, every dispatched keystroke is timed from its
//!   winit event's receipt to the end of the frame that presents its
//!   effect, logged per sample and summarized (count, min, p50, p95, max)
//!   on clean shutdown. Off — the normal case — it costs one branch per
//!   event. The frame-time half of step 3 is the kernel's `compose_frame`
//!   criterion bench, which drives `FrameCompositor::compose` headlessly on
//!   a 10k-line document; the numbers land in `docs/DESKTOP-SHELL-PLAN.md`.
//!
//! # What the context-menu slice delivers
//!
//! - **The right-click menu** ([`context_menu`]), drawn rather than native
//!   per `docs/design/CONTEXT-MENU-MAP.md`'s D-1: another panel on the
//!   [`overlay`] chrome, hung from the click and clamped inside the window,
//!   running Cut, Copy, Paste, Select All and the palette through the same
//!   kernel-first dispatch a chord takes. The pointer steers it — hover
//!   highlights, a click runs — and it is modal to the keyboard while it is
//!   up.
//! - **The click-through fix**: a click outside an open palette or undo tree
//!   now dismisses that panel instead of falling through to the document
//!   underneath it.
//!
//! # What still is not here
//!
//! Document-wide search-match marking waits on the compositor growing a
//! highlight pass for it, and IME composition is not wired.

pub mod app;
pub mod command_palette;
pub mod commands;
pub mod context_menu;
pub mod file_tree;
pub mod highlight;
pub mod history_overlay;
pub mod keys;
pub mod latency;
mod line;
pub mod mouse;
pub mod overlay;
pub mod prompt;
pub mod run;
pub mod search;
pub mod surface;
pub mod tab_strip;
pub mod units;

pub use run::{ExitStatus, main};
