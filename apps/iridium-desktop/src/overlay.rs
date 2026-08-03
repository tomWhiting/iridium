//! The prompt strip: one GPU-painted line at the bottom of the window.
//!
//! This is the first painted overlay, and it deliberately establishes the
//! pattern the bigger overlays (search, palette, undo tree) will reuse:
//! after [`FrameCompositor::compose`](iridium_editor::render::FrameCompositor)
//! has drawn the document into the surface's texture view, a **second render
//! pass** runs on the same view with [`LoadOp::Load`] — the composed frame is
//! kept, not cleared — drawing a background quad and a line of text over it.
//! The primitives are the kernel's own render types,
//! [`QuadRenderer`] and [`TextRenderer`], publicly exported from
//! `iridium_editor::render` precisely so a face paints its overlays with the
//! renderers the kernel already trusts instead of forking text rendering.
//!
//! # The pass structure, for the next overlay to copy
//!
//! 1. Shape the overlay's text into its own glyphon buffer and `prepare` it —
//!    this overlay owns a [`TextRenderer`] (and therefore an atlas and font
//!    system) separate from the compositor's, because `prepare` uploads one
//!    frame's text areas per renderer and the compositor has already spent
//!    its upload on the document.
//! 2. Begin a render pass on the frame's view with `LoadOp::Load`.
//! 3. Draw quads first (background, caret), then the text on top.
//! 4. Submit. The face's `render_frame` closure presents afterwards, so the
//!    document pass and every overlay pass reach the screen as one frame.
//!
//! # Geometry
//!
//! The strip is one line high plus padding, pinned to the bottom edge, full
//! width. Text is drawn on the same `char × char_width` grid the compositor
//! uses for the document, and a field longer than the window scrolls
//! horizontally just enough to keep the caret visible — the same rule the
//! terminal face's prompt line applies, restated in pixels.

use glyphon::TextBounds;
use iridium_editor::IridiumError;
use iridium_editor::render::{FrameTarget, Quad, QuadRenderer, TextRenderer};
use iridium_editor::theme::Theme;
use wgpu::{
    CommandEncoderDescriptor, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureFormat,
};

use crate::units::{dimension_to_bound, pixel_to_bound, pixels_to_cells, u32_to_f32};

/// Horizontal padding between the window edge and the strip's text.
const TEXT_PAD: f32 = 10.0;

/// Vertical padding above and below the strip's line of text.
const STRIP_PAD: f32 = 6.0;

/// The caret bar's width in physical pixels.
const CARET_WIDTH: f32 = 2.0;

/// One line of strip content, ready to paint.
#[derive(Debug, Clone)]
pub struct StripContent {
    /// The line's text, label and field together.
    pub text: String,
    /// The caret's column in `char`s, or `None` for a strip with nothing to
    /// type into.
    pub caret_column: Option<usize>,
    /// Whether the line reports a failure, which colors it accordingly.
    pub is_error: bool,
}

/// Paints the strip. One instance belongs to one surface, exactly as the
/// compositor does; its renderers' viewport uniforms track that surface.
pub struct OverlayPainter {
    /// The kernel's text renderer, owned by this overlay: its own font
    /// system, atlas and prepared-area slot.
    text: TextRenderer,
    /// The kernel's quad renderer for the strip background and caret.
    quads: QuadRenderer,
}

impl std::fmt::Debug for OverlayPainter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayPainter").finish_non_exhaustive()
    }
}

