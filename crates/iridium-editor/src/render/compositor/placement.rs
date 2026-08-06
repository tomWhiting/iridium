//! What runs *between* frames: scroll limits, both directions of
//! hit-testing, and the readbacks they answer from.
//!
//! Everything here is a query, and every query's honesty depends on agreeing
//! with the painter. Where a value is computed in both places it is computed
//! *once*, here, and the painter calls it —
//! [`FrameCompositor::content_left_edge_past_gutter`] is that discipline made
//! concrete.

use super::metrics::{BOTTOM_SLACK, HORIZONTAL_PADDING};
use super::state::FrameCompositor;
use crate::editor::{Editor, FoldState};
use crate::render::units::{index_to_f32, pixel_to_index};

impl FrameCompositor {
    /// The maximum vertical scroll offset in pixels.
    ///
    /// Uses the wrap-aware visual line total from the last composed frame;
    /// before the first frame it falls back to the fold-aware count, which
    /// knows nothing of wrapping — exactly the pre-extraction behavior.
    pub fn max_scroll_y(
        &self,
        editor: &Editor,
        fold_state: &FoldState,
        viewport_height: f32,
    ) -> f32 {
        let line_height = self.text_renderer.line_height();
        let total_visual = if self.cached_total_visual_lines > 0 {
            self.cached_total_visual_lines
        } else {
            // Before first render, fall back to fold-aware count (no wrapping info)
            fold_state.visible_line_count(editor.state().document.line_count())
        };
        let content_height = index_to_f32(total_visual) * line_height;
        // The document starts at `top_inset`, so the last row's bottom sits
        // that much further down; the extra slack below is what stops the
        // final line hugging the window edge. The single `20.0` this
        // replaces was those two tens added together, which agreed with the
        // truth only while the inset stayed at its default.
        (content_height + self.top_inset - viewport_height + BOTTOM_SLACK).max(0.0)
    }

    /// The primary caret's absolute Y position in document space, for
    /// scroll-to-caret decisions.
    ///
    /// Uses the cached wrap-aware position from the last frame when the
    /// caret is still on the same line (the common case: typing on one
    /// line); falls back to fold-only estimation when it has moved.
    pub fn cursor_anchor_y(&self, editor: &Editor, fold_state: &FoldState) -> f32 {
        let line_height = self.text_renderer.line_height();
        let top_inset = self.top_inset;
        let cursor_line = editor.cursor().line;
        if cursor_line == self.cached_cursor_doc_line && self.cached_cursor_abs_y > 0.0 {
            // Cursor on the same line as last render — use cached position
            // (accounts for wrapping within this line)
            self.cached_cursor_abs_y
        } else {
            // Cursor moved to a different line — approximate using fold mapping
            let visual_line = fold_state.document_to_visual_line(cursor_line).unwrap_or(0);
            let row_y = index_to_f32(visual_line) * line_height;
            top_inset + row_y
        }
    }

