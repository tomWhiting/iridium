//! Reachable key hints, using the host's modifier convention.
use super::FileExplorer;
use crate::{PanelRow, Span};
use iridium_editor::theme::Theme;
use iridium_editor::{CommandId, KeyHintIndex, KeyLabelStyle, ModifierState};

impl FileExplorer {
    /// Explains a refused close using the current discard binding.
    #[must_use]
    pub fn unapplied_edits_message(&self) -> String {
        self.discard_message("twice")
    }

    pub(super) fn discard_message(&self, repetition: &str) -> String {
        self.key_hint(&iridium_editor::commands::builtin::EXPLORER_EDIT_LEAVE)
            .map_or_else(
                || "unapplied edits — apply them or bind explorer.edit.leave to discard".to_owned(),
                |key| format!("unapplied edits — {key} {repetition} to discard"),
            )
    }

    /// Selects how this host displays keyboard modifiers.
    pub const fn set_key_label_style(&mut self, style: KeyLabelStyle) {
        self.key_label_style = style;
    }

    pub(super) fn rebuild_hints(&mut self) {
        self.hints = KeyHintIndex::build(&self.keys);
    }

    /// Prefer the user's layer, then the host's native modifier spelling.
    /// A suppressed, shadowed or wildcard binding is never advertised.
    pub(super) fn key_hint(&self, command: &CommandId) -> Option<&str> {
        let candidates = self.hints.hints_for(command.as_str());
        let first = candidates.iter().find(|hint| !hint.is_wildcard())?;
        let preferred = candidates
            .iter()
            .filter(|hint| !hint.is_wildcard() && hint.layer() == first.layer())
            .find(|hint| {
                let meta = hint
                    .sequence()
                    .iter()
                    .any(|stroke| stroke.modifiers.meta == ModifierState::Required);
                match self.key_label_style {
                    KeyLabelStyle::Portable | KeyLabelStyle::MacGlyphsCommandAsCtrl => !meta,
                    KeyLabelStyle::MacGlyphsCommandAsMeta => meta,
                }
            })
            .unwrap_or(first);
        Some(preferred.label(self.key_label_style))
    }

    pub(super) fn hints_text(&self, commands: &[(CommandId, &str)]) -> String {
        commands
            .iter()
            .filter_map(|(command, action)| {
                self.key_hint(command).map(|key| format!("{key} {action}"))
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub(super) fn hints_row(
        &self,
        commands: &[(CommandId, &str)],
        theme: &Theme,
        width: usize,
    ) -> PanelRow {
        PanelRow::new(vec![Span::new(
            super::rows::truncate(&self.hints_text(commands), width),
            theme.editor.line_number,
        )])
    }
}
