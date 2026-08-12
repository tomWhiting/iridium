//! What a key press does while the undo tree is open.
//!
//! One entry point — [`HistoryPanel::handle_key`] — which resolves the press
//! against the panel's keymap and runs whatever verb that named.
//!
//! # There is no fall-through, and that is the whole difference
//!
//! The palette and the file explorer both have a field, so an unclaimed
//! printable character is text and needs a modifier guard before it can be
//! treated as one. This panel is a list and nothing else: an unclaimed key is
//! swallowed, which is what being modal means. Nothing here needs to ask what a
//! character *meant*.

use iridium_editor::history::tree_view::linearize;
use iridium_editor::{Editor, KeyEvent};

use super::panel::{HistoryOutcome, HistoryPanel, isize_of};
use super::resolve::Resolved;
use super::verb::Verb;

impl HistoryPanel {
    /// Handles one key press. Every key is consumed; see the module docs.
    pub fn handle_key(&mut self, event: &KeyEvent, editor: &Editor) -> HistoryOutcome {
        match self.resolve_key(event) {
            Resolved::Verb(verb) => self.run(verb, editor),
            // Modal: a stroke part-way through a sequence, and one nothing
            // claimed, are both swallowed rather than passed to the document.
            Resolved::Pending | Resolved::Unclaimed => HistoryOutcome::Handled,
        }
    }

    /// Carries out one verb.
    ///
    /// The rows are linearized once here rather than per arm: every verb but
    /// [`Verb::Dismiss`] needs them, and the snapshot must be the same one the
    /// selection is moved against and the jump is resolved from.
    fn run(&mut self, verb: Verb, editor: &Editor) -> HistoryOutcome {
        if matches!(verb, Verb::Dismiss) {
            return HistoryOutcome::Closed;
        }
        let snapshot = editor.history_snapshot();
        let rows = linearize(&snapshot);
        let page = isize_of(self.selection.page());
        match verb {
            // Answered above, before the snapshot was taken.
            Verb::Dismiss => HistoryOutcome::Closed,
            // ⚠️ The panel deliberately stays open. See the kernel's
            // `HISTORY_JUMP` for why that is part of the verb.
            Verb::Jump => self
                .selection
                .selected_node(&rows)
                .map_or(HistoryOutcome::Handled, HistoryOutcome::Jump),
            Verb::SelectPrevious => self.move_selection(&rows, -1),
            Verb::SelectNext => self.move_selection(&rows, 1),
            Verb::SelectPageUp => self.move_selection(&rows, -page),
            Verb::SelectPageDown => self.move_selection(&rows, page),
            // `isize::MIN` and `MAX` are how the selection model spells "as far
            // as it goes"; it clamps, so there is no separate first/last verb
            // for it to answer.
            Verb::SelectFirst => self.move_selection(&rows, isize::MIN),
            Verb::SelectLast => self.move_selection(&rows, isize::MAX),
        }
    }
}
