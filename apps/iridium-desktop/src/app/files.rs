//! Reading and writing files, and leaving.
//!
//! The file layer is `iridium-file` — the identical atomic-save and
//! staleness-by-byte-comparison code the terminal face runs. What is decided
//! here is only what a face decides: when to ask a question instead of
//! writing, and what a refusal says.

use std::path::{Path, PathBuf};

use iridium_editor::Language;
use iridium_editor::workspace::NodeId;
use iridium_file::{DiskState, FileError, TextFile};

use super::state::{DesktopApp, DesktopDocument, Flow, UNTITLED};
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

    /// Asks where to write the document, starting from the name it has.
    ///
    /// This is the way in `file.saveAs` needs and `file.save` cannot be. `⌘S`
    /// reaches the prompt only for a buffer that has **no** name, because for
    /// one that has a name it must write to it rather than ask; so before this
    /// existed, a named document could not be written anywhere else at all —
    /// [`save_as`](Self::save_as) was reachable, complete and tested, with no
    /// key, menu row or palette entry leading to it.
    pub(super) fn save_as_prompt(&mut self) -> Flow {
        let current = self
            .workspace
            .active_payload()
            .and_then(|document| document.file.as_ref())
            .map(TextFile::path);
        // An unnamed buffer has nothing to start from, so it gets exactly the
        // prompt `⌘S` already puts up for one. The two ways in agree rather
        // than each having their own shape.
        self.prompt = Some(current.map_or_else(Prompt::save_as, Prompt::save_as_named));
        self.request_redraw();
        Flow::Running
    }

    /// Writes the document to `path`, whether or not it already had one.
    ///
    /// The save is not forced. A file already at `path` is **asked about**
    /// rather than refused or overwritten: refusing outright would make
    /// "put this over that" unsayable, which is half of what a save-as is
    /// for, and overwriting silently would destroy a file the user never
    /// named in this session.
    ///
    /// The buffer takes the new name only when the write succeeded. A failed
    /// save-as leaves it attached to whatever it was attached to before —
    /// unnamed if it was unnamed, and *still pointing at the old file* if it
    /// had one. That is why the write goes through a **clone**: renaming the
    /// document's own [`TextFile`] first and saving second would, on a failure,
    /// leave the buffer believing it lives at a path nothing was ever written
    /// to, and the next `⌘S` would then create that file without a word.
    pub(super) fn save_as(&mut self, path: &Path) -> Flow {
        self.write_as(path, false)
    }

    /// Writes the document to `path`, over whatever is already there.
    ///
    /// The answer to the question [`save_as`](Self::save_as) asks, and the only
    /// caller is that answer coming back from the prompt strip.
    pub(super) fn overwrite_as(&mut self, path: &Path) -> Flow {
        self.write_as(path, true)
    }

    /// The one implementation behind save-as and its confirmed overwrite.
    fn write_as(&mut self, path: &Path, force: bool) -> Flow {
        let path = self.resolve_for_save(path);

        // ⭐ Saving to the path the document already has is a **save**, not a
        // save-as, and delegating rather than duplicating is what keeps that
        // true. The clone below has no baseline, so it would report a file
        // that has sat untouched on disk all session as one that "appeared"
        // and ask permission to overwrite it — a nuisance in place of the real
        // guard. `save` compares against the bytes actually read, which is the
        // protection worth having, and it is also where the read-only refusal
        // lives.
        //
        // That last part is the rule, stated once: **read-only protects the
        // file the buffer came from, not the buffer's text.** Writing back to
        // that file is refused; writing the text somewhere else is not, because
        // a buffer that could never be got out of the editor at all would be a
        // dead end rather than a protection.
        let same_path = self
            .workspace
            .active_payload()
            .and_then(|document| document.file.as_ref())
            .is_some_and(|file| file.path() == path);
        if same_path {
            return self.save(force);
        }

        let Some((editor, document)) = self.workspace.active_editor_and_payload_mut() else {
            self.message = Some(Message::error("there is no document to save"));
            return Flow::Running;
        };
        // Cloned and renamed rather than built fresh, so a file that was read
        // with a byte-order mark keeps it: `TextFile::new` would silently drop
        // the mark, and some tools require it. `rename` drops the baseline,
        // which is right — nothing is known about the new path.
        let mut file = document.file.as_ref().map_or_else(
            || TextFile::new(&path),
            |existing| {
                let mut renamed = existing.clone();
                renamed.rename(&path);
                renamed
            },
        );
        let text = editor.content();
        match file.save(&text, force) {
            Ok(()) => {
                self.message = Some(Message::notice(format!("wrote {}", file.display_name())));
                document.file = Some(file);
                // The name is what says what the document is written in, so a
                // new name is believed — including when it says nothing. This
                // is the one path where a document changes what it is called
                // without changing which editor holds it, so it is the one
                // place a language can outlive the name that chose it:
                // `a.rs` saved as `notes.log` would otherwise stay Rust.
                match language_of(&path) {
                    Some(language) => editor.set_language(language),
                    None => editor.clear_language(),
                }
                self.refresh_title();
            },
            // A file is at the path and this session never read it. The only
            // honest answers are "leave it" and "replace it", so both are
            // offered; `Appeared` is the one `DiskState` a save-as can produce
            // that a person can actually decide about.
            Err(FileError::Changed {
                state: DiskState::Appeared,
                ..
            }) => {
                self.prompt = Some(Prompt::confirm(
                    format!("{} exists. Overwrite it? (y/n)", file.display_name()),
                    Deed::OverwriteWith(path),
                ));
            },
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        self.request_redraw();
        Flow::Running
    }

    /// Turns a name typed on the prompt strip into a path to write.
    ///
    /// ⭐ **A relative name is resolved against the session's own root, not
    /// against the process's working directory, and the difference is not
    /// cosmetic.** A bundle launched from Finder inherits `/` as its working
    /// directory — the case [`crate::project`] exists because of, and one that
    /// cannot be reproduced from a terminal — so `notes.md` typed into a
    /// freshly-made file would resolve to `/notes.md` and fail on a permission
    /// denial no user could explain.
    ///
    /// The root asked for is the one the file chooser opens at and the one the
    /// explorer roots on: the project when a folder is open, the active file's
    /// directory otherwise, the working directory or home below that. One
    /// answer to "where is this session", reused rather than re-derived.
    ///
    /// An absolute path is returned untouched, which is what every path this
    /// face composes itself already is.
    fn resolve_for_save(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            return path.to_path_buf();
        }
        self.explorer_root().path.join(path)
    }

    /// Opens a fresh untitled buffer in a new tab, and activates it.
    ///
    /// The tab joins the group the active one is in, for the reason
    /// [`open_file`](Self::open_file) puts an opened file there: a buffer made
    /// while working inside a group belongs with the work it was made from.
    ///
    /// Nothing is asked and nothing is closed. Before tabs this verb could not
    /// have existed without a confirmation — a new document would have replaced
    /// the open one — and that is the same change of stakes that let a drop
    /// stop asking.
    pub(super) fn new_file(&mut self) -> Flow {
        let parent = self
            .workspace
            .active()
            .and_then(|tab| self.workspace.parent_of(tab));
        match self.open_untitled(parent) {
            Some(tab) => {
                self.workspace.activate(tab);
                self.after_tab_change();
            },
            // Unreachable: `open_with` refuses only a parent that is not a
            // group, and `parent_of` yields exactly a group or none. Reported
            // rather than dropped, because a chord that consumed the key and
            // produced no tab would look like a dead key.
            None => self.message = Some(Message::error("a new file could not be opened")),
        }
        Flow::Running
    }

    /// Opens an empty untitled tab under `parent`, without activating it.
    ///
    /// The one place an untitled buffer is made. [`new_file`](Self::new_file)
    /// makes one on request and
    /// [`ensure_a_tab_is_open`](super::DesktopApp::ensure_a_tab_is_open) makes
    /// one to keep the window from having no document at all; two constructions
    /// would be two ideas of what a blank document *is*, free to disagree about
    /// its label the moment either moved.
    pub(super) fn open_untitled(&mut self, parent: Option<NodeId>) -> Option<NodeId> {
        self.workspace.open_with(
            "",
            UNTITLED,
            parent,
            DesktopDocument {
                scroll_y: 0.0,
                file: None,
                syntax: HighlightCache::new(),
            },
        )
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
