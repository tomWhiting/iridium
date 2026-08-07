//! Reading and writing files, and leaving.
//!
//! The file layer is `iridium-file` — the identical atomic-save and
//! staleness-by-byte-comparison code the terminal face runs. What is decided
//! here is only what a face decides: when to ask a question instead of
//! writing, and what a refusal says.

use std::path::Path;

use iridium_editor::Language;
use iridium_file::{FileError, TextFile};

use super::state::{DesktopApp, DesktopDocument, Flow};
use crate::highlight::HighlightCache;
use crate::prompt::{Deed, Message, Prompt};

impl DesktopApp {
    /// Writes the document to its file.
    ///
    /// An unnamed buffer is not a dead end: it asks for a name on the prompt
    /// strip. A file that changed on disk refuses the unforced save and names
    /// the chord that overwrites, because a refusal that does not say the way
    /// out is a dead end with better manners.
    pub(super) fn save(&mut self, force: bool) -> Flow {
        let Some((editor, document)) = self.workspace.active_editor_and_payload_mut() else {
            // The session keeps a tab open, so this is unreachable — reported
            // rather than ignored, because a save that silently wrote nothing
            // is the one failure a user would not notice until it mattered.
            self.message = Some(Message::error("there is no document to save"));
            return Flow::Running;
        };
        if editor.state().read_only {
            // Nothing could have changed, so a save here can only write a
            // stale buffer over a file that something else may have moved on.
            self.message = Some(Message::error("the document is read-only"));
            return Flow::Running;
        }
        if document.file.is_none() {
            self.prompt = Some(Prompt::save_as());
            return Flow::Running;
        }

        let text = editor.content();
        // Unreachable: the check above returned. Reported rather than ignored
        // so that a later edit cannot make it silent.
        let Some(file) = document.file.as_mut() else {
            self.message = Some(Message::error("there is no file to write"));
            return Flow::Running;
        };
        let outcome = file.save(&text, force).map(|()| file.display_name());
        self.message = Some(match outcome {
            Ok(name) => Message::notice(format!("wrote {name}")),
            Err(error @ FileError::Changed { .. }) => {
                Message::error(format!("{error} — Ctrl+Alt+S (⌘⌥S) saves anyway"))
            },
            Err(error) => Message::error(error.to_string()),
        });
        Flow::Running
    }

    /// Writes an unnamed buffer to a path for the first time.
    ///
    /// The save is not forced, and a file that appeared at this path while
    /// the name was being typed is reported rather than overwritten. The
    /// buffer takes the name only when the write succeeded; a failed save-as
    /// leaves it unnamed, which is what it still is.
    pub(super) fn save_as(&mut self, path: &Path) -> Flow {
        let mut file = TextFile::new(path);
        let Some((editor, document)) = self.workspace.active_editor_and_payload_mut() else {
            self.message = Some(Message::error("there is no document to save"));
            return Flow::Running;
        };
        let text = editor.content();
        match file.save(&text, false) {
            Ok(()) => {
                self.message = Some(Message::notice(format!("wrote {}", file.display_name())));
                document.file = Some(file);
                // The name is what says what the document is written in, so a
                // new name is believed — including when it says nothing. This
                // is the one path where a document changes what it is called
                // without changing which editor holds it, so it is the one
                // place a language can outlive the name that chose it:
                // `a.rs` saved as `notes.log` would otherwise stay Rust.
                match language_of(path) {
                    Some(language) => editor.set_language(language),
                    None => editor.clear_language(),
                }
            },
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        Flow::Running
    }

    /// Opens a file dropped onto the window.
    ///
    /// It asks nothing, and that is the change tabs bought: a drop used to
    /// replace the open buffer, which made it exactly as destructive as a
    /// quit and so had to be confirmed. It now makes a tab beside what is
    /// there and destroys nothing, so there is no question left to ask.
    pub(super) fn dropped(&mut self, path: &Path) {
        self.open_file(path);
    }

    /// Opens a file from disk **in a new tab**, and activates it.
    ///
    /// The new tab joins the group the active one is in, rather than always
    /// landing at the top level: a file opened while working inside a group
    /// belongs with the work it was opened from. `None` — the active tab is
    /// already at the top level — puts it at the top level, which is the
    /// same rule stated for the root.
    ///
    /// Nothing about the current tab is touched, so the whole class of
    /// carry-over bug the single-buffer version had to guard — a scroll
    /// offset that meant something in the previous file, a language left
    /// over from it — cannot arise. The new tab's editor is built by the
    /// workspace with this face's commands, keymaps, theme and viewport
    /// already on it.
    pub(super) fn open_file(&mut self, path: &Path) {
        match TextFile::open(path) {
            Ok((file, text)) => {
                let name = file.display_name();
                let language = language_of(file.path());
                let parent = self.workspace.active().and_then(|tab| {
                    // The *group* the tab is in. A tab is never a parent —
                    // it holds a document, not children — so this is the
                    // enclosing group or nothing.
                    self.workspace.parent_of(tab)
                });
                let opened = self.workspace.open_with(
                    &text,
                    name.clone(),
                    parent,
                    DesktopDocument {
                        scroll_y: 0.0,
                        file: Some(file),
                        syntax: HighlightCache::new(),
                    },
                );
                match opened {
                    Some(tab) => {
                        self.workspace.activate(tab);
                        // After the content, and on the tab that is now
                        // active: setting a language parses the document,
                        // and doing it first would parse an empty one.
                        if let (Some(language), Some(editor)) =
                            (language, self.workspace.active_editor_mut())
                        {
                            editor.set_language(language);
                        }
                        self.message = Some(Message::notice(format!("opened {name}")));
                    },
                    // Unreachable: the only refusal is a parent that is not
                    // a group, and `parent_of` yields exactly that or none.
                    None => {
                        self.message = Some(Message::error(format!(
                            "{name} could not be opened as a tab"
                        )));
                    },
                }
                self.after_tab_change();
            },
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        if let Some(shell) = &self.shell {
            shell.window.request_redraw();
        }
    }

    /// Leaves, asking first when there are unsaved changes.
    pub(super) fn request_quit(&mut self) -> Flow {
        if self.is_dirty() {
            self.prompt = Some(Prompt::confirm(
                "Unsaved changes. Quit without saving? (y/n)",
                Deed::Quit,
            ));
            if let Some(shell) = &self.shell {
                shell.window.request_redraw();
            }
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
