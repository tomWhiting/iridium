//! The gutter column: how wide it is, what text it carries, and the change
//! bars drawn inside it.
//!
//! The two width answers live together on purpose. [`FrameCompositor::gutter_width`]
//! is what a between-frames query measures from and
//! [`FrameCompositor::frame_gutter_width`] is what the painter measures from,
//! and they differ — deliberately, on custom gutter text. Keeping them in one
//! file is the only thing that makes that difference reviewable.

use std::fmt::Write as _;

use glyphon::Buffer;

use super::metrics::{CHANGE_BAR_INSET, CHANGE_BAR_WIDTH, FrameMetrics};
use super::state::FrameCompositor;
use crate::render::gutter::GutterRenderer;
use crate::render::quad::Quad;
use crate::render::units::index_to_f32;

impl FrameCompositor {
    /// The gutter width for the frame being composed: measured from the
    /// longest custom gutter line when custom text is active, otherwise from
    /// the document's digit count.
    ///
    /// Distinct from [`Self::gutter_width`], which is the between-frames
    /// answer hit-testing uses and which — exactly as before the extraction —
    /// does not consult custom gutter text.
    pub(super) fn frame_gutter_width(&self, line_count: usize) -> f32 {
        if !self.gutter_enabled {
            return 0.0;
        }
        let char_width = self.cached_char_width;
        self.custom_gutter_lines.as_ref().map_or_else(
            || self.gutter_renderer.calculate_width(line_count, char_width),
            |custom_lines| {
                // Width from longest custom gutter line
                let max_chars = custom_lines.iter().map(String::len).max().unwrap_or(1);
                // Same padding formula as GutterRenderer::calculate_width
                let number_width = index_to_f32(max_chars) * char_width;
                let side_padding = char_width * 2.0;
                number_width + side_padding
            },
        )
    }

    /// The gutter width used for between-frames layout queries: digit-count
    /// based, zero when the gutter is disabled.
    ///
    /// Deliberately does not consult custom gutter text — the pre-extraction
    /// hit-testing path never did, and pixel-identity keeps that as it is.
    pub fn gutter_width(&self, line_count: usize) -> f32 {
        if self.gutter_enabled {
            self.gutter_renderer
                .calculate_width(line_count, self.cached_char_width)
        } else {
            0.0
        }
    }

    /// Rebuilds the gutter text with wrap-aware blank continuation rows:
    /// custom gutter lines when set, right-aligned line numbers otherwise.
    pub(super) fn build_line_numbers(&mut self, buffer: &Buffer, line_count: usize) {
        // PERF: Reuse pre-allocated string buffer, use write!() to avoid format!() allocation
        let visual_lines_per_line = self.text_renderer.visual_lines_per_logical_line(buffer);
        self.cpu_line_numbers.clear();

        if let Some(custom_lines) = &self.custom_gutter_lines {
            // Custom gutter text: use provided strings instead of auto line numbers.
            // This is used for diff views with dual line numbers + markers.
            for (i, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
                if i > 0 {
                    self.cpu_line_numbers.push('\n');
                }

                // Use custom text for this doc_line, or empty string if out of range
                let text = custom_lines.get(doc_line).map_or("", |s| s.as_str());
                self.cpu_line_numbers.push_str(text);

                // Add blank lines for wrapped visual lines
                let wrap_count = visual_lines_per_line.get(i).copied().unwrap_or(1);
                let text_len = text.len();
                for _ in 1..wrap_count {
                    self.cpu_line_numbers.push('\n');
                    for _ in 0..text_len {
                        self.cpu_line_numbers.push(' ');
                    }
                }
            }

            if self.cpu_visible_doc_lines.is_empty() {
                self.cpu_line_numbers.push(' ');
            }
        } else {
            // Standard auto line numbers
            let digit_width = GutterRenderer::digit_columns(line_count);

            for (i, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
                // Add newline separator (except for first visible line)
                if i > 0 {
                    self.cpu_line_numbers.push('\n');
                }

                // Add line number (1-indexed) directly into buffer without allocation
                // write!() into String never fails, so we can ignore the Result
                let _ = write!(
                    self.cpu_line_numbers,
                    "{:>width$}",
                    doc_line + 1,
                    width = digit_width,
                );

                // Add blank lines for wrapped visual lines (continuation lines)
                let wrap_count = visual_lines_per_line.get(i).copied().unwrap_or(1);
                for _ in 1..wrap_count {
                    self.cpu_line_numbers.push('\n');
                    // Add blank spacing to maintain alignment
                    for _ in 0..digit_width {
                        self.cpu_line_numbers.push(' ');
                    }
                }
            }

            // Handle empty document
            if line_count == 0 {
                self.cpu_line_numbers.push_str(" 1");
            }
        }
    }

    /// Emits gutter change indicator quads (thin colored bars at the left
    /// edge).
    pub(super) fn build_gutter_change_quads(&mut self, m: &FrameMetrics) {
        self.cpu_gutter_change_quads.clear();
        if self.gutter_changes.is_empty() || !self.gutter_enabled {
            return;
        }
        for (vi, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
            if let Some(&bar_color) = self.gutter_changes.get(&doc_line) {
                let buffer_line = self
                    .cpu_doc_to_visual
                    .get(doc_line)
                    .and_then(|v| *v)
                    .unwrap_or(0)
                    .saturating_sub(
                        self.cpu_doc_to_visual
                            .get(m.viewport_start)
                            .and_then(|v| *v)
                            .unwrap_or(0),
                    );

                let mut emitted = false;
                for (vline_idx, &(buf_idx, _)) in self.cached_visual_line_map.iter().enumerate() {
                    if buf_idx != buffer_line {
                        continue;
                    }
                    let row_y = index_to_f32(vline_idx) * m.line_height;
                    let y = m.top_inset + row_y + m.virtual_scroll_offset - m.scroll_y;
                    if y + m.line_height > 0.0 && y < m.surface_height {
                        // 3px wide bar at left gutter edge
                        self.cpu_gutter_change_quads.push(Quad::new(
                            m.left_inset + CHANGE_BAR_INSET,
                            y,
                            CHANGE_BAR_WIDTH,
                            m.line_height,
                            bar_color,
                        ));
                    }
                    emitted = true;
                }

                if !emitted {
                    let row_y = index_to_f32(vi) * m.line_height;
                    let y = m.top_inset + row_y + m.virtual_scroll_offset - m.scroll_y;
                    if y + m.line_height > 0.0 && y < m.surface_height {
                        self.cpu_gutter_change_quads.push(Quad::new(
                            m.left_inset + CHANGE_BAR_INSET,
                            y,
                            CHANGE_BAR_WIDTH,
                            m.line_height,
                            bar_color,
                        ));
                    }
                }
            }
        }
    }
}
