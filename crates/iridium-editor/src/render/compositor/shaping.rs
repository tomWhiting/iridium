//! Content extraction and shaping: the work a frame skips when its
//! [`ShapeKey`] matches the last one's.

use glyphon::Buffer;

use super::highlight::{HighlightContext, HighlightSource};
use super::shape::{RebuildGeometry, RetainedShape, ShapeKey};
use super::state::FrameCompositor;
use crate::editor::{Editor, FoldState};
use crate::render::text::TextRenderer;

impl FrameCompositor {
    /// Rebuilds the retained shaped buffers for a frame whose [`ShapeKey`]
    /// missed.
    ///
    /// When only the document content and/or the highlight answer moved
    /// against the previous frame's key ([`ShapeKey::permits_line_diff`]),
    /// the previous buffers are reclaimed and refilled through the per-line
    /// diffing setters — a one-character keystroke reshapes one line. Any
    /// other miss (viewport shift, resize, font, theme, folds, gutter) runs
    /// the full path: fresh buffers, full fill, full shape. Both arms end
    /// identically — shape, wrap readback, gutter text — so hit, diffed
    /// miss and full miss all leave the same state behind.
    pub(super) fn rebuild_retained(
        &mut self,
        previous: Option<RetainedShape>,
        key: ShapeKey,
        editor: &Editor,
        fold_state: &FoldState,
        highlights: &mut dyn HighlightSource,
        g: &RebuildGeometry,
    ) -> RetainedShape {
        self.shape_rebuilds = self.shape_rebuilds.wrapping_add(1);

        let doc_visual_lines =
            self.build_visible_content(editor, fold_state, g.viewport_start, g.viewport_end);

        let reclaimed = match previous {
            Some(prev) if key.permits_line_diff(&prev.key) => Some(prev),
            _ => None,
        };

        let (mut buffer, previous_gutter, previous_gutter_width_bits) =
            if let Some(prev) = reclaimed {
                let mut buffer = prev.buffer;
                let reshaped = self.fill_content_buffer_diffed(
                    &mut buffer,
                    highlights,
                    g.viewport_start_byte,
                    g.content_width,
                    g.line_height,
                );
                self.lines_reshaped = self
                    .lines_reshaped
                    .wrapping_add(u64::try_from(reshaped).unwrap_or(u64::MAX));
                (buffer, prev.gutter, Some(prev.gutter_width_bits))
            } else {
                let mut buffer = self.text_renderer.create_buffer(Some(g.content_width));
                self.fill_content_buffer(
                    &mut buffer,
                    highlights,
                    g.viewport_start_byte,
                    g.content_width,
                    g.line_height,
                );
                (buffer, None, None)
            };
        self.text_renderer.shape_buffer(&mut buffer);

        self.rebuild_visual_line_map(
            &buffer,
            g.viewport_start,
            g.content_offset_x,
            doc_visual_lines,
        );

        // NOW build line numbers with proper spacing for wrapped lines
        self.build_line_numbers(&buffer, g.line_count);

        // Create/refresh the gutter buffer AFTER we know the wrapping
        let gutter_width_bits = g.gutter_width.to_bits();
        let gutter = if self.gutter_enabled {
            let line_number_color = self.theme.editor.line_number;
            // Reclaim the previous gutter buffer only when its width is
            // bit-identical — see [`RetainedShape::gutter_width_bits`].
            let reusable =
                previous_gutter.filter(|_| previous_gutter_width_bits == Some(gutter_width_bits));
            let mut gutter_buf = if let Some(mut existing) = reusable {
                let reshaped = TextRenderer::set_text_diffed(
                    &mut existing,
                    &self.cpu_line_numbers,
                    line_number_color,
                );
                self.lines_reshaped = self
                    .lines_reshaped
                    .wrapping_add(u64::try_from(reshaped).unwrap_or(u64::MAX));
                existing
            } else {
                let mut fresh = self.text_renderer.create_buffer(Some(g.gutter_width));
                self.text_renderer
                    .set_text(&mut fresh, &self.cpu_line_numbers, line_number_color);
                fresh
            };
            self.text_renderer.shape_buffer(&mut gutter_buf);
            Some(gutter_buf)
        } else {
            None
        };

        RetainedShape {
            key,
            buffer,
            gutter,
            gutter_width_bits,
        }
    }

