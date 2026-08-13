//! Turning a keypress into a command the panel can run.
//!
//! The panel resolves against [its own stack](super::panel::FileExplorer::keys):
//! [`super::keymap::default_keymap`] at the base, and the user's `[keys]`
//! bindings pushed on top by whichever face read the configuration file.
//!
//! # Which mode a key is resolved in is *which screen is showing*
//!
//! [`FileExplorer::screen_mode`] answers it, and it asks the same three
//! questions in the same order the old `handle_edit_key` did — a plan exists
//! only while confirming, a refusal only while one is on screen, a cursor only
//! while the rows are. The modes are not state the panel maintains beside its
//! screens; they are the screens, named.
//!
//! # ⭐ Only the panel's own commands are taken from the user's layer
//!
//! A user's `[keys]` layer holds all their bindings, and most are editor ones
//! with no mode. Pushed into this stack whole, a mode-free binding would apply
//! in **every** mode — so somebody who bound `tab` to `edit.indent` for the
//! document would find `Tab` had stopped opening the oil buffer, having never
//! mentioned the explorer.
//!
//! [`FileExplorer::set_user_keymap`] therefore keeps only the bindings whose
//! command belongs to one of this panel's modes, and scopes each one to the mode
//! its command declares. That is ruling D-2 doing its work: the user writes
//!
//! ```toml
//! [keys]
//! "ctrl+j" = "explorer.moveDown"
//! ```
//!
//! and gets a binding that fires in the explorer and nowhere else, without
//! having said so — because the mode is a property of the command they named,
//! not a second thing for them to keep in step.
//!
//! # ⚠️ An unbind is the one thing that cannot be scoped, and is not dropped
//!
//! `"escape" = ""` suppresses a sequence and carries **no command**, so there is
//! nothing to read a mode off. Dropping it would mean a user could unbind a key
//! everywhere except the panel, which is the surprise this whole task exists to
//! remove; guessing a mode for it would be inventing intent.
//!
//! It is passed through mode-free, so it suppresses that sequence on every
//! screen of the panel — which is the honest reading of what was written: the
//! user said this key does nothing, and it does nothing here too. The panel
//! stays closable regardless, because `explorer.togglePanel` is the editor
//! stack's and the face answers it whatever the panel thinks (D-6).

use iridium_editor::commands::builtin::{EXPLORER_MODE, panel_command_metas};
use iridium_editor::{CommandId, KeyEvent, KeyPress, Keymap, KeymapStack, ModeName, Modifiers};

use super::keymap::default_keymap;
use super::panel::FileExplorer;

/// The name the user's bindings are pushed under inside the panel.
///
/// Distinct from `iridium_config`'s `"user"` because this is a *filtered*
/// projection of that layer, and a diagnostic naming it should not claim to be
/// quoting the layer the editor holds.
pub const USER_LAYER_NAME: &str = "user-explorer";

/// What one keypress came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Resolved {
    /// A binding matched, and named this command.
    Command(CommandId),
    /// The strokes so far begin a longer sequence; nothing has run yet.
    Pending,
    /// Nothing claimed the key. The screen decides what an unclaimed key means.
    Unclaimed,
    /// A sequence in progress was abandoned by a key that cannot continue it.
    ///
    /// Distinct from [`Self::Unclaimed`] on purpose: that key was typed as part
    /// of a chord, not as text, and feeding it to the query field would put a
    /// character in a filter the user was not editing.
    Abandoned,
    /// A binding matched and named no command: the sequence is bound to nothing.
    ///
    /// ⛔ **Distinct from [`Self::Unclaimed`], and it shipped folded into it.**
    /// Both the browsing filter and the rename field take an unclaimed
    /// printable key as text, so `"a" = ""` filtered — or renamed — by the very
    /// character the user had asked the editor to stop reacting to. The comment
    /// in [`FileExplorer::resolve_key`] had said as much for months while the
    /// code did the opposite.
    Suppressed,
}

impl FileExplorer {
    /// The mode naming the screen currently showing.
    ///
    /// The same three questions in the same order the editing key handler asks,
    /// because they are the same three screens.
    pub(super) fn screen_mode(&self) -> ModeName {
        if !self.mode.is_editing() {
            return EXPLORER_MODE;
        }
        if self.mode.plan().is_some() {
            return iridium_editor::commands::builtin::EXPLORER_CONFIRM_MODE;
        }
        if !self.mode.refusals().is_empty() {
            return iridium_editor::commands::builtin::EXPLORER_REFUSED_MODE;
        }
        iridium_editor::commands::builtin::EXPLORER_EDIT_MODE
    }

