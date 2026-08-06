//! The rectangles drawn behind and over the text: line backgrounds,
//! selection highlights and carets.
//!
//! The gutter's change bars are the fourth builder and live in
//! [`super::gutter_column`] with the rest of the gutter's geometry.

use glyphon::Buffer;

use super::metrics::FrameMetrics;
use super::state::FrameCompositor;
use crate::editor::Editor;
use crate::render::quad::Quad;
use crate::render::units::index_to_f32;

impl FrameCompositor {
    /// Emits line background quads for diff highlighting.
    ///
    /// These render behind selection highlights so diffs are visible even
    /// when selected.
    pub(super) fn build_line_background_quads(&mut self, m: &FrameMetrics) {
        self.cpu_line_bg_quads.clear();
        if self.line_backgrounds.is_empty() {
            return;
        }
        for (vi, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
            if let Some(&bg_color) = self.line_backgrounds.get(&doc_line) {
                // Walk the visual line map to find which visual rows correspond
                // to this logical line (accounts for wrapping).
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
                        self.cpu_line_bg_quads.push(Quad::new(
                            m.content_offset_x,
                            y,
                            m.surface_width - m.content_offset_x,
                            m.line_height,
                            bg_color,
                        ));
                    }
                    emitted = true;
                }

                // Fallback: if visual line map wasn't built yet, use simple position
                if !emitted {
                    let row_y = index_to_f32(vi) * m.line_height;
                    let y = m.top_inset + row_y + m.virtual_scroll_offset - m.scroll_y;
                    if y + m.line_height > 0.0 && y < m.surface_height {
                        self.cpu_line_bg_quads.push(Quad::new(
                            m.content_offset_x,
                            y,
                            m.surface_width - m.content_offset_x,
                            m.line_height,
                            bg_color,
                        ));
                    }
                }
            }
        }
    }

    /// Emits selection highlight quads (rendered before text).
    ///
    /// Uses the visual line map for wrap-aware positioning so that selection
    /// highlights align with text even when lines wrap.
    ///
    /// Every cursor's selection is drawn, not only the primary's. Drawing
    /// just the primary is what made multi-cursor invisible in the web face:
    /// the kernel held N selections and the screen showed one, so a command
    /// that worked perfectly looked like a command that did nothing.
    pub(super) fn build_selection_quads(
        &mut self,
        editor: &Editor,
        buffer: &Buffer,
        m: &FrameMetrics,
    ) {
        self.cpu_selection_quads.clear();
        let selection_color = self.theme.editor.selection;
        for selection in editor.state().cursor.all_selections() {
            if selection.is_collapsed() {
                continue;
            }
            let sel_start = selection.start();
            let sel_end = selection.end();

            for doc_line in sel_start.line..=sel_end.line {
                // Skip hidden/folded lines
                let Some(vis_line) = self.cpu_doc_to_visual.get(doc_line).and_then(|v| *v) else {
                    continue;
                };

                let line_content = editor.state().document.line(doc_line);
                let line_len = line_content.map_or(0, |l| l.chars().count());

                // Determine selection columns for this line
                let sel_col_start = if doc_line == sel_start.line {
                    sel_start.column
                } else {
                    0
                };
                let sel_col_end = if doc_line == sel_end.line {
                    sel_end.column
                } else {
                    line_len
                };

                // Extra width for newline visualization on non-final lines
                let wants_newline_extra = doc_line != sel_end.line && sel_col_end == line_len;
                let newline_extra = if wants_newline_extra {
                    m.char_width * 0.5
                } else {
                    0.0
                };

                if sel_col_start >= sel_col_end && !wants_newline_extra {
                    continue;
                }

                // Get buffer-relative line index for this doc line
                let buffer_line = vis_line.saturating_sub(m.viewport_start_visual);

                // Walk the visual line map to find which visual rows this buffer line
                // spans and emit a quad for each wrapped segment that overlaps the selection.
                let mut handled = false;
                for (vi, &(buf_idx, run_start_col)) in
                    self.cached_visual_line_map.iter().enumerate()
                {
                    if buf_idx != buffer_line {
                        continue;
                    }

                    // Determine the end column of this visual segment
                    let run_end_col = self
                        .cached_visual_line_map
                        .get(vi + 1)
                        .filter(|(next_buf, _)| *next_buf == buffer_line)
                        .map_or(line_len, |(_, next_start)| *next_start);

                    // Intersect this segment with the selection range
                    let overlap_start = sel_col_start.max(run_start_col);
                    let overlap_end = sel_col_end.min(run_end_col);

                    // Extra width only applies on the last segment of the line
                    let seg_extra = if run_end_col >= line_len {
                        newline_extra
                    } else {
                        0.0
                    };

                    if overlap_start < overlap_end
                        || (overlap_start == overlap_end && seg_extra > 0.0)
                    {
                        let seg_x = index_to_f32(overlap_start - run_start_col) * m.char_width;
                        let x = m.content_offset_x + seg_x;
                        let row_y = index_to_f32(vi) * m.line_height;
                        let y = m.top_inset + row_y + m.virtual_scroll_offset - m.scroll_y;
                        let seg_width = index_to_f32(overlap_end - overlap_start) * m.char_width;
                        let width = seg_width + seg_extra;

                        if y + m.line_height > 0.0 && y < m.surface_height {
                            self.cpu_selection_quads.push(Quad::new(
                                x,
                                y,
                                width,
                                m.line_height,
                                selection_color,
                            ));
                        }
                        handled = true;
                    }
                }

                // Fallback for lines not in the visual map (shouldn't happen, but safe)
                if !handled {
                    let (sx, sy) = self.text_renderer.cursor_position_in_buffer(
                        buffer,
                        buffer_line,
                        sel_col_start,
                        m.char_width,
                    );
                    let x = m.content_offset_x + sx;
                    let y = m.top_inset + sy + m.virtual_scroll_offset - m.scroll_y;
                    let sel_width = index_to_f32(sel_col_end - sel_col_start) * m.char_width;
                    let width = sel_width + newline_extra;

                    if y + m.line_height > 0.0 && y < m.surface_height {
                        self.cpu_selection_quads.push(Quad::new(
                            x,
                            y,
                            width,
                            m.line_height,
                            selection_color,
                        ));
                    }
                }
            }
        }
    }

    /// Emits a cursor quad (2px wide line) for **every** caret.
    ///
    /// The primary's vertical position is computed in
    /// [`Self::compose`] and kept there, because scrolling must keep
    /// following the primary alone. The others are derived here the same way.
    ///
    /// Blink is shared on purpose: carets blinking out of phase read as a
    /// rendering fault rather than as one multi-cursor edit.
    pub(super) fn build_cursor_quads(
        &mut self,
        editor: &Editor,
        buffer: &Buffer,
        m: &FrameMetrics,
    ) {
        let cursor_color = self.theme.editor.cursor;
        self.cpu_cursor_quads.clear();
        if !self.cursor_renderer.is_visible() {
            return;
        }
        for selection in editor.state().cursor.all_selections() {
            let head = selection.head;
            let head_visual_line = self
                .cpu_doc_to_visual
                .get(head.line)
                .and_then(|v| *v)
                .unwrap_or(0);
            let head_line_in_buffer = head_visual_line.saturating_sub(m.viewport_start_visual);
            let (head_x, head_y) = self.text_renderer.cursor_position_in_buffer(
                buffer,
                head_line_in_buffer,
                head.column,
                m.char_width,
            );
            let x = m.content_offset_x + head_x;
            let y = m.top_inset + head_y + m.virtual_scroll_offset - m.scroll_y;
            // Off-screen carets are skipped, exactly as selection quads are.
            if y + m.line_height > 0.0 && y < m.surface_height {
                self.cpu_cursor_quads
                    .push(Quad::new(x, y, 2.0, m.line_height, cursor_color));
            }
        }
    }
}
