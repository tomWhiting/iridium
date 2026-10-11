//! Focused field operations do not mutate document cursors or history.

use super::{Field, Focus, Motion, SearchOutcome, SearchOverlay};
use iridium_editor::Editor;

/// A field cut captured before any clipboard write. Contents are private so
/// completion can validate the original field, selection and focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCut {
    field: Field,
    focus: Focus,
    text: String,
}

impl SearchCut {
    /// Exact selected text to deliver to the clipboard.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl SearchOverlay {
    /// Deletes the captured selection only after the host has stored its text.
    ///
    /// # Errors
    /// Refuses a changed field, selection or focus; no field text is deleted.
    pub fn finish_cut(&mut self, cut: &SearchCut, editor: &mut Editor) -> Result<(), String> {
        let field = match self.focus {
            Focus::Find => &self.query,
            Focus::Replace => &self.replacement,
        };
        if self.focus != cut.focus || field != &cut.field {
            return Err("Copied text, but the field changed; cut cancelled".into());
        }
        self.edit_field(editor, Field::delete_selection, true);
        Ok(())
    }

    pub(super) fn move_field(
        &mut self,
        editor: &mut Editor,
        motion: Motion,
        extend: bool,
    ) -> SearchOutcome {
        self.edit(editor, |field| field.navigate(motion, extend), false)
    }

    pub(super) fn copy_field(&self, cut: bool) -> SearchOutcome {
        let field = match self.focus {
            Focus::Find => &self.query,
            Focus::Replace => &self.replacement,
        };
        let Some(text) = field.selected_text().map(str::to_owned) else {
            return SearchOutcome::Handled;
        };
        if cut {
            return SearchOutcome::CutRequested(SearchCut {
                field: field.clone(),
                focus: self.focus,
                text,
            });
        }
        SearchOutcome::Copied(text)
    }
}
