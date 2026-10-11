//! Vertical motions share rendering's immutable fold mapping.
use crate::document::{Document, Position};
use crate::editor::FoldState;

use super::KeyboardHandler;
use super::motions::VerticalDirection;

impl KeyboardHandler {
    /// Called before both keyed and named-command dispatch; O(1) snapshot.
    pub(crate) fn sync_folds(&mut self, folds: &FoldState) {
        self.fold_mapping = folds.navigation_mapping();
    }

    /// Horizontal motions crossing a hidden body land on a visible line edge.
    pub(super) fn horizontal_fold_target(
        &self,
        document: &Document,
        head: Position,
        target: Position,
    ) -> Position {
        if target.line == head.line || !self.fold_mapping.is_hidden(target.line) {
            return target;
        }
        let header_row = self.fold_mapping.project_document(target.line);
        if target.line < head.line {
            let line = self.fold_mapping.visual_to_document(header_row);
            return Position::new(line, document.line_len(line).unwrap_or(0));
        }
        let line = self
            .fold_mapping
            .visual_to_document(header_row.saturating_add(1));
        if line >= document.line_count() {
            head
        } else {
            Position::new(line, 0)
        }
    }

    /// Move in visible rows while retaining each cursor's preferred column.
    pub(super) fn folded_vertical_move(
        &self,
        document: &Document,
        head: Position,
        preferred: usize,
        direction: VerticalDirection,
        lines: usize,
    ) -> (Position, usize) {
        let visual = self.fold_mapping.project_document(head.line);
        let last = document
            .line_count()
            .saturating_sub(self.fold_mapping.total_hidden())
            .saturating_sub(1);
        let target = match direction {
            VerticalDirection::Up => visual.saturating_sub(lines),
            VerticalDirection::Down => visual.saturating_add(lines).min(last),
        };
        let line = self.fold_mapping.visual_to_document(target);
        let width = document.line_len(line).unwrap_or(0);
        let sticky = match direction {
            VerticalDirection::Up if visual == 0 => 0,
            VerticalDirection::Down if visual >= last => width,
            _ => preferred,
        };
        let head = super::graphemes::move_to(
            document,
            Position::new(line, sticky.min(width)),
            super::graphemes::Boundary::Floor,
        );
        (head, sticky)
    }
}
