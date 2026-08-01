//! The fold region itself, and the post-processing that turns raw detections
//! into the list callers see.

use serde::{Deserialize, Serialize};

/// Kind of foldable region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FoldKind {
    /// Block delimited by braces: { }
    Block,
    /// Region delimited by markers: #region / #endregion
    Region,
    /// Import statements
    Import,
    /// Multi-line comment
    Comment,
}

/// A region of code that can be folded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldRegion {
    /// Start line (0-indexed)
    pub start_line: usize,
    /// End line (0-indexed, inclusive)
    pub end_line: usize,
    /// Kind of fold
    pub kind: FoldKind,
    /// Whether currently folded
    pub is_folded: bool,
}

impl FoldRegion {
    /// Creates a new fold region.
    #[must_use]
    pub const fn new(start_line: usize, end_line: usize, kind: FoldKind) -> Self {
        Self {
            start_line,
            end_line,
            kind,
            is_folded: false,
        }
    }

    /// Returns the number of lines in this region.
    #[must_use]
    pub const fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line) + 1
    }

    /// Returns the number of lines that would be hidden when folded.
    ///
    /// The first line is always visible (shows the fold indicator),
    /// so only lines 2..=end are hidden.
    #[must_use]
    pub const fn hidden_line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }

    /// Returns true if the given line is within this region.
    #[must_use]
    pub const fn contains_line(&self, line: usize) -> bool {
        line >= self.start_line && line <= self.end_line
    }

    /// Returns true if this region strictly contains another region.
    #[must_use]
    pub const fn contains_region(&self, other: &Self) -> bool {
        self.start_line < other.start_line && self.end_line > other.end_line
    }
}

impl PartialOrd for FoldRegion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FoldRegion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.start_line
            .cmp(&other.start_line)
            .then_with(|| other.end_line.cmp(&self.end_line)) // Larger regions first
    }
}

/// A detected region together with the extent of the node that produced it.
///
/// The byte extent is what makes an incremental update possible: it says
/// whether an edit or a changed range touches the node this region came from,
/// which line numbers alone cannot answer.
///
/// Keeping the tree's pre-order matters beyond tidiness. [`publish`] sorts by
/// line and then drops later duplicates, and Rust's sort is stable, so which of
/// two regions sharing a line span survives is decided by their order going in.
/// Reproducing the walk order reproduces that decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedRegion {
    /// First byte of the node that produced the region.
    pub start_byte: usize,
    /// One past the last byte of that node.
    pub end_byte: usize,
    /// The region itself.
    pub region: FoldRegion,
}

impl TrackedRegion {
    /// The key that reproduces a pre-order tree walk.
    ///
    /// A parent starts at or before its child and ends at or after it, so
    /// ascending start with descending end visits parents first and siblings in
    /// order. Two nodes can tie only by covering exactly the same bytes, and a
    /// stable sort then keeps whichever the walk emitted first — which is the
    /// outer one. That is why the sorts either side of this are stable.
    pub const fn order_key(&self) -> (usize, std::cmp::Reverse<usize>) {
        (self.start_byte, std::cmp::Reverse(self.end_byte))
    }

    /// True when this region's node shares any byte with `range`, endpoints
    /// included.
    ///
    /// Touching counts, and an empty range is a point that a node can still
    /// contain. Both are deliberate: the test is used to decide *both* which
    /// retained regions to throw away and which nodes to recompute, so any
    /// looseness is symmetric and the two sets stay exactly complementary. A
    /// pure deletion leaves an empty range, and the nodes around it are the ones
    /// that must be looked at again.
    pub const fn touches(&self, range: &std::ops::Range<usize>) -> bool {
        self.start_byte <= range.end && self.end_byte >= range.start
    }
}

/// Turns raw pre-order detections into the list callers see.
///
/// Sorted by start line with larger regions first, duplicate line spans
/// collapsed, and runs of single-line imports merged. Split out so the full
/// walk and the incremental cache cannot drift in how they finish.
pub fn publish(tracked: &[TrackedRegion]) -> Vec<FoldRegion> {
    let mut regions: Vec<FoldRegion> = tracked.iter().map(|t| t.region.clone()).collect();

    // Sort by start line (larger regions at same start line come first)
    regions.sort();

    // Remove duplicates (same start/end)
    regions.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);

    // Group consecutive imports
    merge_consecutive_imports(&mut regions);

    regions
}

/// Merges consecutive single-line imports into foldable groups.
///
/// A lone import is dropped: folding one line hides nothing.
fn merge_consecutive_imports(regions: &mut Vec<FoldRegion>) {
    if regions.is_empty() {
        return;
    }

    // Find runs of consecutive single-line imports
    let mut merged = Vec::with_capacity(regions.len());
    let mut i = 0;

    while i < regions.len() {
        let region = &regions[i];

        if region.kind == FoldKind::Import && region.start_line == region.end_line {
            // Look for consecutive imports
            let mut end_idx = i;
            let mut last_line = region.start_line;

            while end_idx + 1 < regions.len() {
                let next = &regions[end_idx + 1];
                if next.kind == FoldKind::Import
                    && next.start_line == next.end_line
                    && next.start_line == last_line + 1
                {
                    last_line = next.start_line;
                    end_idx += 1;
                } else {
                    break;
                }
            }

            // If we found 2+ consecutive imports, merge them
            if end_idx > i {
                merged.push(FoldRegion::new(
                    region.start_line,
                    regions[end_idx].end_line,
                    FoldKind::Import,
                ));
                i = end_idx + 1;
            } else {
                // Single import, skip (not foldable alone)
                i += 1;
            }
        } else {
            // Non-import region, keep as-is
            merged.push(region.clone());
            i += 1;
        }
    }

    *regions = merged;
}