    /// Converts pixel coordinates to a (line, column) document position,
    /// accounting for folded lines, scroll, and line wrapping.
    ///
    /// Runs on the input hot path (every mouse move), off the visual line map
    /// cached by the last [`Self::compose`]. Before the first frame it falls
    /// back to a fold-only calculation with no wrap awareness — exactly the
    /// pre-extraction behavior, rounding included.
    pub fn pixel_to_position(
        &self,
        editor: &Editor,
        fold_state: &FoldState,
        scroll_y: f32,
        x: f32,
        y: f32,
    ) -> (usize, usize) {
        let line_height = self.text_renderer.line_height();
        let char_width = self.cached_char_width;
        let top_inset = self.top_inset;

        let doc = &editor.state().document;
        let line_count = doc.line_count();

        if self.cached_visual_line_map.is_empty() {
            // Fallback: no cached map (before first render), use simple calculation
            let visual_line = pixel_to_index(((y + scroll_y - top_inset) / line_height).max(0.0));
            let doc_line = fold_state
                .visual_to_document_line(visual_line)
                .min(line_count.saturating_sub(1));

            let offset_x = self.content_left_edge(line_count);
            let line_len = doc.line(doc_line).map_or(0, |l| l.chars().count());
            let column = pixel_to_index(((x - offset_x) / char_width + 0.5).max(0.0));
            let clamped_column = column.min(line_len);

            return (doc_line, clamped_column);
        }

        // Calculate the text area top offset (must match compose positioning)
        let virtual_scroll_offset = index_to_f32(self.cached_map_viewport_start) * line_height;
        let adjusted_scroll_y = scroll_y - virtual_scroll_offset;
        let text_area_top = top_inset - adjusted_scroll_y;

        // Calculate which visual line in the buffer was clicked
        let y_in_buffer = y - text_area_top;
        let visual_line_in_buffer = pixel_to_index((y_in_buffer / line_height).floor().max(0.0));

        // Use the cached visual line map to resolve the click position
        // Clamp to last visual line in the map
        let clamped_visual =
            visual_line_in_buffer.min(self.cached_visual_line_map.len().saturating_sub(1));
        let (buffer_line_idx, run_start_col) = self.cached_visual_line_map[clamped_visual];

        // Map buffer line index to document line
        let doc_line = self
            .cpu_visible_doc_lines
            .get(buffer_line_idx)
            .copied()
            .unwrap_or(0)
            .min(line_count.saturating_sub(1));

        // Calculate column within this wrap segment
        let col_in_run =
            pixel_to_index(((x - self.cached_content_offset_x) / char_width + 0.5).max(0.0));
        let column = run_start_col + col_in_run;

        // Clamp to actual line length
        let line_len = doc.line(doc_line).map_or(0, |l| l.chars().count());
        let clamped_column = column.min(line_len);

        (doc_line, clamped_column)
    }

    /// Converts a document (line, column) to pixel coordinates — the inverse
    /// of [`Self::pixel_to_position`], off the same cached visual line map.
    ///
    /// Returns `None` when the line is hidden by a fold or outside the
    /// composed viewport.
    pub fn position_to_pixel(
        &self,
        editor: &Editor,
        fold_state: &FoldState,
        scroll_y: f32,
        doc_line: usize,
        column: usize,
    ) -> Option<(f32, f32)> {
        let line_height = self.text_renderer.line_height();
        let char_width = self.cached_char_width;
        let top_inset = self.top_inset;

        // Check if line is folded (hidden)
        if fold_state.is_line_hidden(doc_line) {
            return None;
        }

        if self.cached_visual_line_map.is_empty() {
            // Fallback: before first render, use simple calculation (no word wrap)
            let visual_line = fold_state.document_to_visual_line(doc_line)?;
            let offset_x = self.content_left_edge(editor.state().document.line_count());
            let column_x = index_to_f32(column) * char_width;
            let x = offset_x + column_x;
            let row_y = index_to_f32(visual_line) * line_height;
            let y = top_inset + row_y - scroll_y;
            return Some((x, y));
        }

        // Find the buffer line index for this document line
        let buf_idx = self
            .cpu_visible_doc_lines
            .iter()
            .position(|&dl| dl == doc_line)?;

        // Find the visual line that contains this column
        // (handles word wrap — a single document line may span multiple visual lines)
        let mut target_visual = None;
        let mut col_in_segment = column;

        for (visual_idx, &(bi, run_start)) in self.cached_visual_line_map.iter().enumerate() {
            if bi == buf_idx {
                // Check if this is the last segment for this buffer line
                let next_run_start = self
                    .cached_visual_line_map
                    .get(visual_idx + 1)
                    .filter(|&&(next_bi, _)| next_bi == buf_idx)
                    .map(|&(_, start)| start);

                if let Some(next_start) = next_run_start {
                    if column >= run_start && column < next_start {
                        target_visual = Some(visual_idx);
                        col_in_segment = column - run_start;
                        break;
                    }
                } else if column >= run_start {
                    // Last (or only) segment — column must be here
                    target_visual = Some(visual_idx);
                    col_in_segment = column - run_start;
                    break;
                }
            }
        }

        // A miss here means the document line is not in the current viewport.
        let visual_idx = target_visual?;
        let virtual_scroll_offset = index_to_f32(self.cached_map_viewport_start) * line_height;
        let seg_x = index_to_f32(col_in_segment) * char_width;
        let x = self.cached_content_offset_x + seg_x;
        let row_y = index_to_f32(visual_idx) * line_height;
        let y = top_inset + row_y + virtual_scroll_offset - scroll_y;
        Some((x, y))
    }

