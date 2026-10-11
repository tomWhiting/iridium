//! Project fields use the app clipboard transport and shared binding resolver.
use crate::app::{App, clipboard::CutTarget};
use iridium_editor::commands::builtin::{CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE};
use iridium_editor::{
    ClipboardOperation, CommandId, KeyBinding, KeyCode, KeyEvent, KeyPress, Keymap, KeymapStack,
    ModeName, ModifierPattern, Modifiers, StrokePattern,
};
const MODE: ModeName = ModeName::from_static("project-field");
#[derive(Default)]
pub(super) struct Keys {
    stack: KeymapStack,
    pending: Vec<KeyPress>,
}
enum Resolved {
    Unclaimed,
    Handled,
    Command(CommandId),
    Invalid,
}
impl Keys {
    pub fn clear(&mut self) {
        self.pending.clear();
    }
    pub fn set_user_keymap(&mut self, user: &Keymap) {
        let mut base = Keymap::new("project-field-clipboard");
        for modifiers in [
            Modifiers::ctrl(),
            Modifiers {
                meta: true,
                ..Modifiers::none()
            },
        ] {
            for (key, id) in [
                ('c', CLIPBOARD_COPY),
                ('x', CLIPBOARD_CUT),
                ('v', CLIPBOARD_PASTE),
            ] {
                base.push(
                    KeyBinding::new(
                        StrokePattern::new(KeyCode::Char(key), ModifierPattern::exact(modifiers)),
                        &[],
                        id,
                    )
                    .in_mode(MODE),
                );
            }
        }
        let mut layer = Keymap::new("user-project-field-clipboard");
        for binding in user.bindings() {
            if binding
                .command()
                .is_none_or(|id| [CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE].contains(id))
            {
                layer.push(binding.clone().in_mode(MODE));
            }
        }
        self.stack = KeymapStack::with_base(base);
        if !layer.is_empty() {
            self.stack.push(layer);
        }
        self.clear();
    }
    fn resolve(&mut self, event: &KeyEvent) -> Resolved {
        if !self.pending.is_empty() && event.key == KeyCode::Escape {
            self.clear();
            return Resolved::Handled;
        }
        self.pending.push(KeyPress::new(event.key, event.modifiers));
        if let Some(binding) = self.stack.exact_match(&self.pending, Some(&MODE)) {
            let result = if !binding.following_commands().is_empty()
                || binding.enters_mode().is_some()
                || binding.accepts_count()
                || binding.records_input()
            {
                Resolved::Invalid
            } else {
                binding
                    .command()
                    .cloned()
                    .map_or(Resolved::Handled, Resolved::Command)
            };
            self.clear();
            return result;
        }
        if self.stack.has_continuation(&self.pending, Some(&MODE)) {
            return Resolved::Handled;
        }
        self.clear();
        Resolved::Unclaimed
    }
}
impl App {
    pub(super) fn drive_project_clipboard(&mut self, event: &KeyEvent) -> bool {
        let Some(panel) = self.project_search.as_mut() else {
            return false;
        };
        if panel.clipboard_field().is_none() {
            panel.clipboard_keys.clear();
            return false;
        }
        let command = match panel.clipboard_keys.resolve(event) {
            Resolved::Unclaimed => return false,
            Resolved::Handled => return true,
            Resolved::Invalid => {
                panel.clipboard_notice("Project clipboard bindings cannot compose document commands, counts or mode changes".into());
                return true;
            },
            Resolved::Command(id) => id,
        };
        let Some(field) = panel.clipboard_field() else {
            return true;
        };
        if command == CLIPBOARD_PASTE {
            self.message = None;
            self.clipboard(ClipboardOperation::Paste);
        } else if let Some(text) = field.selected_text().map(str::to_owned) {
            let cut = if command == CLIPBOARD_CUT {
                field
                    .capture_cut()
                    .map(|cut| CutTarget::Project(Box::new(cut)))
            } else {
                None
            };
            self.message = None;
            self.store_clipboard(text, cut);
        }
        self.project_clipboard_notice();
        true
    }
    pub(in crate::app) fn project_clipboard_notice(&mut self) {
        if let Some(panel) = self.project_search.as_mut().filter(|p| p.open) {
            if let Some(message) = &self.message {
                panel.clipboard_notice(message.text().to_owned());
            } else if self.pending_clipboard.is_some() {
                panel.clipboard_notice(
                    "Waiting for terminal clipboard delivery; cut not applied yet".into(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
