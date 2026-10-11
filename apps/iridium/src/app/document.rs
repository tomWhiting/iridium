//! What the session does to a file: opening, saving, reloading, and deciding
//! whether there is anything to lose.
//!
//! This half of [`App`] is where every irreversible thing happens, and all of
//! it goes through [`TextFile`], which never writes over a file it has not just
//! read and never writes anything but a whole file at once. Nothing here
//! touches the disk directly.
//!
//! # Asking twice
//!
//! Two of these verbs can destroy work, and each asks first. A quit with
//! unsaved changes asks; so does a reload, because
//! [`Editor::set_content`](iridium_editor::Editor::set_content) replaces the
//! undo tree along with the text, so what a reload discards is not recoverable
//! by any key afterwards. The question is one keystroke and the alternative is
//! a destroyed afternoon.
//!
//! A save asks nothing and refuses instead: a file that changed on disk since
//! it was read is reported, and overwriting it is a separate command with a
//! separate chord. That is the right way round — a save is what the user just
//! asked for, and the surprise belongs to the disk, not to them.

use std::fmt;
use std::path::Path;

use iridium_editor::Language;
use iridium_editor::workspace::FaceSetupError;

use super::prompt::{Deed, Message, Prompt};
use super::{App, Flow};
use crate::file::{FileError, TextFile};
use crate::theme::ThemeError;

/// Why a session could not be started.
///
/// Every one of these happens before the terminal is touched, so the message
/// reaches an ordinary shell rather than a screen that is about to be torn
/// down.
#[derive(Debug)]
pub enum StartupError {
    /// A command this face contributes could not be registered.
    Setup(FaceSetupError),
    /// The shared workspace could not create or activate a document.
    Session(&'static str),
    /// The theme could not be loaded.
    Theme(ThemeError),
    /// The file could not be opened.
    File(FileError),
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(error) => write!(f, "{error}"),
            Self::Session(message) => f.write_str(message),
            Self::Theme(error) => write!(f, "{error}"),
            Self::File(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for StartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Setup(error) => Some(error),
            Self::Session(_) => None,
            Self::Theme(error) => Some(error),
            Self::File(error) => Some(error),
        }
    }
}