    /// Rebuilds the visible-content string and the doc↔visual line maps for
    /// the given viewport, honoring folds. Returns the total visible-line
    /// count (each visible document line counted once, wrapping not yet
    /// known).
    fn build_visible_content(
        &mut self,
        editor: &Editor,
        fold_state: &FoldState,
        viewport_start: usize,
        viewport_end: usize,
    ) -> usize {
        let doc = &editor.state().document;
        let line_count = doc.line_count();

        // Build visible content, only including lines in viewport
        // Line numbers are built AFTER shaping to account for line wrapping
        // PERF: Reuse pre-allocated buffers to avoid per-frame allocations
        self.cpu_visible_content.clear();
        self.cpu_visible_doc_lines.clear();

        // Ensure doc_to_visual has capacity for all lines (grows if needed, never shrinks)
        if self.cpu_doc_to_visual.capacity() < line_count {
            self.cpu_doc_to_visual
                .reserve(line_count - self.cpu_doc_to_visual.capacity());
        }
        self.cpu_doc_to_visual.clear();

        let mut visual_line = 0;

        // Pre-fill doc_to_visual for lines before viewport
        for doc_line in 0..viewport_start {
            if fold_state.is_line_hidden(doc_line) {
                self.cpu_doc_to_visual.push(None);
            } else {
                self.cpu_doc_to_visual.push(Some(visual_line));
                visual_line += 1;
            }
        }

        // Only process lines in viewport range
        for doc_line in viewport_start..viewport_end {
            if fold_state.is_line_hidden(doc_line) {
                self.cpu_doc_to_visual.push(None);
                continue;
            }

            // Add newline separator (except for first visible line in our buffer)
            if !self.cpu_visible_doc_lines.is_empty() {
                self.cpu_visible_content.push('\n');
            }

            // Add line content
            if let Some(line_text) = doc.line(doc_line) {
                self.cpu_visible_content.push_str(&line_text);
            }

            // If this line is folded, also append the closing brace from the fold end
            if fold_state.is_folded(doc_line) {
                if let Some(region) = fold_state.region_at(doc_line) {
                    // Get the end line and find the closing brace
                    if let Some(end_line_text) = doc.line(region.end_line) {
                        let trimmed = end_line_text.trim();
                        // Append the closing portion (usually just "}")
                        if !trimmed.is_empty() {
                            self.cpu_visible_content.push_str(" ... ");
                            self.cpu_visible_content.push_str(trimmed);
                        }
                    }
                }
            }

            self.cpu_visible_doc_lines.push(doc_line);
            self.cpu_doc_to_visual.push(Some(visual_line));
            visual_line += 1;
        }

        // Fill remaining doc_to_visual for lines after viewport
        for doc_line in viewport_end..line_count {
            if fold_state.is_line_hidden(doc_line) {
                self.cpu_doc_to_visual.push(None);
            } else {
                self.cpu_doc_to_visual.push(Some(visual_line));
                visual_line += 1;
            }
        }

        visual_line
    }

