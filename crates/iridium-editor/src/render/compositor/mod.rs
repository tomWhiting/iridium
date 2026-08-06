//! Face-neutral frame composition: the one place editor state becomes pixels.
//!
//! Every GPU face used to be at risk of the same fork: the browser bindings
//! grew a complete per-frame assembly — visible-content extraction, wrap
//! readback, gutter text, selection and caret quads, the render pass itself —
//! and a native shell would have had to rewrite all of it against the same
//! renderers. [`FrameCompositor`] is that assembly moved to the kernel, so a
//! face owns only what is genuinely its own: a surface, an input source, and
//! (optionally) a syntax resolver.
//!
//! # What a face keeps
//!
//! Three things deliberately stay on the face side of the seam:
//!
//! - **The surface.** [`FrameCompositor::compose`] takes a bare
//!   [`wgpu::TextureView`] plus device, queue and pixel dimensions
//!   ([`FrameTarget`]); acquiring and presenting the frame is the face's
//!   business, because that is where canvas and native window genuinely
//!   differ.
//! - **The scroll offset.** `scroll_y` is an input parameter. The face decides
//!   where the viewport is; the compositor only answers what that looks like
//!   ([`FrameCompositor::max_scroll_y`] and
//!   [`FrameCompositor::cursor_anchor_y`] exist so the face can decide well).
//! - **Syntax spans.** A face with a real highlighter (the web face runs
//!   tree-sitter in a worker) supplies its spans through [`HighlightSource`].
//!   When the face reports a language but has no spans this frame, the
//!   built-in keyword highlighter bridges until they arrive; when it reports
//!   no language, there is nothing to bridge to and the content renders in
//!   the plain theme foreground.
//!
//! # The caches are an interface, not a data structure
//!
//! Wrapping currently lives inside cosmic-text: the compositor hands the text
//! buffer a width and reads the resulting layout runs back into
//! `cached_visual_line_map`. Hit-testing ([`FrameCompositor::pixel_to_position`]
//! and its inverse) and scroll clamping run *between* frames, off that
//! readback. They are exposed as methods rather than fields so the planned
//! kernel-side wrap model (`docs/SOFT-WRAP-DESIGN.md`) can replace the
//! implementation without touching any face.
//!
//! # Where the pieces live
//!
//! [`FrameCompositor`] is one type with one set of fields; the split below is
//! by *when the code runs*, because that is the axis along which its bugs
//! appear. Two real defects found in this file both had the same shape — a
//! per-frame painter and a between-frames query each computing the same
//! number, agreeing by coincidence — and neither was visible while both
//! copies were thousands of lines apart in one scroll.
//!
//! - [`state`] — the fields, their documentation, and construction. The
//!   inventory every other module reads.
//! - [`target`], [`highlight`] — the two seams a face plugs into: the surface
//!   values, and the span resolver.
//! - [`metrics`], [`shape`] — the per-frame value types. Layout constants and
//!   the derived [`metrics::FrameMetrics`]; the retained-shaping cache key and
//!   the shapes it guards.
//! - [`frame`] — what runs *during* a frame: `compose`, the uniform sync, and
//!   the render pass.
//! - [`shaping`], [`gutter_column`], [`quads`] — the passes `compose` drives:
//!   content extraction and shaping, the gutter's width/text/change bars, and
//!   the background, selection and caret rectangles.
//! - [`placement`] — what runs *between* frames: scroll limits, both
//!   directions of hit-testing, and the readbacks they answer from.
//! - [`insets`], [`settings`] — what a face pushes in between frames: reserved
//!   chrome, fonts, theme, toggles and presentation inputs.

mod frame;
mod gutter_column;
mod highlight;
mod insets;
mod metrics;
mod placement;
mod quads;
mod settings;
mod shape;
mod shaping;
mod state;
mod target;

pub use highlight::{HighlightContext, HighlightSource};
pub use state::FrameCompositor;
pub use target::FrameTarget;
