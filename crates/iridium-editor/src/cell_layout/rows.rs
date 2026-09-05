//! Borrowed document/fold ownership and greedy grapheme-boundary screen rows.

use std::ops::Range;
use std::sync::OnceLock;

use crate::document::Document;
use crate::editor::FoldState;

use super::clusters::{CellCluster, segment};
use super::types::{CellLayoutError, CellWrapParameters, ScreenRow};

/// One visible screen row; its source spans never contain a partial grapheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRow {
    pub(super) line: usize,
    pub(super) scalars: Range<usize>,
    pub(super) bytes: Range<usize>,
    pub(super) clusters: Vec<CellCluster>,
    pub(super) width: usize,
}

impl CellRow {
    /// The original document line, not its fold-visible ordinal.
    pub const fn document_line(&self) -> usize {
        self.line
    }

    /// Contiguous original scalar span, including all leading/trailing spaces.
    pub fn scalar_range(&self) -> Range<usize> {
        self.scalars.clone()
    }

    /// Contiguous byte span within the original document line.
    pub fn byte_range(&self) -> Range<usize> {
        self.bytes.clone()
    }

    /// Atoms in source order, already measured from this row's origin.
    pub fn clusters(&self) -> &[CellCluster] {
        &self.clusters
    }

    /// Logical width. An indivisible overflow atom can exceed the viewport.
    pub const fn width(&self) -> usize {
        self.width
    }

    const fn empty(line: usize, scalar: usize, byte: usize) -> Self {
        Self {
            line,
            scalars: scalar..scalar,
            bytes: byte..byte,
            clusters: Vec::new(),
            width: 0,
        }
    }

    fn push(&mut self, cluster: CellCluster) -> Result<(), CellLayoutError> {
        self.width = self
            .width
            .checked_add(cluster.width)
            .ok_or(CellLayoutError::CellOverflow { line: self.line })?;
        self.scalars.end = cluster.scalars.end;
        self.bytes.end = cluster.bytes.end;
        self.clusters.push(cluster);
        Ok(())
    }
}

/// Immutable cell geometry borrowing its exact document and fold state.
///
/// No method accepts a substitute document and no retained cache is keyed by
/// revision. Distinct mutable clones can have equal document ids and revisions;
/// the source borrow, not those numbers, is the freshness guarantee.
/// Hidden/zero-width lines acquire a grapheme-boundary index only when placement
/// demands one. The index and any measurement error belong to this exact borrow;
/// subsequent queries reuse boundaries without retaining another copy of the text.
///
/// Document mutation cannot overlap the map's use:
///
/// ```compile_fail
/// use iridium_editor::{Document, Position};
/// use iridium_editor::editor::FoldState;
/// use iridium_editor::cell_layout::{CellRowMap, CellWrapParameters};
/// let mut document = Document::new("first");
/// let folds = FoldState::new();
/// let rows = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(4, 4))?;
/// document.insert(Position::zero(), "changed")?;
/// println!("{}", rows.total_rows());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// Fold mutation is excluded for the same reason:
///
/// ```compile_fail
/// use iridium_editor::Document;
/// use iridium_editor::editor::FoldState;
/// use iridium_editor::cell_layout::{CellRowMap, CellWrapParameters};
/// let document = Document::new("text");
/// let mut folds = FoldState::new();
/// let rows = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(4, 4))?;
/// folds.unfold_all();
/// println!("{}", rows.total_rows());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct CellRowMap<'a> {
    pub(super) document: &'a Document,
    pub(super) folds: &'a FoldState,
    pub(super) parameters: CellWrapParameters,
    pub(super) rows: Vec<CellRow>,
    pub(super) line_rows: Vec<Range<usize>>,
    pub(super) unplaced_boundaries: Vec<OnceLock<Result<Vec<usize>, CellLayoutError>>>,
}

impl<'a> CellRowMap<'a> {
    /// Prepares rows from the live source without mutating or copying its text
    /// into a cache identity. Zero columns produces an explicit empty map.
    pub fn prepare(
        document: &'a Document,
        folds: &'a FoldState,
        parameters: CellWrapParameters,
    ) -> Result<Self, CellLayoutError> {
        let mut result = Self {
            document,
            folds,
            parameters,
            rows: Vec::new(),
            line_rows: Vec::with_capacity(document.line_count()),
            unplaced_boundaries: Vec::with_capacity(document.line_count()),
        };
        for line in 0..document.line_count() {
            let first = result.rows.len();
            if parameters.columns() > 0 && !folds.is_line_hidden(line) {
                let text = document
                    .line(line)
                    .ok_or(CellLayoutError::InvalidPosition { line, column: 0 })?;
                result.wrap_line(line, &text)?;
            }
            result.line_rows.push(first..result.rows.len());
            result.unplaced_boundaries.push(OnceLock::new());
        }
        Ok(result)
    }

    /// The borrowed source; it cannot be replaced while this map is in use.
    pub const fn document(&self) -> &'a Document {
        self.document
    }

    /// The actual visibility owner supplied at preparation.
    pub const fn folds(&self) -> &'a FoldState {
        self.folds
    }

    /// Explicit immutable width and tab-stop inputs.
    pub const fn parameters(&self) -> CellWrapParameters {
        self.parameters
    }

    /// Number of visible screen rows, including real empty document lines.
    pub fn total_rows(&self) -> usize {
        self.rows.len()
    }

    /// All rows in screen order, for bounded viewport slicing by a face.
    pub fn rows(&self) -> &[CellRow] {
        &self.rows
    }

    /// Retrieves one row, refusing an invalid index rather than inventing one.
    pub fn row(&self, row: ScreenRow) -> Result<&CellRow, CellLayoutError> {
        self.rows.get(row.0).ok_or(CellLayoutError::InvalidRow {
            row: row.0,
            total_rows: self.rows.len(),
        })
    }

    fn wrap_line(&mut self, line: usize, text: &str) -> Result<(), CellLayoutError> {
        let mut row = CellRow::empty(line, 0, 0);
        for mut cluster in segment(text)? {
            cluster.place_at(row.width, self.parameters.tab_width());
            // Check remaining space without overflowing when a tab is enormous.
            // Invisible atoms stay with the preceding row and never cause a
            // phantom row by themselves, including after an exactly full row.
            let needs_break = cluster.width > 0
                && row.width > 0
                && (row.width >= self.parameters.columns()
                    || cluster.width > self.parameters.columns() - row.width);
            if needs_break {
                let next = CellRow::empty(line, cluster.scalars.start, cluster.bytes.start);
                self.rows.push(row);
                row = next;
                cluster.place_at(0, self.parameters.tab_width());
            }
            row.push(cluster)?;
        }
        // Always one row for a real empty line, never an extra row merely
        // because a positive-width atom filled the final row exactly.
        self.rows.push(row);
        Ok(())
    }
}
