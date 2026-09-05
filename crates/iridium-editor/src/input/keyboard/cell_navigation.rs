//! Visual-row motions and transient cell columns outside document/history state.

use crate::cell_layout::{Affinity, CellColumn, CellHit, CellRowMap, ScreenRow};
use crate::document::{CursorState, Selection};
use crate::editor::{CellInputError, CellInputOptions, EditorState};

use super::actions::KeyboardAction;
use super::cell_input::CellInputContext;
use super::{KeyResult, KeyboardHandler};

use KeyboardAction as Action;

#[derive(Debug, Clone, Copy)]
struct CursorMemory {
    selection: Selection,
    affinity: Affinity,
    preferred: Option<CellColumn>,
}

/// Scoped to one `KeyboardHandler` owner, and eagerly cleared on host mutation.
#[derive(Debug)]
pub(super) struct CellNavigationState {
    cursor: CursorState,
    document: u64,
    revision: u64,
    folds: u64,
    options: CellInputOptions,
    memories: Vec<CursorMemory>,
}

impl CellNavigationState {
    fn matches(&self, cell: &CellInputContext<'_>) -> bool {
        self.cursor == *cell.cursor
            && self.document == cell.document.id()
            && self.revision == cell.document.revision()
            && self.folds == cell.folds.generation()
            && self.options == cell.options
    }

    fn memory(&self, selection: Selection) -> Option<CursorMemory> {
        self.memories
            .iter()
            .find(|memory| memory.selection == selection)
            .copied()
    }
}

#[derive(Clone, Copy)]
enum RowMotion {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

#[derive(Clone, Copy)]
pub(super) struct CellMotion {
    motion: RowMotion,
    extend: bool,
}

impl CellMotion {
    pub(super) const fn for_action(action: KeyboardAction) -> Option<Self> {
        let (motion, extend) = match action {
            Action::LineUp => (RowMotion::Up, false),
            Action::LineUpSelect => (RowMotion::Up, true),
            Action::LineDown => (RowMotion::Down, false),
            Action::LineDownSelect => (RowMotion::Down, true),
            Action::PageUp => (RowMotion::PageUp, false),
            Action::PageUpSelect => (RowMotion::PageUp, true),
            Action::PageDown => (RowMotion::PageDown, false),
            Action::PageDownSelect => (RowMotion::PageDown, true),
            Action::LineStart => (RowMotion::Home, false),
            Action::LineStartSelect => (RowMotion::Home, true),
            Action::LineEnd => (RowMotion::End, false),
            Action::LineEndSelect => (RowMotion::End, true),
            _ => return None,
        };
        Some(Self { motion, extend })
    }
}

impl KeyboardHandler {
    pub(super) fn cell_motion(
        &mut self,
        motion: CellMotion,
        count: u32,
        cell: &CellInputContext<'_>,
    ) -> Result<KeyResult, CellInputError> {
        if cell.options.visible_rows == 0
            && matches!(motion.motion, RowMotion::PageUp | RowMotion::PageDown)
        {
            self.reset_cell_state();
            return Ok(KeyResult::Handled);
        }
        let map = CellRowMap::prepare(cell.document, cell.folds, cell.options.wrap)?;
        let prior = self
            .cell_navigation
            .as_ref()
            .filter(|state| !self.cell_geometry_dirty && state.matches(cell));
        let hops = usize::try_from(count).unwrap_or(usize::MAX);
        let mut memories = Vec::with_capacity(cell.cursor.cursor_count());
        let mut new_cursor = cell.cursor.clone();
        for selection in new_cursor.all_selections_mut() {
            let memory = prior.and_then(|state| state.memory(*selection));
            let affinity = memory.map_or(Affinity::Downstream, |value| value.affinity);
            let Some(placement) = map.place(selection.head, affinity)? else {
                continue;
            };
            let preferred = memory
                .and_then(|value| value.preferred)
                .unwrap_or(placement.column);
            let hit = row_target(
                &map,
                motion.motion,
                placement.row,
                preferred,
                hops,
                cell.options.visible_rows,
            )?;
            let anchor = if motion.extend {
                selection.anchor
            } else {
                hit.position
            };
            *selection = Selection::new(anchor, hit.position);
            memories.push(CursorMemory {
                selection: *selection,
                affinity: hit.affinity,
                preferred: if matches!(motion.motion, RowMotion::Home | RowMotion::End) {
                    None
                } else {
                    Some(preferred)
                },
            });
        }
        let mut merged = CursorState::new(new_cursor.primary);
        for selection in new_cursor.secondary {
            merged.add_cursor(selection);
        }
        let new_cursor = merged;
        self.cell_geometry_dirty = false;
        self.cell_navigation = Some(CellNavigationState {
            cursor: new_cursor.clone(),
            document: cell.document.id(),
            revision: cell.document.revision(),
            folds: cell.folds.generation(),
            options: cell.options,
            memories,
        });
        Ok(Self::create_selection_command(cell.cursor, &new_cursor))
    }

