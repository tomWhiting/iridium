//! Per-document terminal state and the single path into the shared workspace.
use std::path::{Path, PathBuf};

use iridium_editor::workspace::NodeId;
use iridium_editor::{Editor, Keymap};
use iridium_tui::frame::{Frame, SearchOverlay};

use super::document::language_of;
use super::{App, StartupError};
use crate::file::{FileError, TextFile};

/// Invocation-wide editing permission, applied to every future document.
#[derive(Clone, Copy)]
pub(super) enum Access {
    ReadWrite,
    ReadOnly,
}

/// These caches belong to a document, never to the currently selected tab.
/// Two documents can have the same revision but different syntax spans.
pub(super) struct DocumentSession {
    pub file: Option<TextFile>,
    pub frame: Frame,
    pub search: SearchOverlay,
}

impl DocumentSession {
    fn new(file: Option<TextFile>, keys: &Keymap) -> Self {
        Self {
            file,
            frame: Frame::new(),
            search: SearchOverlay::new(keys),
        }
    }

    pub fn is_dirty(&self, editor: &Editor) -> bool {
        let saved = self
            .file
            .as_ref()
            .and_then(TextFile::saved_text)
            .unwrap_or("");
        *editor.state().document.rope() != saved
    }
}

impl App {
    pub(super) fn add_file(&mut self, path: &Path) -> Result<NodeId, StartupError> {
        self.add_file_with_limit(path, usize::MAX)
    }

    pub(super) fn add_file_with_limit(
        &mut self,
        path: &Path,
        limit: usize,
    ) -> Result<NodeId, StartupError> {
        let path = file_identity(path).map_err(StartupError::File)?;
        if let Some(tab) = self.tab_for_path(&path) {
            self.activate_tab(tab)?;
            return Ok(tab);
        }
        let (file, text) = TextFile::open_with_limit(&path, limit).map_err(StartupError::File)?;
        let name = file.display_name();
        let tab = self
            .workspace
            .open_with(
                &text,
                name,
                None,
                DocumentSession::new(Some(file), &self.user_keys),
            )
            .ok_or(StartupError::Session("could not open a root tab"))?;
        self.activate_tab(tab)?;
        if let Some(editor) = self.workspace.active_editor_mut() {
            editor.state_mut().read_only = matches!(self.access, Access::ReadOnly);
            if let Some(language) = language_of(&path) {
                editor.set_language(language);
            }
            if self.startup_mode == super::modal::StartupMode::Structural {
                editor.perform_ast_request(iridium_editor::input::AstRequest::SelectNode);
                editor.set_mode(Some(
                    iridium_editor::commands::structural_keymap::STRUCTURAL,
                ));
            }
        }
        Ok(tab)
    }

    pub(super) fn add_scratch(&mut self) -> Result<NodeId, StartupError> {
        let tab = self
            .workspace
            .open_with(
                "",
                "Untitled",
                None,
                DocumentSession::new(None, &self.user_keys),
            )
            .ok_or(StartupError::Session("could not open an untitled tab"))?;
        self.activate_tab(tab)?;
        if let Some(editor) = self.workspace.active_editor_mut() {
            editor.state_mut().read_only = matches!(self.access, Access::ReadOnly);
            if self.startup_mode == super::modal::StartupMode::Structural {
                editor.set_mode(Some(
                    iridium_editor::commands::structural_keymap::STRUCTURAL,
                ));
            }
        }
        Ok(tab)
    }

    pub(super) fn tab_for_path(&self, path: &Path) -> Option<NodeId> {
        self.workspace.tabs().into_iter().find(|tab| {
            self.workspace
                .node(*tab)
                .and_then(iridium_editor::workspace::Node::document)
                .and_then(|id| self.workspace.payload(id))
                .and_then(|session| session.file.as_ref())
                .is_some_and(|file| file.path() == path)
        })
    }

    fn activate_tab(&mut self, tab: NodeId) -> Result<(), StartupError> {
        if self.workspace.active() == Some(tab) {
            return Ok(());
        }
        self.leave_tab();
        if !self.workspace.activate(tab) {
            return Err(StartupError::Session("the requested tab no longer exists"));
        }
        self.after_tab_change();
        Ok(())
    }
}

/// Resolve existing symlinks and relative aliases before deduplicating tabs.
/// A new file uses its real parent, so saving it does not change its identity.
pub(super) fn file_identity(path: &Path) -> Result<PathBuf, FileError> {
    let resolved = match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            path.file_name().map_or(Err(error), |name| {
                std::fs::canonicalize(parent).map(|parent| parent.join(name))
            })
        },
        Err(error) => Err(error),
    };
    resolved.map_err(|source| FileError::Unreadable {
        path: path.to_owned(),
        source,
    })
}
