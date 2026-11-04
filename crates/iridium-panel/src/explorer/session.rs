//! One editing session: the listing, the kernel editor holding it as text,
//! and what is true while the session is open.
//!
//! The editor is the kernel's own, so every key means here what it means in a
//! document: Enter opens a line at the indentation above, carets multiply,
//! undo walks the tree. The session's one job beside it is to follow the text
//! into the [`Listing`] after every change.

use std::fmt;

use iridium_editor::commands::builtin::{
    EXPLORER_EDIT_MODE, SELECTION_COLLAPSE_TO_PRIMARY, default_registry,
};
use iridium_editor::{CommandArgs, Editor, EditorConfig, Keymap, Position};

use super::buffer::{Listing, LoadError, SourceRow};
use super::keymap::edit_layer;
use super::plan::{Plan, Refusal};

/// The keymap layers an editing session's editor is given above the kernel's
/// own: the face's, the panel's, then the user's.
#[derive(Debug, Clone, Default)]
pub struct EditLayers {
    /// The face's layer, cut down to commands the kernel registers.
    face: Option<Keymap>,
    /// The user's `[keys]` layer, cut down the same way.
    user: Option<Keymap>,
}

impl EditLayers {
    /// Keeps `face` as the layer under the panel's own.
    pub fn set_face(&mut self, face: &Keymap) {
        self.face = runnable(face);
    }

    /// Keeps `user` as the layer over the panel's own.
    pub fn set_user(&mut self, user: &Keymap) {
        self.user = runnable(user);
    }

    /// Pushes every layer onto `editor`, returning what could not be pushed.
    fn install(&self, editor: &mut Editor) -> Vec<String> {
        let mut problems = Vec::new();
        let layers = [self.face.clone(), Some(edit_layer()), self.user.clone()];
        for layer in layers.into_iter().flatten() {
            let name = layer.name().to_owned();
            if let Err(error) = editor.push_keymap(layer) {
                problems.push(format!(
                    "the `{name}` keys are not in use in the file explorer: {error}"
                ));
            }
        }
        problems
    }
}

/// The bindings of `layer` an editor with only the kernel's commands can run.
///
/// A face's own commands — saving a file, say — mean nothing in a listing,
/// and a layer naming one would be refused whole. A suppression is kept.
fn runnable(layer: &Keymap) -> Option<Keymap> {
    let registry = default_registry().ok()?;
    let mut kept = Keymap::new(layer.name().to_owned());
    for binding in layer.bindings() {
        let known = binding
            .command()
            .is_none_or(|command| registry.get(command.as_str()).is_some());
        if known {
            kept.push(binding.clone());
        }
    }
    (!kept.is_empty()).then_some(kept)
}

/// The editor configuration a listing is edited under.
///
/// Two-space indents, matching the projection, so Tab nests a line one level.
/// No auto-pairs and no comment token: neither means anything in a file name.
fn config() -> EditorConfig {
    EditorConfig {
        tab_width: crate::outline::INDENT_WIDTH,
        insert_spaces: true,
        auto_indent: true,
        auto_pairs: false,
        line_comment_token: None,
        ..EditorConfig::default()
    }
}

/// The state of an editing session.
pub struct Editing {
    /// The listing as loaded and as the text now reads it.
    pub listing: Listing,
    /// The kernel editor the text lives in.
    pub editor: Box<Editor>,
    /// Why the last attempt to confirm was turned down, or empty. Cleared by
    /// every change to the text, because it described the text before.
    pub refusals: Vec<Refusal>,
    /// Whether an escape has already been refused for unapplied edits, so the
    /// next one throws them away.
    pub discard_armed: bool,
    /// The document revision the listing last followed.
    revision: u64,
    /// The page height last written to the editor.
    page_rows: usize,
}

impl fmt::Debug for Editing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Editing")
            .field("listing", &self.listing)
            .field("refusals", &self.refusals)
            .field("discard_armed", &self.discard_armed)
            .finish_non_exhaustive()
    }
}