impl App {
    /// Whether the document differs from what is on disk.
    ///
    /// Compared against the bytes the file held when it and this program last
    /// agreed, not against a revision counter: a document undone back to its
    /// saved state is clean, and a counter would call it modified for the rest
    /// of the session.
    ///
    /// The comparison is a rope against a `&str`, which returns on a length
    /// mismatch without reading a byte and otherwise walks the rope's chunks.
    /// It allocates nothing, and the common case — an edit that changed the
    /// document's length — costs a comparison of two integers.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.workspace
            .active_document()
            .is_some_and(|id| self.document_is_dirty(id))
    }

    /// Writes the document to its file.
    ///
    /// An unnamed buffer is not a dead end: it asks for a name, which is what
    /// the command line promises when it is given no file.
    pub(super) fn save(&mut self, force: bool) -> Flow {
        self.write_file(force);
        Flow::Running
    }

    pub(super) fn write_file(&mut self, force: bool) -> super::ex::WriteOutcome {
        let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() else {
            self.message = Some(Message::error("there is no document to save"));
            return super::ex::WriteOutcome::Refused;
        };
        if editor.state().read_only {
            // Nothing could have changed, so a save here can only write a
            // stale buffer over a file that something else may have moved on.
            self.message = Some(Message::error("the document is read-only"));
            return super::ex::WriteOutcome::Refused;
        }
        if session.file.is_none() {
            self.prompt = Some(Prompt::save_as());
            return super::ex::WriteOutcome::NeedsName;
        }

        let text = editor.content();
        // Unreachable: the check above returned. Reported rather than ignored
        // so that a later edit cannot make it silent.
        let Some(file) = session.file.as_mut() else {
            self.message = Some(Message::error("there is no file to write"));
            return super::ex::WriteOutcome::Refused;
        };
        let outcome = file.save(&text, force).map(|()| file.display_name());
        let succeeded = outcome.is_ok();
        self.message = Some(match outcome {
            Ok(name) => Message::notice(format!("wrote {name}")),
            Err(error) => Message::error(error.to_string()),
        });
        if succeeded {
            super::ex::WriteOutcome::Saved
        } else {
            super::ex::WriteOutcome::Refused
        }
    }

    /// Writes an unnamed buffer to a path for the first time.
    ///
    /// The save is not forced, and a file that appeared at this path while the
    /// name was being typed is [`DiskState::Appeared`](crate::file::DiskState)
    /// — so it is reported rather than overwritten. The buffer takes the name
    /// only when the write succeeded; a failed save-as leaves it unnamed, which
    /// is what it still is.
    pub(super) fn write_as(&mut self, path: &Path) -> super::ex::WriteOutcome {
        let project_path = self
            .project
            .as_ref()
            .filter(|_| path.is_relative())
            .map(|root| root.join(path));
        let path = project_path.as_deref().unwrap_or(path);
        let path = match super::session::file_identity(path) {
            Ok(path) => path,
            Err(error) => {
                self.message = Some(Message::error(error.to_string()));
                return super::ex::WriteOutcome::Refused;
            },
        };
        if self
            .tab_for_path(&path)
            .is_some_and(|tab| Some(tab) != self.workspace.active())
        {
            self.message = Some(Message::error("that file is already open in another tab"));
            return super::ex::WriteOutcome::Refused;
        }
        let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() else {
            self.message = Some(Message::error("there is no document to save"));
            return super::ex::WriteOutcome::Refused;
        };
        if editor.state().read_only {
            self.message = Some(Message::error("the document is read-only"));
            return super::ex::WriteOutcome::Refused;
        }
        let mut file = TextFile::new(&path);
        match file.save(&editor.content(), false) {
            Ok(()) => {
                let name = file.display_name();
                session.file = Some(file);
                match language_of(&path) {
                    Some(language) => editor.set_language(language),
                    None => editor.clear_language(),
                }
                self.message = Some(Message::notice(format!("wrote {name}")));
                if let Some(tab) = self.workspace.active() {
                    if !self.workspace.rename(tab, name) {
                        self.message =
                            Some(Message::error("saved, but could not update the tab title"));
                    }
                }
                super::ex::WriteOutcome::Saved
            },
            Err(error) => {
                self.message = Some(Message::error(error.to_string()));
                super::ex::WriteOutcome::Refused
            },
        }
    }

    /// Asks before re-reading a file over unsaved changes.
    pub(super) fn request_reload(&mut self) -> Flow {
        if self.file().is_none() {
            self.message = Some(Message::error("there is no file to re-read"));
            return Flow::Running;
        }
        if self.is_dirty() {
            self.prompt = Some(Prompt::confirm(
                "Unsaved changes. Re-read the file and lose them? (y/n)",
                Deed::Reload,
            ));
            return Flow::Running;
        }
        self.reload()
    }

    /// Re-reads the file, discarding unsaved changes.
    pub(super) fn reload(&mut self) -> Flow {
        let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() else {
            self.message = Some(Message::error("there is no document to re-read"));
            return Flow::Running;
        };
        let Some(file) = session.file.as_mut() else {
            self.message = Some(Message::error("there is no file to re-read"));
            return Flow::Running;
        };
        let outcome = file.reload().map(|text| (text, file.display_name()));
        match outcome {
            Ok((text, name)) => {
                editor.set_content(&text);
                self.ensure_caret_visible();
                self.message = Some(Message::notice(format!("re-read {name}")));
            },
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        Flow::Running
    }

    /// Opens the selected path through the shared workspace.
    pub(super) fn open_file(&mut self, path: &Path) -> Flow {
        match self.add_file(path) {
            Ok(_) => {
                if self
                    .explorer
                    .as_ref()
                    .is_some_and(iridium_tui::frame::FileExplorerPanel::is_sidebar)
                {
                    self.focus = crate::app::sidebar::Focus::Document;
                } else {
                    self.close_explorer();
                }
                self.sync_viewport();
                self.message = Some(Message::notice(format!("opened {}", path.display())));
            },
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        Flow::Running
    }

    /// Leaves, asking again when there are unsaved changes.
    pub(super) fn request_quit(&mut self) -> Flow {
        if self.workspace.tabs().into_iter().any(|tab| {
            self.workspace
                .node(tab)
                .and_then(iridium_editor::workspace::Node::document)
                .is_some_and(|id| self.document_is_dirty(id))
        }) {
            self.prompt = Some(Prompt::confirm(
                "Unsaved changes. Quit without saving? (y/n)",
                Deed::Quit,
            ));
            return Flow::Running;
        }
        Flow::Exit
    }
}

/// The language a file name implies, if the kernel knows one for it.
///
/// Matches the whole name, not just the extension, because the vendored
/// manifests claim files that have none — `flake.lock`, `tsconfig.json`,
/// `.env`, `.bashrc`, `PKGBUILD`.
///
/// A name nothing claims is not an error: the document is then edited without
/// highlighting and without folds, which is what a plain text file is.
pub(super) fn language_of(path: &Path) -> Option<Language> {
    Language::for_path(path)
}
