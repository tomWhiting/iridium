//! Input action types.
//!
//! Defines the actions that can result from input events.

use crate::editor::Position;

/// Actions that can be performed by the editor.
///
/// These represent high-level editor operations triggered by input events.
#[derive(Debug, Clone, PartialEq)]
pub enum InputAction {
    // Cursor movement (no selection)
    /// Move cursor left by one character.
    MoveLeft,
    /// Move cursor right by one character.
    MoveRight,
    /// Move cursor up by one line.
    MoveUp,
    /// Move cursor down by one line.
    MoveDown,
    /// Move cursor to the start of the line.
    MoveToLineStart,
    /// Move cursor to the end of the line.
    MoveToLineEnd,
    /// Move cursor left by one word.
    MoveWordLeft,
    /// Move cursor right by one word.
    MoveWordRight,
    /// Move cursor up by one page.
    PageUp,
    /// Move cursor down by one page.
    PageDown,
    /// Move cursor to the start of the document.
    MoveToDocumentStart,
    /// Move cursor to the end of the document.
    MoveToDocumentEnd,

    // Selection (extend selection)
    /// Extend selection left by one character.
    SelectLeft,
    /// Extend selection right by one character.
    SelectRight,
    /// Extend selection up by one line.
    SelectUp,
    /// Extend selection down by one line.
    SelectDown,
    /// Extend selection to the start of the line.
    SelectToLineStart,
    /// Extend selection to the end of the line.
    SelectToLineEnd,
    /// Extend selection left by one word.
    SelectWordLeft,
    /// Extend selection right by one word.
    SelectWordRight,
    /// Select all text.
    SelectAll,
    /// Extend selection up by one page.
    SelectPageUp,
    /// Extend selection down by one page.
    SelectPageDown,
    /// Extend selection to the start of the document.
    SelectToDocumentStart,
    /// Extend selection to the end of the document.
    SelectToDocumentEnd,

    // Text editing
    /// Insert a character.
    InsertChar(char),
    /// Insert a string of text.
    InsertText(String),
    /// Insert a newline (Enter key).
    InsertNewline,
    /// Delete the character before the cursor (Backspace).
    Backspace,
    /// Delete the character after the cursor (Delete).
    Delete,
    /// Delete the word before the cursor.
    DeleteWordBefore,
    /// Delete the word after the cursor.
    DeleteWordAfter,
    /// Delete the entire line.
    DeleteLine,

    // Clipboard
    /// Cut selection to clipboard.
    Cut,
    /// Copy selection to clipboard.
    Copy,
    /// Paste from clipboard.
    Paste,

    // History
    /// Undo the last action.
    Undo,
    /// Redo the last undone action.
    Redo,

    // Mouse actions
    /// Click at a position to place cursor.
    Click(Position),
    /// Click at a position while holding shift to extend selection.
    ShiftClick(Position),
    /// Start a drag selection from a position.
    DragStart(Position),
    /// Continue drag selection to a position.
    DragMove(Position),
    /// End drag selection.
    DragEnd,
    /// Double-click to select word.
    DoubleClick(Position),
    /// Triple-click to select line.
    TripleClick(Position),

    // IME
    /// Begin IME composition.
    ImeCompositionStart,
    /// Update IME composition text.
    ImeCompositionUpdate(String),
    /// Commit IME composition.
    ImeCompositionEnd(String),
    /// Cancel IME composition.
    ImeCompositionCancel,

    // Other
    /// No action (input was not handled).
    None,
}

impl InputAction {
    /// Returns true if this action modifies the document.
    #[must_use]
    pub fn modifies_content(&self) -> bool {
        matches!(
            self,
            InputAction::InsertChar(_)
                | InputAction::InsertText(_)
                | InputAction::InsertNewline
                | InputAction::Backspace
                | InputAction::Delete
                | InputAction::DeleteWordBefore
                | InputAction::DeleteWordAfter
                | InputAction::DeleteLine
                | InputAction::Cut
                | InputAction::Paste
                | InputAction::Undo
                | InputAction::Redo
                | InputAction::ImeCompositionEnd(_)
        )
    }