    /// Resolves `event` against the panel's keymap in the current screen's mode.
    ///
    /// Pushes the stroke onto the pending sequence first, so a multi-stroke
    /// binding a user wrote is reachable. Almost always the sequence is one
    /// stroke long and is cleared again before returning.
    pub(super) fn resolve_key(&mut self, event: &KeyEvent) -> Resolved {
        let mode = self.screen_mode();
        self.pending.push(KeyPress::new(event.key, event.modifiers));

        if let Some(binding) = self.keys.exact_match(&self.pending, Some(&mode)) {
            // A suppression matches and names no command: the sequence is
            // deliberately bound to nothing, which is not the same as nothing
            // having claimed it. Both end the sequence; only one reaches the
            // field.
            let command = binding.command().cloned();
            self.pending.clear();
            return command.map_or(Resolved::Suppressed, Resolved::Command);
        }

        if self.keys.has_continuation(&self.pending, Some(&mode)) {
            return Resolved::Pending;
        }

        let mid_sequence = self.pending.len() > 1;
        self.pending.clear();
        if mid_sequence {
            Resolved::Abandoned
        } else {
            Resolved::Unclaimed
        }
    }

    /// Replaces the user's bindings inside the panel with those in `user`.
    ///
    /// Only bindings naming one of this panel's commands are kept, each scoped
    /// to the mode its command declares; suppressions are kept unscoped. See the
    /// module documentation for why both of those are the way they are.
    ///
    /// Rebuilds the whole stack rather than popping a layer, so calling it twice
    /// — which a configuration reload does — cannot stack two copies of the
    /// user's bindings.
    pub fn set_user_keymap(&mut self, user: &Keymap) {
        let mut layer = Keymap::new(USER_LAYER_NAME);
        for binding in user.bindings() {
            match binding.command() {
                Some(id) => {
                    if let Some(mode) = panel_mode_of(id) {
                        layer.push(binding.clone().in_mode(mode));
                    }
                },
                None => layer.push(binding.clone()),
            }
        }

        let mut stack = KeymapStack::with_base(default_keymap());
        if !layer.is_empty() {
            stack.push(layer);
        }
        self.keys = stack;
        // A sequence half-typed against the old bindings means nothing against
        // the new ones.
        self.pending.clear();
    }

    /// Forgets any half-typed multi-stroke sequence.
    ///
    /// ⚠️ **A face must call this when the panel loses focus**, for the reason
    /// the editor's own resolver is reset on blur: a leader pressed before the
    /// user clicked away would otherwise still be waiting when they came back,
    /// and the next key they typed would be read as its continuation.
    pub fn abandon_pending_keys(&mut self) {
        self.pending.clear();
    }

    /// Whether a multi-stroke sequence is part-way through.
    ///
    /// Exposed so a face can say so on screen, as the editor's pending-chord
    /// indicator does.
    #[must_use]
    pub fn has_pending_keys(&self) -> bool {
        !self.pending.is_empty()
    }
}

/// Whether a keypress is somebody typing a character rather than pressing a
/// chord.
///
/// ⛔ **The guard the fall-through cannot do without.** An unclaimed key is only
/// text if no modifier that changes its meaning is held: `Ctrl+S` while the
/// explorer is up carries `KeyCode::Char('s')`, and a fall-through that read the
/// character alone would put an `s` in the query — which is the panel silently
/// inventing input from a chord it was asked to swallow.
///
/// Shift is not consulted, because shift is how a capital is typed. `AltGraph`
/// is not consulted either, and does not need to be: the layouts that compose
/// with it report `Ctrl+Alt` held, which this already refuses — exactly as the
/// `chord` table it replaced did.
pub(super) const fn types_text(modifiers: Modifiers) -> bool {
    !modifiers.ctrl && !modifiers.alt && !modifiers.meta
}

/// The mode `id` belongs to, or `None` when it is not this panel's to bind.
///
/// Read off the command's own metadata rather than its id's prefix, because
/// `explorer.togglePanel` shares the prefix and is a genuine global — it is how
/// the panel is *opened*, so scoping it into the mode it exists to enter would
/// make it unreachable from anywhere the user could press it.
fn panel_mode_of(id: &CommandId) -> Option<ModeName> {
    panel_command_metas()
        .find(|meta| meta.id() == id)
        .and_then(|meta| meta.mode().cloned())
}
