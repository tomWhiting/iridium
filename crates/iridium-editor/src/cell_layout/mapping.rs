//! Canonical cell hits and grapheme-boundary placements on a borrowed row map.

use unicode_segmentation::UnicodeSegmentation;

use crate::document::Position;

use super::rows::{CellRow, CellRowMap};
use super::types::{Affinity, CellColumn, CellHit, CellLayoutError, CellPlacement, ScreenRow};

impl CellRowMap<'_> {
    /// Places a complete grapheme boundary, selecting its soft-break side.
    ///
    /// `None` means an empty-width layout or a valid hidden source position.
    /// Scalar-interior positions are errors even when their line is hidden.
    /// At a shared boundary Downstream selects the next row; Upstream selects
    /// the preceding row. Other placements use their unique row.
    pub fn place(
        &self,
        position: Position,
        affinity: Affinity,
    ) -> Result<Option<CellPlacement>, CellLayoutError> {
        let range = self
            .line_rows
            .get(position.line)
            .ok_or_else(|| invalid_position(position))?;
        if range.is_empty() {
            self.validate_unplaced(position)?;
            return Ok(None);
        }
        let rows = &self.rows[range.clone()];
        let mut index = rows.partition_point(|row| row.scalars.end < position.column);
        let Some(row) = rows.get(index) else {
            return Err(invalid_position(position));
        };
        if affinity == Affinity::Downstream
            && row.scalars.end == position.column
            && rows
                .get(index + 1)
                .is_some_and(|next| next.scalars.start == position.column)
        {
            index += 1;
        }
        let row = rows.get(index).ok_or_else(|| invalid_position(position))?;
        let column = cell_at_boundary(row, position)?;
        Ok(Some(CellPlacement {
            row: ScreenRow(range.start + index),
            column: CellColumn(column),
            affinity,
        }))
    }

    /// Canonicalizes a cell to a complete grapheme boundary without mutating it.
    ///
    /// Tab/wide continuation cells hit the atom's start. Beyond text hits that
    /// row's complete end with Upstream affinity, including trailing spaces.
    /// A maximal run of invisible clusters at one cell maps to its trailing
    /// boundary. These are intentionally many-to-one mappings, not bijections.
    pub fn position_at(
        &self,
        screen_row: ScreenRow,
        column: CellColumn,
    ) -> Result<CellHit, CellLayoutError> {
        let row = self.row(screen_row)?;
        if column.0 >= row.width {
            return Ok(CellHit {
                position: Position::new(row.line, row.scalars.end),
                affinity: Affinity::Upstream,
            });
        }
        // End offsets are monotone, including zero-width atoms. Skipping them
        // gives the boundary after the entire invisible run at this cell.
        let index = row
            .clusters
            .partition_point(|cluster| cluster.column.0 + cluster.width <= column.0);
        let cluster = row
            .clusters
            .get(index)
            .ok_or_else(|| CellLayoutError::InvalidRow {
                row: screen_row.0,
                total_rows: self.total_rows(),
            })?;
        Ok(CellHit {
            position: Position::new(row.line, cluster.scalars.start),
            affinity: Affinity::Downstream,
        })
    }

    fn validate_unplaced(&self, position: Position) -> Result<(), CellLayoutError> {
        let cached = self
            .unplaced_boundaries
            .get(position.line)
            .ok_or_else(|| invalid_position(position))?;
        let boundaries = cached
            .get_or_init(|| {
                let text =
                    self.document
                        .line(position.line)
                        .ok_or(CellLayoutError::InvalidPosition {
                            line: position.line,
                            column: 0,
                        })?;
                let mut boundaries = vec![0];
                let mut column = 0;
                for cluster in text.graphemes(true) {
                    column += cluster.chars().count();
                    boundaries.push(column);
                }
                Ok(boundaries)
            })
            .as_ref()
            .map_err(Clone::clone)?;
        match boundaries.binary_search(&position.column) {
            Ok(_) => Ok(()),
            Err(index) if index == boundaries.len() => Err(invalid_position(position)),
            Err(_) => Err(interior_position(position)),
        }
    }
}

fn cell_at_boundary(row: &CellRow, position: Position) -> Result<usize, CellLayoutError> {
    if position.column == row.scalars.end {
        return Ok(row.width);
    }
    row.clusters
        .binary_search_by_key(&position.column, |cluster| cluster.scalars.start)
        .ok()
        .and_then(|index| row.clusters.get(index))
        .map(|cluster| cluster.column.0)
        .ok_or_else(|| interior_position(position))
}

const fn invalid_position(position: Position) -> CellLayoutError {
    CellLayoutError::InvalidPosition {
        line: position.line,
        column: position.column,
    }
}

const fn interior_position(position: Position) -> CellLayoutError {
    CellLayoutError::NonGraphemeBoundary {
        line: position.line,
        column: position.column,
    }
}
