//! What runs *during* a frame: the per-frame pipeline, the uniform sync it
//! opens with, and the render pass it closes with.

use glyphon::{TextArea, TextBounds};
use web_time::Instant;
use wgpu::{
    CommandEncoderDescriptor, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureView,
};

use super::highlight::HighlightSource;
use super::metrics::{FrameMetrics, GUTTER_TEXT_PADDING};
use super::shape::{RebuildGeometry, ShapeKey};
use super::state::FrameCompositor;
use super::target::FrameTarget;
use crate::editor::{Editor, FoldState, IridiumError};
use crate::render::quad::Quad;
use crate::render::text::TextRenderer;
use crate::render::units::{
    dimension_to_bound, index_to_f32, pixel_to_bound, pixel_to_index, u32_to_f32,
};

impl FrameCompositor {
    /// Composes one frame from editor state onto the target.
    ///
    /// This is the whole per-frame pipeline: viewport virtualization and
    /// fold-aware content extraction, shaping (which is where wrapping
    /// happens), the wrap readback that feeds hit-testing, gutter text,
    /// background/selection/caret quads, and the render pass itself.
    /// `scroll_y` is the face's vertical offset in physical pixels;
    /// `highlights` is consulted only when syntax highlighting is enabled.
    ///
    /// The shaped buffers are retained across frames behind a
    /// [`ShapeKey`](super::shape::ShapeKey) of every input they depend on: a
    /// frame whose key matches the last one skips content extraction,
    /// highlight resolution and all shaping, and pays only what a frame
    /// legitimately owes — cursor walk, quads, glyph preparation and the
    /// render pass. Retention is an optimization and nothing else: hit or
    /// miss, the composed output is identical.
    ///
    /// # Errors
    ///
    /// Returns an error when glyph preparation or text rendering fails.
    pub fn compose(
        &mut self,
        editor: &Editor,
        fold_state: &FoldState,
        scroll_y: f32,
        highlights: &mut dyn HighlightSource,
        target: FrameTarget<'_>,
    ) -> Result<(), IridiumError> {
        let FrameTarget {
            view,
            device,
            queue,
            width,
            height,
        } = target;

        // PERF: Only update viewport uniforms when dimensions actually change
        // Avoids 3 GPU buffer writes per frame when dimensions are unchanged
        self.sync_viewport_uniforms(queue, width, height);

        // Get the content to render, handling folded lines
        let doc = &editor.state().document;
        let line_count = doc.line_count();

        // Calculate visible line range for viewport virtualization
        let line_height = self.text_renderer.line_height();
        let surface_height = u32_to_f32(height);
        let first_visible_line = pixel_to_index((scroll_y / line_height).floor());
        let visible_line_count = pixel_to_index((surface_height / line_height).ceil()) + 2;
        let overscan = 10; // Extra lines above/below for smooth scrolling
        let viewport_start = first_visible_line.saturating_sub(overscan);
        let viewport_end = (first_visible_line + visible_line_count + overscan).min(line_count);

        // Calculate the byte offset where visible_content starts in the full
        // document. This is needed to correctly map span byte offsets to
        // visible_content positions.
        let viewport_start_byte = doc.line_to_byte_offset(viewport_start).unwrap_or(0);

        // Calculate layout dimensions
        let char_width = self.cached_char_width;
        // `left_inset` is the chrome a face reserved beside the document;
        // `top_inset` is the chrome it reserved above. They start life at or
        // near the same number as each other and as the document's own
        // horizontal padding, and none of the three is the same concern — a
        // tab strip moves one and must not move the others.
        let top_inset = self.top_inset;
        let left_inset = self.left_inset;

        // Calculate gutter width (based on digit count or custom text width,
        // before shaping)
        let gutter_width = self.frame_gutter_width(line_count);
        let gutter_right = left_inset + gutter_width;
        let content_offset_x = self.content_left_edge_past_gutter(gutter_width);
        let content_width = u32_to_f32(width) - content_offset_x;

        // The retained-shaping gate: every input the shaped buffers depend
        // on, compared against the last frame's. On a hit the content
        // extraction, highlight resolution, both shaping passes, the wrap
        // readback and the gutter text are all skipped — the retained
        // buffers and the between-frames caches (`cpu_visible_content`,
        // `cpu_visible_doc_lines`, `cpu_doc_to_visual`,
        // `cached_visual_line_map`, `cpu_line_numbers`,
        // `cached_total_visual_lines`) are deterministic functions of the
        // same key and already hold this frame's values.
        let key = ShapeKey {
            document_revision: doc.revision(),
            viewport_start,
            viewport_end,
            content_width: content_width.to_bits(),
            font_size: self.text_renderer.font_size().to_bits(),
            line_height_factor: self.text_renderer.config().line_height.to_bits(),
            font_generation: self.font_generation,
            theme_generation: self.theme_generation,
            syntax_enabled: self.syntax_enabled,
            language_active: highlights.language_active(),
            highlight_generation: highlights.generation(),
            syntax_theme_generation: self.syntax_theme_generation,
            fold_generation: fold_state.generation(),
            gutter_text_generation: self.gutter_text_generation,
        };

        let retained = match self.retained.take() {
            Some(shape) if shape.key == key => shape,
            previous => self.rebuild_retained(
                previous,
                key,
                editor,
                fold_state,
                highlights,
                &RebuildGeometry {
                    viewport_start,
                    viewport_end,
                    viewport_start_byte,
                    content_width,
                    gutter_width,
                    content_offset_x,
                    line_height,
                    line_count,
                },
            ),
        };
        let buffer = &retained.buffer;
        let gutter_buffer = retained.gutter.as_ref();

        // Calculate cursor position (accounting for gutter offset, folding,
        // scroll, and line wrapping)
        let cursor_pos = editor.cursor();
        let visual_cursor_line = self
            .cpu_doc_to_visual
            .get(cursor_pos.line)
            .and_then(|v| *v)
            .unwrap_or(0);

        // Calculate virtual scroll offset for viewport virtualization
        // This must match the offset used for text rendering (uses viewport_start directly)
        let virtual_scroll_offset = index_to_f32(viewport_start) * line_height;

        // Calculate cursor's line index within the viewport buffer
        // The buffer only contains lines from viewport_start, so we need relative positioning
        let viewport_start_visual = self
            .cpu_doc_to_visual
            .get(viewport_start)
            .and_then(|v| *v)
            .unwrap_or(0);
        let cursor_line_in_buffer = visual_cursor_line.saturating_sub(viewport_start_visual);

        // Use buffer layout to get accurate position with line wrapping.
        //
        // Only the vertical position is wanted here. Scrolling follows the
        // primary caret alone, and inline blame sits on its line; the caret
        // *quads* — the primary's included — are positioned in the pass further
        // down, which walks every selection.
        let (_, wrap_y) = self.text_renderer.cursor_position_in_buffer(
            buffer,
            cursor_line_in_buffer,
            cursor_pos.column,
            char_width,
        );
        // Absolute cursor Y in document space (for cursor_anchor_y)
        let cursor_abs_y = top_inset + wrap_y + virtual_scroll_offset;
        // Viewport-relative cursor Y for rendering
        let cursor_y = cursor_abs_y - scroll_y;

        // Cache cursor position for cursor_anchor_y
        self.cached_cursor_abs_y = cursor_abs_y;
        self.cached_cursor_doc_line = cursor_pos.line;

        // Update cursor blink state
        self.cursor_renderer.update(Instant::now());

        let metrics = FrameMetrics {
            line_height,
            char_width,
            top_inset,
            content_offset_x,
            left_inset,
            virtual_scroll_offset,
            viewport_start,
            viewport_start_visual,
            scroll_y,
            surface_width: u32_to_f32(width),
            surface_height,
        };

        // PERF: Reuse pre-allocated quad buffers
        // Create gutter background quad
        self.cpu_gutter_quads.clear();
        if self.gutter_enabled {
            let gutter_bg_color = self.theme.editor.gutter;
            self.cpu_gutter_quads.push(Quad::new(
                left_inset,
                0.0,
                gutter_width,
                metrics.surface_height,
                gutter_bg_color,
            ));
        }

        self.build_line_background_quads(&metrics);
        self.build_gutter_change_quads(&metrics);
        self.build_selection_quads(editor, buffer, &metrics);
        self.build_cursor_quads(editor, buffer, &metrics);

        // Text areas share the virtualized scroll position: the buffer starts
        // at viewport_start, so only the remainder of the scroll applies.
        let adjusted_scroll_y = scroll_y - virtual_scroll_offset;
        let foreground = self.theme.editor.foreground;

        // Main content text area
        let main_text_area = TextRenderer::create_text_area(
            buffer,
            content_offset_x,
            top_inset - adjusted_scroll_y,
            1.0,
            TextBounds {
                // Clipped at the gutter's right edge rather than at the
                // window's, for the same reason the top is clipped at the
                // inset: chrome beside the text must not be drawn over.
                //
                // The gutter's edge, not the content's own origin, so the
                // padding stays as slack for a glyph with negative left
                // bearing — an italic `f` at column zero has ink left of its
                // origin, and clipping at the origin would shave it.
                left: pixel_to_bound(gutter_right),
                // Clipped at the reserved band rather than at the window
                // edge: with chrome above the text, a glyph scrolled past
                // the top would otherwise be shaped and drawn behind it.
                top: pixel_to_bound(top_inset),
                right: dimension_to_bound(width),
                bottom: dimension_to_bound(height),
            },
            foreground,
        );

        // Build blame buffer if blame data exists for the cursor line.
        // This creates a third TextArea rendered as ghost text after the line content.
        let blame_fg = self.theme.editor.blame_foreground;
        let blame_buffer = if self.blame_data.is_empty() {
            None
        } else {
            let cursor_doc_line = editor.cursor().line;
            if let Some(blame_text) = self.blame_data.get(&cursor_doc_line) {
                self.cpu_blame_content.clear();
                // Pad with spaces to separate from line content
                self.cpu_blame_content.push_str("  ");
                self.cpu_blame_content.push_str(blame_text);
                let mut blame_buf = self.text_renderer.create_buffer(None);
                self.text_renderer
                    .set_text(&mut blame_buf, &self.cpu_blame_content, blame_fg);
                self.text_renderer.shape_buffer(&mut blame_buf);
                Some(blame_buf)
            } else {
                None
            }
        };

        // Calculate blame text area position: after the end of the cursor line content
        let blame_left = if blame_buffer.is_some() {
            let cursor_doc_line = editor.cursor().line;
            let line_len = editor
                .state()
                .document
                .line(cursor_doc_line)
                .map_or(0, |l| l.chars().count());
            let line_end_x = index_to_f32(line_len) * char_width;
            content_offset_x + line_end_x
        } else {
            0.0
        };

        // Prepare text for rendering - use Vec for dynamic text area count
        let line_number_color = self.theme.editor.line_number;

        // Build text areas list dynamically based on what's enabled
        let mut text_areas: Vec<TextArea> = Vec::with_capacity(3);
        text_areas.push(main_text_area);

        if let Some(gutter_buf) = gutter_buffer {
            text_areas.push(TextRenderer::create_text_area(
                gutter_buf,
                // Measured from the gutter's own left edge, which is the
                // window's only until a face reserves chrome beside the text.
                left_inset + GUTTER_TEXT_PADDING,
                // `top_inset`, not `padding`: the numbers have to start where
                // the lines they number start. The two were the same value
                // until a face reserved chrome above the document, at which
                // point the code moved down and the numbers did not, and
                // every line wore the number of the line above it.
                top_inset - adjusted_scroll_y,
                1.0,
                TextBounds {
                    left: pixel_to_bound(left_inset),
                    // Clipped at the inset like the content is, so a scrolled
                    // gutter's rows vanish into the reserved band rather than
                    // drawing inside it.
                    top: pixel_to_bound(top_inset),
                    right: pixel_to_bound(gutter_right),
                    bottom: dimension_to_bound(height),
                },
                line_number_color,
            ));
        }

        if let Some(blame_buf) = &blame_buffer {
            text_areas.push(TextRenderer::create_text_area(
                blame_buf,
                blame_left,
                cursor_y,
                1.0,
                TextBounds {
                    left: pixel_to_bound(blame_left),
                    // Clipped at the inset like the content and the gutter
                    // are. Blame sits on the caret's line, and the caret can
                    // be scrolled above the reserved band — with the window
                    // edge as the bound, the ghost text would then be drawn
                    // *inside* a face's chrome. Missed when the other two
                    // were fixed because blame is set only from the web face,
                    // which draws nothing above the document yet.
                    top: pixel_to_bound(top_inset),
                    right: dimension_to_bound(width),
                    bottom: dimension_to_bound(height),
                },
                blame_fg,
            ));
        }

        self.text_renderer.prepare(device, queue, text_areas)?;

        // Get background color from theme
        let bg = self.theme.editor.background;
        let clear_color = wgpu::Color {
            r: f64::from(bg.r),
            g: f64::from(bg.g),
            b: f64::from(bg.b),
            a: f64::from(bg.a),
        };

        // PERF: Batch all background quads using pre-allocated buffer.
        // Render order (back to front): gutter bg → line backgrounds → gutter change bars → selection
        self.cpu_background_quads.clear();
        self.cpu_background_quads
            .extend(self.cpu_gutter_quads.iter().copied());
        self.cpu_background_quads
            .extend(self.cpu_line_bg_quads.iter().copied());
        self.cpu_background_quads
            .extend(self.cpu_gutter_change_quads.iter().copied());
        self.cpu_background_quads
            .extend(self.cpu_selection_quads.iter().copied());

        self.submit_pass(view, device, queue, clear_color)?;

        // Trim glyph cache periodically
        self.text_renderer.trim_cache();

        // Keep the shaped buffers for the next frame. A frame that failed
        // above simply drops them — the next frame rebuilds, which costs a
        // miss and nothing else.
        self.retained = Some(retained);

        Ok(())
    }