/// The state of a confirmation.
#[derive(Debug)]
pub struct Confirming {
    /// The session that produced the plan, kept whole so going back returns
    /// to the text, its carets and its undo history.
    pub editing: Box<Editing>,
    /// What would happen.
    pub plan: Plan,
}

impl Editing {
    /// A session over `source` with the caret at the end of text line
    /// `start_line`, and any layer that could not be installed, described.
    ///
    /// # Errors
    ///
    /// [`LoadError`] when the rows cannot be written as text.
    pub fn new(
        source: &[SourceRow],
        start_line: usize,
        layers: &EditLayers,
    ) -> Result<(Self, Vec<String>), LoadError> {
        let listing = Listing::load(source)?;
        let mut editor = Box::new(Editor::new(config()));
        editor.set_content(&listing.text());
        editor.set_mode(Some(EXPLORER_EDIT_MODE));
        let problems = layers.install(&mut editor);

        let document = &editor.state().document;
        let line = start_line.min(document.line_count().saturating_sub(1));
        let column = document.line_len(line).unwrap_or(0);
        let revision = document.revision();
        editor.set_cursor(Position::new(line, column));
        Ok((
            Self {
                listing,
                editor,
                refusals: Vec::new(),
                discard_armed: false,
                revision,
                page_rows: 0,
            },
            problems,
        ))
    }

    /// Follows the editor's text into the listing, reporting whether it
    /// changed.
    ///
    /// Reads only the lines the document says changed since the revision
    /// last followed, so a keystroke costs what it touched. When the
    /// document cannot say, or its span does not fit the listing's lines,
    /// every line is read instead: slower, never wrong.
    pub fn sync(&mut self) -> bool {
        let document = &self.editor.state().document;
        if document.revision() == self.revision {
            return false;
        }
        let change = document
            .lines_changed_since(self.revision)
            .filter(|change| {
                change.old_end <= self.listing.line_count()
                    && self.listing.line_count() - change.old_end + change.new_end
                        == document.line_count()
            });
        self.revision = document.revision();
        let (start, old_end, new_end) = change.map_or_else(
            || (0, self.listing.line_count(), document.line_count()),
            |change| (change.start, change.old_end, change.new_end),
        );
        let lines = (start..new_end)
            .map(|line| document.line(line).unwrap_or_default())
            .collect();
        let changed = self.listing.edit(start, old_end, lines);
        if changed {
            self.refusals.clear();
            self.discard_armed = false;
        }
        changed
    }

    /// Tells the editor how many lines a page is, when that changed.
    ///
    /// Guarded because writing the viewport resets the editor's sticky column.
    pub fn set_page_rows(&mut self, rows: usize) {
        if rows != self.page_rows {
            self.page_rows = rows;
            self.editor.state_mut().viewport.visible_lines = rows;
        }
    }

    /// Drops every caret but the primary one, reporting whether there were
    /// others to drop.
    pub fn collapse_carets(&mut self) -> bool {
        if self.editor.state().cursor.cursor_count() <= 1 {
            return false;
        }
        self.editor
            .run_command(SELECTION_COLLAPSE_TO_PRIMARY.as_str(), CommandArgs::NONE)
            .is_ok()
    }

    /// Puts the caret at the end of text line `line`.
    pub fn place_caret(&mut self, line: usize) {
        let document = &self.editor.state().document;
        let line = line.min(document.line_count().saturating_sub(1));
        let column = document.line_len(line).unwrap_or(0);
        self.editor.set_cursor(Position::new(line, column));
    }

    /// The line the primary caret is on.
    #[must_use]
    pub fn caret_line(&self) -> usize {
        self.editor.state().cursor.primary.head.line
    }

    /// Arms the next escape to throw the edits away, and reports whether it
    /// was already armed.
    pub const fn arm_discard(&mut self) -> bool {
        let armed = self.discard_armed;
        self.discard_armed = true;
        armed
    }
}
