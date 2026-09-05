//! Explicit local frame extent, chrome, caret hints and typed embedding errors.

use iridium_editor::cell_layout::{CellLayoutError, CellPlacement, ScreenRow};

use super::{CellPosition, Chrome, TextArea};

/// Host-owned local allocation; no parent origin or terminal state is retained.
#[derive(Debug, Clone, Copy)]
pub struct CellFrameOptions<'a> {
    /// Actual local buffer width in cells.
    pub columns: usize,
    /// Actual local buffer height in cells.
    pub rows: usize,
    /// First document screen row visible in the text rectangle.
    pub first_row: ScreenRow,
    /// None is bare; Some preserves the existing chrome allocation rules.
    pub chrome: Option<Chrome<'a>>,
}

/// A logical caret edge and its bounded physical cell are distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellCaret {
    /// Cell in this local frame, never a parent or terminal coordinate.
    pub position: CellPosition,
    /// Unclipped logical edge in the document screen-row map.
    pub placement: CellPlacement,
    /// True when the logical edge lies beyond the last physical text cell.
    /// Such a hint is not an ordinary interior hit-test round trip.
    pub trailing_edge: bool,
}

/// Geometry used by every operation on one prepared local cell frame.
#[derive(Debug, Clone)]
pub struct CellFrameLayout {
    /// Local frame width.
    pub columns: usize,
    /// Local frame height.
    pub rows: usize,
    /// Left band width, zero in bare mode.
    pub sidebar_columns: usize,
    /// Gutter width, zero in bare mode.
    pub gutter_width: usize,
    /// Local text columns; wrapped mode has no horizontal scroll.
    pub text: TextArea,
    /// Number of visible text rows.
    pub text_rows: usize,
    /// Host-selected first document screen row.
    pub first_row: ScreenRow,
    /// Total document screen rows, including rows outside the local rectangle.
    pub total_rows: usize,
    /// Search panel allocation, when chrome is present and space permits.
    pub search_rows: Option<(usize, usize)>,
    /// Status row, absent in bare mode.
    pub status_row: Option<usize>,
    /// Primary document caret, when visible.
    pub primary_caret: Option<CellCaret>,
    /// Search-field caret, filled by painting its host-owned overlay.
    pub search_caret: Option<CellPosition>,
}

impl CellFrameLayout {
    /// Physical cursor position, with search focus taking precedence.
    #[must_use]
    pub fn caret(&self) -> Option<CellPosition> {
        self.search_caret
            .or_else(|| self.primary_caret.map(|caret| caret.position))
    }
}

/// A failed preparation or extent check leaves the destination untouched.
#[derive(Debug)]
pub enum CellFrameError {
    /// Kernel geometry rejected a coordinate or source line.
    Layout(CellLayoutError),
    /// Render destination differs from the allocation prepared by the host.
    ExtentMismatch {
        /// Prepared width and height.
        prepared: (usize, usize),
        /// Actual buffer width and height.
        buffer: (usize, usize),
    },
    /// A kernel cluster could not be represented by the terminal cell store.
    Cluster {
        /// Original document line.
        line: usize,
        /// Original byte offset within that line.
        byte: usize,
    },
}

impl From<CellLayoutError> for CellFrameError {
    fn from(error: CellLayoutError) -> Self {
        Self::Layout(error)
    }
}

impl std::fmt::Display for CellFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Layout(error) => write!(f, "{error}"),
            Self::ExtentMismatch { prepared, buffer } => write!(
                f,
                "prepared frame extent {}x{} differs from buffer {}x{}",
                prepared.0, prepared.1, buffer.0, buffer.1
            ),
            Self::Cluster { line, byte } => write!(
                f,
                "document line {line} cluster at byte {byte} cannot be stored in a cell"
            ),
        }
    }
}

impl std::error::Error for CellFrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Layout(error) => Some(error),
            Self::ExtentMismatch { .. } | Self::Cluster { .. } => None,
        }
    }
}