    pub(crate) fn remember_cell_pointer(
        &mut self,
        state: &EditorState,
        options: CellInputOptions,
        affinity: Affinity,
    ) {
        self.cell_geometry_dirty = false;
        self.cell_navigation = Some(CellNavigationState {
            cursor: state.cursor.clone(),
            document: state.document.id(),
            revision: state.document.revision(),
            folds: state.fold_state.generation(),
            options,
            memories: vec![CursorMemory {
                selection: state.cursor.primary,
                affinity,
                preferred: None,
            }],
        });
    }

    pub(crate) fn cell_affinity(&self, state: &EditorState, options: CellInputOptions) -> Affinity {
        self.cell_cursor_affinity(state, options, 0)
            .map_or(Affinity::Downstream, |value| value)
    }

    pub(crate) fn cell_cursor_affinity(
        &self,
        state: &EditorState,
        options: CellInputOptions,
        index: usize,
    ) -> Option<Affinity> {
        let selection = state.cursor.all_selections().nth(index)?;
        Some(
            self.cell_navigation
                .as_ref()
                .filter(|memory| {
                    !self.cell_geometry_dirty
                        && memory.cursor == state.cursor
                        && memory.document == state.document.id()
                        && memory.revision == state.document.revision()
                        && memory.folds == state.fold_state.generation()
                        && memory.options == options
                })
                .and_then(|memory| memory.memory(*selection))
                .map_or(Affinity::Downstream, |memory| memory.affinity),
        )
    }
}

fn row_target(
    map: &CellRowMap<'_>,
    motion: RowMotion,
    row: ScreenRow,
    column: CellColumn,
    hops: usize,
    page_rows: usize,
) -> Result<CellHit, CellInputError> {
    let distance = if matches!(motion, RowMotion::PageUp | RowMotion::PageDown) {
        hops.saturating_mul(page_rows)
    } else {
        hops
    };
    let target = match motion {
        RowMotion::Up | RowMotion::PageUp => ScreenRow(row.0.saturating_sub(distance)),
        RowMotion::Down | RowMotion::PageDown => ScreenRow(
            row.0
                .saturating_add(distance)
                .min(map.total_rows().saturating_sub(1)),
        ),
        RowMotion::Home | RowMotion::End => row,
    };
    let hit = match motion {
        RowMotion::Home => {
            let row = map.row(target)?;
            CellHit {
                position: crate::document::Position::new(
                    row.document_line(),
                    row.scalar_range().start,
                ),
                affinity: Affinity::Downstream,
            }
        },
        RowMotion::End => {
            let row = map.row(target)?;
            CellHit {
                position: crate::document::Position::new(
                    row.document_line(),
                    row.scalar_range().end,
                ),
                affinity: Affinity::Upstream,
            }
        },
        _ => map.position_at(target, column)?,
    };
    Ok(hit)
}
