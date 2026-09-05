//! Distinct screen coordinates and explicit failures for borrowed cell geometry.

use crate::document::Position;

/// A zero-based screen row, independent of document and fold-visible lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScreenRow(pub usize);

/// A cell offset within a screen row, including its trailing edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellColumn(pub usize);

/// Which side of a soft break a document boundary occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affinity {
    /// The preceding row's trailing edge.
    Upstream,
    /// The following row's leading edge; the canonical soft-break placement.
    Downstream,
}

/// Explicit measurement inputs. This does not change editor configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellWrapParameters {
    columns: usize,
    tab_width: usize,
}

impl CellWrapParameters {
    /// Selects the width and tab stops. Zero columns produces no screen rows.
    ///
    /// A zero tab width becomes one, preserving the existing terminal rule.
    pub const fn new(columns: usize, tab_width: usize) -> Self {
        Self {
            columns,
            tab_width: if tab_width == 0 { 1 } else { tab_width },
        }
    }

    /// Available cells in each screen row.
    pub const fn columns(self) -> usize {
        self.columns
    }

    /// Effective distance between tab stops, always positive.
    pub const fn tab_width(self) -> usize {
        self.tab_width
    }
}

/// A logical caret placement; its column may equal or exceed the visible width.
///
/// The terminal face must represent a trailing edge or oversized atom explicitly
/// when choosing a bounded physical caret cell. This is not a terminal cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPlacement {
    /// The screen row containing this edge.
    pub row: ScreenRow,
    /// The logical edge measured from that row's origin.
    pub column: CellColumn,
    /// The selected side of the soft break.
    pub affinity: Affinity,
}

/// The canonical document boundary corresponding to a read-only cell query.
///
/// This is a query result, not authority to apply a later mutation. Cell-mode
/// input prepares current geometry inside the editor before applying commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellHit {
    /// A complete grapheme boundary in existing scalar coordinates.
    pub position: Position,
    /// The selected side of a soft break.
    pub affinity: Affinity,
}

/// An invalid source position, row request or unrepresentable cell measurement.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CellLayoutError {
    /// The requested source line or scalar column does not exist.
    #[error("cell layout position {line}:{column} is outside the document")]
    InvalidPosition {
        /// Requested document line.
        line: usize,
        /// Requested Unicode-scalar column.
        column: usize,
    },
    /// A scalar position splits an extended grapheme cluster.
    #[error("cell layout position {line}:{column} is inside a grapheme cluster")]
    NonGraphemeBoundary {
        /// Requested document line.
        line: usize,
        /// Requested Unicode-scalar column.
        column: usize,
    },
    /// There is no row at the requested index.
    #[error("cell layout row {row} is outside its {total_rows} screen rows")]
    InvalidRow {
        /// Requested screen row.
        row: usize,
        /// Measured number of screen rows.
        total_rows: usize,
    },
    /// A line passed to the within-line measurer contains a real line break.
    #[error("cell line measurement contains a line ending at byte {byte}")]
    LineEnding {
        /// Byte offset of the line break.
        byte: usize,
    },
    /// The requested cell extent cannot be represented by this machine.
    #[error("cell measurement overflows on document line {line}")]
    CellOverflow {
        /// The line being measured; zero for a standalone line measurement.
        line: usize,
    },
}
