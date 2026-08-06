//! Composing the explorer into rows the overlay can paint.
//!
//! Split from `panel` by the question each method answers: nothing here reads
//! a key or changes what is open. A defect in it presents as "the wrong thing
//! is on screen" — a row missing, a window that will not follow the
//! selection, a caret in the wrong column.

use iridium_editor::theme::Theme;

use super::panel::{FileExplorer, PROMPT};
use super::rows::{RowShape, entry_row};
use crate::line::{LineBuilder, skip_chars};
use crate::overlay::scroll_for;
use crate::overlay::{
    PANEL_MAX_VISIBLE_ROWS, PanelAnchor, PanelCaret, PanelContent, PanelFit, PanelRow, Span,
};

impl FileExplorer {
    /// Composes the panel for painting: the query row, then the visible slice
    /// of whichever list is showing.
    ///
    /// `&mut self` because composition is where the scroll window follows the
    /// selection and the page size is learned, exactly as in the palette.
    pub fn content(&mut self, theme: &Theme, fit: PanelFit) -> PanelContent {
        let total = self.row_count();
        // At least one row, so a tree whose root has not listed yet still
        // says "Reading…" rather than composing an empty panel; and never
        // more than the panel's own limit or the window's, less the query
        // row that is always drawn.
        let ceiling = PANEL_MAX_VISIBLE_ROWS.min(fit.max_interior_rows.saturating_sub(1).max(1));
        let visible = total.clamp(1, ceiling);
        self.follow_selection(total, visible);

        let mut rows = Vec::with_capacity(visible + 1);
        let (input, caret_column) = self.input_row(theme, fit.content_columns);
        rows.push(input);
        for offset in 0..visible {
            rows.push(self.list_row(self.scroll + offset, theme, fit.content_columns));
        }

        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: fit.content_columns,
            rows,
            caret: caret_column.map(|column| PanelCaret { row: 0, column }),
        }
    }

    /// How many rows the list under the query has.
    fn row_count(&self) -> usize {
        if self.is_filtering() {
            self.view.rows.len()
        } else {
            self.tree.len()
        }
    }

    /// The selected row's index within the list under the query.
    fn selected_row(&self) -> Option<usize> {
        if self.is_filtering() {
            Some(self.filtered)
        } else {
            self.tree.selected()
        }
    }

    /// Composes the query row and reports where its caret landed.
    fn input_row(&self, theme: &Theme, width: usize) -> (PanelRow, Option<usize>) {
        let mut line = LineBuilder::new(width);
        line.push(PROMPT, theme.editor.line_number);
        let prompt_chars = PROMPT.chars().count();
        if width <= prompt_chars {
            return (PanelRow::new(line.finish()), None);
        }
        let field_width = width - prompt_chars;
        let caret = self.query.caret_column();
        let scroll = scroll_for(caret, field_width);
        let shown: String = skip_chars(self.query.text(), scroll)
            .chars()
            .take(field_width)
            .collect();
        line.push(&shown, theme.editor.foreground);
        (
            PanelRow::new(line.finish()),
            Some(prompt_chars + caret - scroll),
        )
    }

    /// What an empty result list says.
    ///
    /// **Three different statements, because they mean three different
    /// things.** A search still reading directories has not failed to find
    /// anything — it has not looked everywhere yet, and saying "no matching
    /// files" while the crawl is running is a lie that resolves itself a
    /// second later, which is exactly long enough for someone to have given
    /// up. A search that stopped at its limit has genuinely not looked
    /// everywhere and never will, and that is worth saying out loud rather
    /// than passing off as an answer.
    fn nothing_found_yet(&self) -> &'static str {
        if !self.files.is_fully_crawled() {
            return "Searching…";
        }
        if self.files.crawl_hit_its_limit() {
            return "No matches — the search stopped at its limit";
        }
        "No matching files"
    }

    /// One row of whichever list is showing.
    fn list_row(&self, index: usize, theme: &Theme, width: usize) -> PanelRow {
        if self.is_filtering() {
            let Some(row) = self.view.rows.get(index) else {
                return PanelRow::new(vec![Span::new(
                    self.nothing_found_yet(),
                    theme.editor.line_number,
                )]);
            };
            let shape = RowShape {
                depth: row.depth,
                // Every folder in a filtered view is showing what is inside
                // it, so the disclosure points down — and no folder is drawn
                // at all unless something under it matched.
                has_children: self.files.info(row.id).is_some_and(|info| {
                    info.kind.is_expandable() && !self.files.listed_children(row.id).is_empty()
                }),
                expanded: true,
                selected: index == self.filtered && row.is_selectable(),
                positions: row.positions.clone(),
            };
            return entry_row(&self.files, row.id, &shape, theme, width);
        }

        let Some(row) = self.tree.row(index) else {
            // Only reachable for a tree with no rows at all, which is a root
            // whose read has not landed yet.
            return PanelRow::new(vec![Span::new("Reading…", theme.editor.line_number)]);
        };
        let shape = RowShape {
            depth: row.depth,
            has_children: row.has_children,
            expanded: row.expanded,
            selected: self.tree.selected() == Some(index),
            positions: Vec::new(),
        };
        entry_row(&self.files, row.id, &shape, theme, width)
    }

    /// Slides the window so the selection is inside it, and never past the
    /// end of the rows.
    ///
    /// The clamp is applied on the way out as well as the way in: a refresh
    /// that removed rows can leave a scroll pointing past the end, and a
    /// window starting past the last row draws nothing at all.
    fn follow_selection(&mut self, total: usize, visible: usize) {
        let last_start = total.saturating_sub(visible);
        if let Some(selected) = self.selected_row() {
            if selected < self.scroll {
                self.scroll = selected;
            } else if visible > 0 && selected >= self.scroll + visible {
                self.scroll = selected + 1 - visible;
            }
        }
        self.scroll = self.scroll.min(last_start);
    }
}
