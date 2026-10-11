//! Host-measured vertical rows, dispatched through ordinary editing commands.
use super::{KeyboardHandler, motions::VerticalDirection, types::KeyResult};
use crate::editor::LineMapping;
use crate::{CursorState, Document, Position, Selection};
use std::{fmt::Debug, sync::Arc};

/// A backend supplies its measured rows without owning cursor or history state.
pub trait VisualNavigation: Debug + Send + Sync {
    /// Width and measurement settings; changing these invalidates sticky columns.
    fn layout_key(&self) -> (usize, usize);
    /// Move through visible rows, returning the target and preferred display column.
    ///
    /// # Errors
    /// A failed measurement rejects the entire multi-cursor motion.
    fn move_rows(
        &self,
        document: &Document,
        folds: &LineMapping,
        motion: VisualMotion,
    ) -> Result<(Position, usize), String>;
}

/// One cursor's requested movement in measured rows.
#[derive(Clone, Copy, Debug)]
pub struct VisualMotion {
    /// Source position.
    pub head: Position,
    /// Retained display column, or none for the first vertical motion.
    pub preferred: Option<usize>,
    /// True moves down; false moves up.
    pub down: bool,
    /// Number of visible rows to move.
    pub rows: usize,
}
impl KeyboardHandler {
    /// Replace layout measurement without ending an editing transaction.
    pub fn set_visual_navigation(&mut self, navigation: Option<Arc<dyn VisualNavigation>>) {
        if self.visual_navigation.as_ref().map(|n| n.layout_key())
            != navigation.as_ref().map(|n| n.layout_key())
        {
            self.reset_vertical_state();
        }
        self.visual_navigation = navigation;
    }
    pub(super) fn visual_motion(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        extend: bool,
        direction: VerticalDirection,
        rows: usize,
    ) -> KeyResult {
        match self.visual_targets(document, cursor, extend, direction, rows) {
            Ok(targets) => {
                let selections: Vec<_> = cursor
                    .all_selections()
                    .zip(&targets)
                    .map(|(sel, (head, _))| {
                        Selection::new(if extend { sel.anchor } else { *head }, *head)
                    })
                    .collect();
                let Some((primary, secondary)) = selections.split_first() else {
                    return KeyResult::Handled;
                };
                let next = if !extend && self.preserves_vertical_selection() {
                    CursorState {
                        primary: *primary,
                        secondary: secondary.to_vec(),
                    }
                } else {
                    CursorState::from_selections(*primary, secondary.to_vec())
                };
                if next.cursor_count() == cursor.cursor_count() {
                    self.store_sticky_columns(
                        &next,
                        targets.into_iter().map(|(_, col)| col).collect(),
                        document.revision(),
                    );
                } else {
                    self.reset_vertical_state();
                }
                Self::create_selection_command(cursor, &next)
            },
            Err(error) => KeyResult::Error(error),
        }
    }
    fn visual_targets(
        &self,
        document: &Document,
        cursor: &CursorState,
        extend: bool,
        direction: VerticalDirection,
        rows: usize,
    ) -> Result<Vec<(Position, usize)>, String> {
        let navigation = self
            .visual_navigation
            .as_ref()
            .ok_or("visual navigation is not installed")?;
        let reusable = self.has_sticky_columns(cursor, document.revision());
        cursor
            .all_selections()
            .enumerate()
            .map(|(index, sel)| {
                let preferred = reusable
                    .then(|| {
                        self.preferred_columns
                            .as_ref()
                            .and_then(|columns| columns.get(index))
                            .copied()
                    })
                    .flatten();
                navigation.move_rows(
                    document,
                    &self.fold_mapping,
                    VisualMotion {
                        head: self.vertical_origin(sel, extend),
                        preferred,
                        down: matches!(direction, VerticalDirection::Down),
                        rows,
                    },
                )
            })
            .collect()
    }
}