    /// Returns true if this action changes the selection.
    #[must_use]
    pub fn modifies_selection(&self) -> bool {
        matches!(
            self,
            InputAction::MoveLeft
                | InputAction::MoveRight
                | InputAction::MoveUp
                | InputAction::MoveDown
                | InputAction::MoveToLineStart
                | InputAction::MoveToLineEnd
                | InputAction::MoveWordLeft
                | InputAction::MoveWordRight
                | InputAction::PageUp
                | InputAction::PageDown
                | InputAction::MoveToDocumentStart
                | InputAction::MoveToDocumentEnd
                | InputAction::SelectLeft
                | InputAction::SelectRight
                | InputAction::SelectUp
                | InputAction::SelectDown
                | InputAction::SelectToLineStart
                | InputAction::SelectToLineEnd
                | InputAction::SelectWordLeft
                | InputAction::SelectWordRight
                | InputAction::SelectAll
                | InputAction::SelectPageUp
                | InputAction::SelectPageDown
                | InputAction::SelectToDocumentStart
                | InputAction::SelectToDocumentEnd
                | InputAction::Click(_)
                | InputAction::ShiftClick(_)
                | InputAction::DragStart(_)
                | InputAction::DragMove(_)
                | InputAction::DragEnd
                | InputAction::DoubleClick(_)
                | InputAction::TripleClick(_)
        )
    }
}

/// Result of processing an input event.
#[derive(Debug, Clone)]
pub struct InputResult {
    /// The action(s) to perform.
    pub actions: Vec<InputAction>,
    /// Whether the input was consumed (prevents propagation).
    pub consumed: bool,
}

impl InputResult {
    /// Creates a result with a single action.
    #[must_use]
    pub fn action(action: InputAction) -> Self {
        Self {
            actions: vec![action],
            consumed: true,
        }
    }

    /// Creates a result with multiple actions.
    #[must_use]
    pub fn actions(actions: Vec<InputAction>) -> Self {
        Self {
            actions,
            consumed: true,
        }
    }

    /// Creates a result indicating the input was not handled.
    #[must_use]
    pub fn not_handled() -> Self {
        Self {
            actions: vec![InputAction::None],
            consumed: false,
        }
    }

    /// Creates a result with no actions but the input was consumed.
    #[must_use]
    pub fn consumed() -> Self {
        Self {
            actions: vec![],
            consumed: true,
        }
    }

    /// Returns true if any action modifies content.
    #[must_use]
    pub fn will_modify_content(&self) -> bool {
        self.actions.iter().any(InputAction::modifies_content)
    }

    /// Returns true if any action modifies selection.
    #[must_use]
    pub fn will_modify_selection(&self) -> bool {
        self.actions.iter().any(InputAction::modifies_selection)
    }
}

impl Default for InputResult {
    fn default() -> Self {
        Self::not_handled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_action_modifies_content() {
        assert!(InputAction::InsertChar('a').modifies_content());
        assert!(InputAction::Backspace.modifies_content());
        assert!(InputAction::Paste.modifies_content());
        assert!(!InputAction::MoveLeft.modifies_content());
        assert!(!InputAction::SelectAll.modifies_content());
    }

    #[test]
    fn test_input_action_modifies_selection() {
        assert!(InputAction::MoveLeft.modifies_selection());
        assert!(InputAction::SelectAll.modifies_selection());
        assert!(InputAction::Click(Position::origin()).modifies_selection());
        assert!(!InputAction::InsertChar('a').modifies_selection());
        assert!(!InputAction::Backspace.modifies_selection());
    }

    #[test]
    fn test_input_result_action() {
        let result = InputResult::action(InputAction::MoveLeft);
        assert_eq!(result.actions.len(), 1);
        assert!(result.consumed);
    }

    #[test]
    fn test_input_result_not_handled() {
        let result = InputResult::not_handled();
        assert!(!result.consumed);
    }

    #[test]
    fn test_input_result_will_modify_content() {
        let result = InputResult::action(InputAction::InsertChar('a'));
        assert!(result.will_modify_content());

        let result = InputResult::action(InputAction::MoveLeft);
        assert!(!result.will_modify_content());
    }
}
