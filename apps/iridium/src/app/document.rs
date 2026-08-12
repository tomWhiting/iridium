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
use std::path::{Path, PathBuf};

use iridium_editor::{KeymapError, Language, Position, RegistryError};

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
    Command(RegistryError),
    /// The keymap layer this face pushes was refused.
    Keymap(KeymapError),
    /// The theme could not be loaded.
    Theme(ThemeError),
    /// The file could not be opened.
    File(FileError),
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => write!(f, "{error}"),
            Self::Keymap(error) => write!(f, "{error}"),
            Self::Theme(error) => write!(f, "{error}"),
            Self::File(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for StartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Keymap(error) => Some(error),
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
        let saved = self
            .file
            .as_ref()
            .and_then(TextFile::saved_text)
            .unwrap_or("");
        *self.editor.state().document.rope() != saved
    }

    /// Writes the document to its file.
    ///
    /// An unnamed buffer is not a dead end: it asks for a name, which is what
    /// the command line promises when it is given no file.
    pub(super) fn save(&mut self, force: bool) -> Flow {
        if self.editor.state().read_only {
            // Nothing could have changed, so a save here can only write a
            // stale buffer over a file that something else may have moved on.
            self.message = Some(Message::error("the document is read-only"));
            return Flow::Running;
        }
        if self.file.is_none() {
            self.prompt = Some(Prompt::save_as());
            return Flow::Running;
        }

        let text = self.editor.content();
        // Unreachable: the check above returned. Reported rather than ignored
        // so that a later edit cannot make it silent.
        let Some(file) = self.file.as_mut() else {
            self.message = Some(Message::error("there is no file to write"));
            return Flow::Running;
        };
        let outcome = file.save(&text, force).map(|()| file.display_name());
        self.message = Some(match outcome {
            Ok(name) => Message::notice(format!("wrote {name}")),
            Err(error) => Message::error(error.to_string()),
        });
        Flow::Running
    }

    /// Writes an unnamed buffer to a path for the first time.
    ///
    /// The save is not forced, and a file that appeared at this path while the
    /// name was being typed is [`DiskState::Appeared`](crate::file::DiskState)
    /// — so it is reported rather than overwritten. The buffer takes the name
    /// only when the write succeeded; a failed save-as leaves it unnamed, which
    /// is what it still is.
    pub(super) fn save_as(&mut self, path: &Path) -> Flow {
        let mut file = TextFile::new(path);
        let text = self.editor.content();
        match file.save(&text, false) {
            Ok(()) => {
                self.message = Some(Message::notice(format!("wrote {}", file.display_name())));
                self.file = Some(file);
                // The name is what says what the document is written in, so a
                // new name is believed — including when it says nothing. This
                // face holds one buffer for the session, so a rename from
                // `a.rs` to `notes.log` would otherwise leave Rust
                // highlighting a log file for as long as the program ran.
                match language_of(path) {
                    Some(language) => self.editor.set_language(language),
                    None => self.editor.clear_language(),
                }
            },
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        Flow::Running
    }

    /// Asks before re-reading a file over unsaved changes.
    pub(super) fn request_reload(&mut self) -> Flow {
        if self.file.is_none() {
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
        let Some(file) = self.file.as_mut() else {
            self.message = Some(Message::error("there is no file to re-read"));
            return Flow::Running;
        };
        let outcome = file.reload().map(|text| (text, file.display_name()));
        match outcome {
            Ok((text, name)) => {
                self.editor.set_content(&text);
                self.ensure_caret_visible();
                self.message = Some(Message::notice(format!("re-read {name}")));
            },
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        Flow::Running
    }

    /// Asks before replacing the document with another file.
    ///
    /// ⚠️ **The same guard `request_reload` has, for the same reason and not
    /// by analogy: this face edits one buffer, so opening a file *discards*
    /// the one on screen.** The GPU face's answer to the identical
    /// `ExplorerOutcome::Open` is a new tab and loses nothing — the outcome
    /// means "the user picked this file", and what that costs is the face's to
    /// know. Recorded here because the word "open" reads as harmless and is
    /// not.
    pub(super) fn request_open(&mut self, path: PathBuf) -> Flow {
        if self.is_dirty() {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            self.prompt = Some(Prompt::confirm(
                format!("Unsaved changes. Open {name} and lose them? (y/n)"),
                Deed::Open(path),
            ));
            return Flow::Running;
        }
        self.open_file(&path)
    }

    /// Replaces the document with `path`, discarding unsaved changes.
    ///
    /// The language is re-derived rather than kept: a session that opened a
    /// `.rs` and then a `.json` would otherwise highlight the second as the
    /// first, which is #39's defect wearing a different hat. `set_language`
    /// takes `None` for a name nothing claims, which is what makes a plain
    /// text file plain rather than the previous file's language.
    pub(super) fn open_file(&mut self, path: &Path) -> Flow {
        match TextFile::open(path) {
            Ok((file, text)) => {
                let name = file.display_name();
                self.file = Some(file);
                self.editor.set_content(&text);
                // ⚠️ Cleared, not left alone, when the new name claims no
                // language. That is #39's defect exactly: a session that
                // opened a `.rs` and then a plain `NOTES` would highlight the
                // second as Rust, because nothing had ever said "stop".
                match language_of(path) {
                    Some(language) => self.editor.set_language(language),
                    None => self.editor.clear_language(),
                }
                self.editor.set_cursor(Position::new(0, 0));
                self.ensure_caret_visible();
                self.message = Some(Message::notice(format!("opened {name}")));
            },
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        Flow::Running
    }

    /// Leaves, asking again when there are unsaved changes.
    pub(super) fn request_quit(&mut self) -> Flow {
        if self.is_dirty() {
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