    /// Fills the content buffer's text and colors: the face's resolved spans
    /// when it has them, the built-in keyword bridge when a language is set
    /// but its spans are not available this frame, plain foreground when no
    /// language is set or syntax highlighting is off.
    fn fill_content_buffer(
        &mut self,
        buffer: &mut Buffer,
        highlights: &mut dyn HighlightSource,
        content_start_byte: usize,
        content_width: f32,
        line_height: f32,
    ) {
        // Set the text with syntax highlighting if enabled
        let foreground = self.theme.editor.foreground;
        if self.syntax_enabled {
            let context = HighlightContext {
                content: &self.cpu_visible_content,
                content_start_byte,
                content_width,
                line_height,
                viewport_config: &self.viewport_config,
                syntax_theme: &self.syntax_theme,
                foreground,
            };
            if let Some(rich_spans) = highlights.resolve(&context) {
                self.text_renderer
                    .set_rich_text(buffer, rich_spans.into_iter());
            } else if highlights.language_active() {
                // The bridge: a language is set but its spans are not here
                // this frame — the keyword highlighter colors until they are.
                // PERF: Pass iterator directly instead of collecting into Vec
                let spans = self.highlighter.highlight_flat(&self.cpu_visible_content);
                self.text_renderer.set_rich_text(
                    buffer,
                    spans.iter().map(|span| (span.text.as_str(), span.color)),
                );
            } else {
                // No language set: nothing to bridge to, plain foreground.
                self.text_renderer
                    .set_text(buffer, &self.cpu_visible_content, foreground);
            }
        } else {
            // Plain text
            self.text_renderer
                .set_text(buffer, &self.cpu_visible_content, foreground);
        }
    }

    /// The stage-2a counterpart of [`Self::fill_content_buffer`]: the same
    /// four-way span resolution, routed through the per-line diffing
    /// setters so lines whose text and colors did not change keep their
    /// shape caches. Returns how many lines were actually reshaped.
    fn fill_content_buffer_diffed(
        &self,
        buffer: &mut Buffer,
        highlights: &mut dyn HighlightSource,
        content_start_byte: usize,
        content_width: f32,
        line_height: f32,
    ) -> usize {
        let foreground = self.theme.editor.foreground;
        if self.syntax_enabled {
            let context = HighlightContext {
                content: &self.cpu_visible_content,
                content_start_byte,
                content_width,
                line_height,
                viewport_config: &self.viewport_config,
                syntax_theme: &self.syntax_theme,
                foreground,
            };
            if let Some(rich_spans) = highlights.resolve(&context) {
                TextRenderer::set_rich_text_diffed(buffer, rich_spans.into_iter())
            } else if highlights.language_active() {
                let spans = self.highlighter.highlight_flat(&self.cpu_visible_content);
                TextRenderer::set_rich_text_diffed(
                    buffer,
                    spans.iter().map(|span| (span.text.as_str(), span.color)),
                )
            } else {
                TextRenderer::set_text_diffed(buffer, &self.cpu_visible_content, foreground)
            }
        } else {
            TextRenderer::set_text_diffed(buffer, &self.cpu_visible_content, foreground)
        }
    }

    /// Reads the shaped buffer's layout runs back into the visual line map
    /// and derives the wrap-aware total visual line count.
    fn rebuild_visual_line_map(
        &mut self,
        buffer: &Buffer,
        viewport_start: usize,
        content_offset_x: f32,
        doc_visual_lines: usize,
    ) {
        // Build cached visual line map from layout_runs() for pixel_to_position.
        // Each entry maps a visual line (within the buffer) to (buffer_line_index, run_start_column).
        // This allows pixel_to_position to correctly resolve clicks on wrapped lines.
        self.cached_visual_line_map.clear();
        self.cached_map_viewport_start = viewport_start;
        self.cached_content_offset_x = content_offset_x;
        for run in buffer.layout_runs() {
            let run_start_col = if run.glyphs.is_empty() {
                0
            } else {
                run.glyphs.first().map_or(0, |g| g.start)
            };
            self.cached_visual_line_map
                .push((run.line_i, run_start_col));
        }

        // Cache total visual lines for max_scroll_y.
        // doc_visual_lines (from the doc_to_visual pass) counts all visible doc lines as 1 each.
        // The actual buffer visual lines (from layout_runs) may be more due to wrapping.
        // Extra wrapped lines = buffer_visual_lines - buffer_logical_lines.
        let buffer_visual_lines = self.cached_visual_line_map.len();
        let buffer_logical_lines = self.cpu_visible_doc_lines.len();
        let extra_wrap_lines = buffer_visual_lines.saturating_sub(buffer_logical_lines);
        self.cached_total_visual_lines = doc_visual_lines + extra_wrap_lines;
    }
}
