//! What the window is called.

use iridium_file::TextFile;

use super::state::DesktopApp;

/// The window title before a session has a file to name.
pub(super) const TITLE: &str = "iridium";

impl DesktopApp {
    /// Sets the window title to the document's name and dirty state, when it
    /// changed.
    pub(super) fn refresh_title(&mut self) {
        let name = self
            .workspace
            .active_payload()
            .and_then(|document| document.file.as_ref())
            .map(TextFile::display_name);
        let title = title_for(name.as_deref(), self.is_dirty());
        if title != self.title {
            if let Some(shell) = &self.shell {
                shell.window.set_title(&title);
            }
            self.title = title;
        }
    }
}

/// The window title for a document's name and dirty state, in the terminal
/// face's statusline vocabulary: `[No Name]` for an unnamed buffer, ` [+]`
/// for unsaved changes.
pub(super) fn title_for(name: Option<&str>, dirty: bool) -> String {
    let mut title = String::from(name.unwrap_or("[No Name]"));
    if dirty {
        title.push_str(" [+]");
    }
    title.push_str(" — ");
    title.push_str(TITLE);
    title
}