impl OverlayPainter {
    /// Creates the painter for a surface with the given format and initial
    /// pixel dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error when the kernel's text renderer cannot be created.
    pub fn new(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Self, IridiumError> {
        let text = TextRenderer::new(device, queue, format)?;
        let quads = QuadRenderer::new(device, format);
        let mut painter = Self { text, quads };
        painter.resize(queue, width, height);
        Ok(painter)
    }

    /// Sets the strip's font, size first so the load remeasures at it —
    /// the same order the compositor requires, for the same reason:
    /// `load_font` is the only remeasuring path.
    pub fn set_font(&mut self, size: f32, data: Vec<u8>) {
        self.text.set_font_size(size);
        self.text.load_font(data);
    }

    /// Tracks a surface resize, in physical pixels.
    pub fn resize(&mut self, queue: &Queue, width: u32, height: u32) {
        self.text.update_viewport(queue, width, height);
        self.quads.update_viewport(queue, width, height);
    }

    /// Paints the strip over an already-composed frame.
    ///
    /// Runs the second render pass described in the module documentation.
    /// A window shorter than the strip paints nothing rather than a sliver.
    ///
    /// # Errors
    ///
    /// Returns an error when glyph preparation or text rendering fails.
    pub fn paint(
        &mut self,
        target: FrameTarget<'_>,
        content: &StripContent,
        theme: &Theme,
    ) -> Result<(), IridiumError> {
        let FrameTarget {
            view,
            device,
            queue,
            width,
            height,
        } = target;

        let width_f = u32_to_f32(width);
        let height_f = u32_to_f32(height);
        let line_height = self.text.line_height();
        let strip_height = 2.0_f32.mul_add(STRIP_PAD, line_height);
        if strip_height > height_f || width_f <= 2.0 * TEXT_PAD {
            return Ok(());
        }
        let strip_top = height_f - strip_height;
        let char_width = self.text.char_width();

        // Scroll the field just enough to keep the caret on screen.
        let cells = pixels_to_cells(2.0_f32.mul_add(-TEXT_PAD, width_f), char_width);
        let scroll_cells = content
            .caret_column
            .map_or(0, |caret| scroll_for(caret, cells));
        let scroll_x = crate::units::index_to_f32(scroll_cells) * char_width;

        // Background first, caret over it, text on top of both.
        let mut quads = vec![Quad::new(
            0.0,
            strip_top,
            width_f,
            strip_height,
            theme.editor.gutter,
        )];
        if let Some(caret) = content.caret_column {
            if caret >= scroll_cells {
                let caret_x =
                    crate::units::index_to_f32(caret - scroll_cells).mul_add(char_width, TEXT_PAD);
                quads.push(Quad::new(
                    caret_x,
                    strip_top + STRIP_PAD,
                    CARET_WIDTH,
                    line_height,
                    theme.editor.cursor,
                ));
            }
        }

        let color = if content.is_error {
            theme.editor.diagnostic_error
        } else {
            theme.editor.foreground
        };
        let mut buffer = self.text.create_buffer(None);
        self.text.set_text(&mut buffer, &content.text, color);
        self.text.shape_buffer(&mut buffer);

        let area = TextRenderer::create_text_area(
            &buffer,
            TEXT_PAD - scroll_x,
            strip_top + STRIP_PAD,
            1.0,
            TextBounds {
                // Clipped at the padding so a scrolled field's cut glyphs
                // vanish instead of bleeding under the window edge.
                left: pixel_to_bound(TEXT_PAD),
                top: pixel_to_bound(strip_top),
                right: dimension_to_bound(width),
                bottom: dimension_to_bound(height),
            },
            color,
        );
        self.text.prepare(device, queue, [area])?;

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Iridium Overlay Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Iridium Overlay Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: Operations {
                        // The composed document frame is underneath; keep it.
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.quads.render(&mut pass, queue, &quads);
            self.text.render(&mut pass)?;
        }
        queue.submit(std::iter::once(encoder.finish()));

        self.text.trim_cache();
        Ok(())
    }
}

/// The first cell of a line that is shown, so `caret` stays inside `cells`.
///
/// Zero until the caret would fall off the right edge, and then just enough
/// to keep it in the last cell. The caret's own cell counts: a caret past the
/// last glyph of a full line must still be visible, or typing into a field
/// that has filled the strip gives no feedback at all.
const fn scroll_for(caret: usize, cells: usize) -> usize {
    if cells == 0 || caret < cells {
        return 0;
    }
    caret + 1 - cells
}

#[cfg(test)]
mod tests {
    use super::scroll_for;

    #[test]
    fn a_short_line_does_not_scroll() {
        assert_eq!(scroll_for(0, 40), 0);
        assert_eq!(scroll_for(39, 40), 0);
    }

    #[test]
    fn the_caret_is_kept_in_the_last_cell() {
        assert_eq!(scroll_for(40, 40), 1);
        assert_eq!(scroll_for(55, 40), 16);
    }

    #[test]
    fn a_zero_width_strip_is_not_divided_by() {
        assert_eq!(scroll_for(10, 0), 0);
    }
}
