//! Native requests delivered on the editor's event loop.

use std::path::PathBuf;

use crate::menubar::MenuCommand;

/// Work queued by native callbacks, which cannot borrow the running editor.
#[derive(Debug, Clone)]
pub enum DesktopEvent {
    /// Execute a native menu command.
    Menu(MenuCommand),
    /// Open files in order, retaining URL conversion failures for display.
    OpenFiles(Vec<Result<PathBuf, String>>),
}
