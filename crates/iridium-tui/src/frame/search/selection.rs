//! Hand the search's matches back to the document as editable selections.

use iridium_editor::Editor;

use super::{Feedback, SearchOutcome, SearchOverlay};

impl SearchOverlay {
    pub(super) fn select_matches(&mut self, editor: &mut Editor) -> SearchOutcome {
        match editor.select_search_matches() {
            Ok(0) => {
                self.feedback = Feedback::NoSelection;
                SearchOutcome::Handled
            },
            Ok(count) => {
                self.close(editor);
                SearchOutcome::Selected(count)
            },
            Err(message) => {
                self.error = Some(message);
                SearchOutcome::Handled
            },
        }
    }
}
