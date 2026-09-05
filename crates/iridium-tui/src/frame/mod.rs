//! Borrowed cell frames and legacy viewport rendering; no terminal ownership.

mod command_palette;
mod field;
mod file_explorer;
mod geometry;
mod gutter;
mod highlight;
mod history_panel;
mod line;
mod palette;
mod panel;
mod search;
mod status;
mod text;
mod units;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub use crate::cell::CellBuffer;
#[cfg(test)]
pub use iridium_editor::Editor;

mod cell_geometry;
mod cell_options;
mod cell_render;
#[cfg(test)]
mod embedding_tests;
mod prepared_cells;
mod render;
mod types;
#[cfg(test)]
mod wrap_tests;

pub use self::command_palette::{CommandPalette, PaletteOutcome};
pub use self::file_explorer::{ExplorerAction, FileExplorerPanel};
pub use self::geometry::{CellPosition, Chrome, FrameLayout, document_rows};
pub use self::history_panel::{HistoryOutcome, HistoryPanel};
pub use self::line::{LineLayout, PlacedCluster};
pub use self::palette::Palette;
pub use self::search::{SearchOutcome, SearchOverlay};
pub use self::status::Status;
pub use self::text::TextArea;

pub use cell_options::{CellCaret, CellFrameError, CellFrameLayout, CellFrameOptions};
pub use prepared_cells::PreparedCellFrame;
pub use types::Frame;
