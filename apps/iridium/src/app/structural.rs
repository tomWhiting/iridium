//! Palette access to the terminal's structural navigation preset.
use iridium_editor::commands::ModeName;
use iridium_editor::input::AstRequest;

use super::{App, Flow};

impl App {
    pub(super) fn enter_structural_navigation(&mut self, mode: ModeName) -> Flow {
        if let Some(editor) = self.workspace.active_editor_mut() {
            editor.perform_ast_request(AstRequest::SelectNode);
            editor.set_mode(Some(mode));
        }
        self.ensure_caret_visible();
        Flow::Running
    }
}

impl App {
    /// Enter action-oriented selection mode without snapping the selected range.
    pub(super) fn select_mode(&mut self) {
        let mode = iridium_editor::commands::structural_keymap::STRUCTURAL;
        if self.startup_mode == super::modal::StartupMode::Structural
            && let Some(editor) = self.workspace.active_editor_mut()
            && editor.mode() != Some(&mode)
            && editor.mode() != Some(&iridium_editor::commands::structural_keymap::STRUCTURAL_VERB)
        {
            editor.set_mode(Some(mode));
        }
    }
}