    /// The content column's left edge for a document of `line_count` lines:
    /// the reserved inset, the gutter's width, then the document's own
    /// horizontal breathing room.
    ///
    /// Computed rather than read back from the last frame, so it answers
    /// before one has been composed — which is why it exists alongside
    /// [`Self::content_offset_x`], and why the faces call it instead of
    /// rebuilding the sum. A face that rebuilds it hardcodes the padding and
    /// forgets the inset, and then agrees with the painter exactly until
    /// something is reserved.
    #[must_use]
    pub fn content_left_edge(&self, line_count: usize) -> f32 {
        self.content_left_edge_past_gutter(self.gutter_width(line_count))
    }

    /// The content column's left edge given the gutter's width.
    ///
    /// The one place the inset and the padding are added, so the only thing
    /// that can differ between the painter and a between-frames query is the
    /// gutter width itself — which they measure deliberately differently, and
    /// which is documented where they do.
    pub(super) const fn content_left_edge_past_gutter(&self, gutter_width: f32) -> f32 {
        self.left_inset + gutter_width + HORIZONTAL_PADDING
    }

    /// The line height in pixels, derived from the font configuration.
    pub fn line_height(&self) -> f32 {
        self.text_renderer.line_height()
    }

    /// The measured character advance width in pixels.
    pub const fn char_width(&self) -> f32 {
        self.cached_char_width
    }

    /// The wrap readback from the last frame: one entry per visual line,
    /// mapping to (`buffer_line_index`, `start_column_in_line`). Empty before
    /// the first frame.
    pub fn visual_line_map(&self) -> &[(usize, usize)] {
        &self.cached_visual_line_map
    }

    /// The first document line included when the visual line map was built.
    pub const fn map_viewport_start(&self) -> usize {
        self.cached_map_viewport_start
    }

    /// The content column's left edge when the visual line map was built.
    pub const fn content_offset_x(&self) -> f32 {
        self.cached_content_offset_x
    }

    /// Total visual lines including wrapped sub-lines, from the last frame.
    pub const fn total_visual_lines(&self) -> usize {
        self.cached_total_visual_lines
    }

    /// The primary caret's absolute Y (document space) from the last frame.
    pub const fn cursor_abs_y(&self) -> f32 {
        self.cached_cursor_abs_y
    }

    /// The caret's document line when [`Self::cursor_abs_y`] was computed.
    pub const fn cursor_doc_line(&self) -> usize {
        self.cached_cursor_doc_line
    }

    /// The document lines present in the last composed viewport, in order.
    pub fn visible_doc_lines(&self) -> &[usize] {
        &self.cpu_visible_doc_lines
    }

    /// How many times [`Self::compose`] has run the retained-shape rebuild
    /// path since this compositor was created.
    ///
    /// The observable behind every retention claim: a frame whose shaping
    /// inputs are unchanged must not move it, and a frame whose inputs
    /// changed must. Exists so the cache is testable without reading
    /// pixels — the exact pattern of the desktop face's
    /// `HighlightCache::rebuilds`.
    #[must_use]
    pub const fn shape_rebuilds(&self) -> u64 {
        self.shape_rebuilds
    }

    /// How many buffer lines the per-line diffing rebuild path has
    /// actually reshaped, across the content and gutter buffers, since
    /// this compositor was created.
    ///
    /// Full rebuilds do not move it — it counts only the diffed path's
    /// work, so "a one-character keystroke reshapes one line" is a
    /// testable claim rather than an asserted one.
    #[must_use]
    pub const fn lines_reshaped(&self) -> u64 {
        self.lines_reshaped
    }
}
