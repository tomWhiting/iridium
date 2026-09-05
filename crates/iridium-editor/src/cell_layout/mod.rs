//! Borrowed, grapheme-aware cell geometry; no renderer, input owner or retained cache.

pub mod clusters;
pub mod mapping;
pub mod rows;
pub mod types;

pub use clusters::{CellCluster, CellLine, ClusterKind};
pub use rows::{CellRow, CellRowMap};
pub use types::{
    Affinity, CellColumn, CellHit, CellLayoutError, CellPlacement, CellWrapParameters, ScreenRow,
};

#[cfg(test)]
pub mod tests;
