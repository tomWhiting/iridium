//! The shared scalar/byte/grapheme/cell measurement used by terminal layouts.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::types::{CellColumn, CellLayoutError};

/// How a complete source grapheme is represented in a terminal cell grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterKind {
    /// A printable grapheme, occupying one or two cells.
    Glyph,
    /// An indivisible tab expanded to its next row-relative tab stop.
    Tab,
    /// A control or standalone zero-width cluster; retained but not painted.
    Invisible,
}

/// One whole extended grapheme with source spans and its measured cell run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellCluster {
    pub(super) scalars: Range<usize>,
    pub(super) bytes: Range<usize>,
    pub(super) column: CellColumn,
    pub(super) width: usize,
    pub(super) kind: ClusterKind,
}

impl CellCluster {
    /// Unicode-scalar span within the original logical line.
    pub fn scalar_range(&self) -> Range<usize> {
        self.scalars.clone()
    }

    /// Byte span within the original logical line.
    pub fn byte_range(&self) -> Range<usize> {
        self.bytes.clone()
    }

    /// The first cell this cluster occupies in its measured row.
    pub const fn column(&self) -> CellColumn {
        self.column
    }

    /// Number of cells occupied, including zero and expanded tabs.
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Whether this is a glyph, tab or invisible source cluster.
    pub const fn kind(&self) -> ClusterKind {
        self.kind
    }

    pub(super) fn place_at(&mut self, column: usize, tab_width: usize) {
        self.column = CellColumn(column);
        self.width = measured_width(self.kind, self.width, column, tab_width);
    }
}

/// A complete logical line measured without wrapping.
///
/// Its source spans can also be used by a face that horizontally clips text.
/// Control clusters remain in the mapping but must never be sent to a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellLine {
    clusters: Vec<CellCluster>,
    width: usize,
    scalar_count: usize,
    byte_count: usize,
}

impl CellLine {
    /// Measures one line with the existing terminal width and tab rules.
    ///
    /// Rejects line endings and arithmetic overflow. A zero tab width is one,
    /// matching the existing editor/terminal behavior, not a new default.
    pub fn measure(text: &str, tab_width: usize) -> Result<Self, CellLayoutError> {
        let mut clusters = segment(text)?;
        let mut width = 0_usize;
        let tab_width = tab_width.max(1);
        for cluster in &mut clusters {
            cluster.place_at(width, tab_width);
            width = width
                .checked_add(cluster.width)
                .ok_or(CellLayoutError::CellOverflow { line: 0 })?;
        }
        Ok(Self {
            clusters,
            width,
            scalar_count: text.chars().count(),
            byte_count: text.len(),
        })
    }

    /// The clusters in source order.
    pub fn clusters(&self) -> &[CellCluster] {
        &self.clusters
    }

    /// Width in terminal cells, independent of Unicode-scalar count.
    pub const fn width(&self) -> usize {
        self.width
    }

    /// The logical line's final scalar position.
    pub const fn scalar_count(&self) -> usize {
        self.scalar_count
    }

    /// Bytes in the logical line, without a line ending.
    pub const fn byte_count(&self) -> usize {
        self.byte_count
    }
}

/// A borrowed, unplaced extended grapheme with original source coordinates.
/// Cell rows and legacy unwrapped faces share this measurement authority.
#[derive(Debug, Clone)]
pub struct MeasuredGrapheme<'a> {
    text: &'a str,
    scalars: Range<usize>,
    bytes: Range<usize>,
    kind: ClusterKind,
    width: usize,
}

impl MeasuredGrapheme<'_> {
    /// Original whole grapheme, including invisible control clusters.
    pub const fn text(&self) -> &str {
        self.text
    }
    /// Original Unicode-scalar range.
    pub fn scalar_range(&self) -> Range<usize> {
        self.scalars.clone()
    }
    /// Original byte range.
    pub fn byte_range(&self) -> Range<usize> {
        self.bytes.clone()
    }
    /// Printable glyph, indivisible tab, or invisible control/zero-width text.
    pub const fn kind(&self) -> ClusterKind {
        self.kind
    }
    /// Width from this row-relative cell column. A zero tab width floors to
    /// one, preserving the existing terminal/editor rule.
    pub fn width_at(&self, column: CellColumn, tab_width: usize) -> usize {
        measured_width(self.kind, self.width, column.0, tab_width)
    }
}

/// Measures source graphemes once without deciding line-ending policy or
/// accumulating a possibly overflowing line width.
///
/// Strict cell lines reject CR/LF; legacy unwrapped faces retain those clusters
/// as invisible controls.
pub fn measure_graphemes(text: &str) -> impl Iterator<Item = MeasuredGrapheme<'_>> {
    let mut scalars = 0;
    text.grapheme_indices(true).map(move |(byte, text)| {
        let scalar_count = text.chars().count();
        let (kind, width) = if text == "\t" {
            (ClusterKind::Tab, 0)
        } else if text.chars().any(char::is_control) {
            (ClusterKind::Invisible, 0)
        } else {
            let width = UnicodeWidthStr::width(text).min(2);
            let kind = if width == 0 {
                ClusterKind::Invisible
            } else {
                ClusterKind::Glyph
            };
            (kind, width)
        };
        let measured = MeasuredGrapheme {
            text,
            scalars: scalars..scalars + scalar_count,
            bytes: byte..byte + text.len(),
            kind,
            width,
        };
        // Scalar and byte spans cannot exceed the allocated source.
        scalars += scalar_count;
        measured
    })
}

/// Segment without accumulating columns: wrapping can represent enormous tabs
/// even when their unwrapped line width would overflow.
pub(super) fn segment(text: &str) -> Result<Vec<CellCluster>, CellLayoutError> {
    measure_graphemes(text)
        .map(|measured| {
            if measured.text.contains(['\r', '\n']) {
                return Err(CellLayoutError::LineEnding {
                    byte: measured.bytes.start,
                });
            }
            Ok(CellCluster {
                scalars: measured.scalars,
                bytes: measured.bytes,
                column: CellColumn(0),
                width: measured.width,
                kind: measured.kind,
            })
        })
        .collect()
}

/// Shared tab stops and measured glyph widths for both placed and raw clusters.
fn measured_width(kind: ClusterKind, width: usize, column: usize, tab_width: usize) -> usize {
    if kind == ClusterKind::Tab {
        let tab_width = tab_width.max(1);
        tab_width - column % tab_width
    } else {
        width
    }
}