    /// Re-uploads the renderers' viewport uniforms when the surface size
    /// changed since the last frame; a no-op otherwise.
    fn sync_viewport_uniforms(&mut self, queue: &Queue, width: u32, height: u32) {
        if width != self.cached_viewport_width || height != self.cached_viewport_height {
            self.cached_viewport_width = width;
            self.cached_viewport_height = height;
            self.text_renderer.update_viewport(queue, width, height);
            self.background_quad_renderer
                .update_viewport(queue, width, height);
            self.cursor_quad_renderer
                .update_viewport(queue, width, height);
        }
    }

    /// Records and submits the frame's render pass: background quads, then
    /// text, then cursor quads on top.
    ///
    /// The two quad renderers are separate on purpose — see the field
    /// documentation on `FrameCompositor::cursor_quad_renderer`.
    fn submit_pass(
        &mut self,
        view: &TextureView,
        device: &Device,
        queue: &Queue,
        clear_color: wgpu::Color,
    ) -> Result<(), IridiumError> {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Iridium Frame Encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Iridium Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(clear_color),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Render gutter background and selection highlights (behind text)
            self.background_quad_renderer
                .render(&mut pass, queue, &self.cpu_background_quads);

            // Render text (main content and gutter line numbers)
            self.text_renderer.render(&mut pass)?;

            // Render cursor on top - separate vertex buffer avoids the GPU
            // buffer overwrite issue that caused selection blinking
            self.cursor_quad_renderer
                .render(&mut pass, queue, &self.cpu_cursor_quads);
        }

        queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }
}
