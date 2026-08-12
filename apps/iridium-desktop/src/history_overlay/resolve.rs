//! Turning a keypress into a verb the panel can run.
//!
//! The panel resolves against [its own stack](super::panel::HistoryPanel):
//! [`super::keymap::default_keymap`] at the base, and the user's `[keys]`
//! bindings pushed on top by the face that read the configuration file.
//!
//! # ⭐ Only this panel's own commands are taken from the user's layer
//!
//! A user's `[keys]` layer holds all their bindings, and most are editor ones
//! with no mode. Pushed into this stack whole, a mode-free binding would apply
//! in **every** mode — so somebody who bound `end` to `edit.selectToLineEnd` for
//! the document would find `End` had stopped reaching the newest state, having
//! never mentioned this panel.
//!
//! [`HistoryPanel::set_user_keymap`] therefore keeps only the bindings whose
//! command is one of this panel's verbs, and scopes each one to
//! [`HISTORY_MODE`]. The mode is a property of the command the user named, not a
//! second thing for them to keep in step.
//!
//! # ⚠️ An unbind is the one thing that cannot be scoped, and is not dropped
//!
//! `"escape" = ""` suppresses a sequence and carries **no command**, so there is
//! nothing to read a verb off. Dropping it would mean a user could unbind a key
//! everywhere except this panel; guessing a verb for it would be inventing
//! intent. It is passed through mode-free, so it suppresses that sequence here
//! too — the honest reading of what was written. The panel stays closable
//! regardless, because `history.togglePanel` is the editor stack's and the face
//! answers it whatever the panel thinks.

use iridium_editor::commands::builtin::HISTORY_MODE;
use iridium_editor::{KeyEvent, KeyPress, Keymap, KeymapStack};

use super::keymap::default_keymap;
use super::panel::HistoryPanel;
use super::verb::Verb;

/// The name the user's bindings are pushed under inside the panel.
///
/// Distinct from `iridium_config`'s `"user"` because this is a *filtered*
/// projection of that layer, and a diagnostic naming it should not claim to be
/// quoting the layer the editor holds.
pub const USER_LAYER_NAME: &str = "user-history";

/// What one keypress came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Resolved {
    /// A binding matched, and named a verb this panel owns.
    Verb(Verb),
    /// The strokes so far begin a longer sequence; nothing has run yet.
    Pending,
    /// Nothing claimed the key.
    ///
    /// ⚠️ Unlike the palette and the explorer, this panel has no field, so there
    /// is nothing for an unclaimed key to fall through *to*. It is swallowed,
    /// which is what being modal means — and that is why this enum has no
    /// separate `Abandoned`: the two would be acted on identically, and a
    /// distinction nothing reads is a distinction that will drift.
    Unclaimed,
}

impl HistoryPanel {
    /// Resolves `event` against the panel's keymap.
    ///
    /// Pushes the stroke onto the pending sequence first, so a multi-stroke
    /// binding a user wrote is reachable. Almost always the sequence is one
    /// stroke long and is cleared again before returning.
    pub(super) fn resolve_key(&mut self, event: &KeyEvent) -> Resolved {
        self.pending.push(KeyPress::new(event.key, event.modifiers));

        if let Some(binding) = self.keys.exact_match(&self.pending, Some(&HISTORY_MODE)) {
            // A suppression matches and names no command: the sequence is
            // deliberately bound to nothing, which the panel treats exactly as
            // it treats a key nobody claimed — both do nothing, and here that
            // is the same nothing.
            let verb = binding.command().and_then(Verb::from_id);
            self.pending.clear();
            return verb.map_or(Resolved::Unclaimed, Resolved::Verb);
        }

        if self
            .keys
            .has_continuation(&self.pending, Some(&HISTORY_MODE))
        {
            return Resolved::Pending;
        }

        self.pending.clear();
        Resolved::Unclaimed
    }

    /// Replaces the user's bindings inside the panel with those in `user`.
    ///
    /// Only bindings naming one of this panel's verbs are kept, each scoped to
    /// [`HISTORY_MODE`]; suppressions are kept unscoped. See the module
    /// documentation for why both of those are the way they are.
    ///
    /// Rebuilds the whole stack rather than popping a layer, so calling it twice
    /// — which a configuration reload does — cannot stack two copies of the
    /// user's bindings.
    pub fn set_user_keymap(&mut self, user: &Keymap) {
        let mut layer = Keymap::new(USER_LAYER_NAME);
        for binding in user.bindings() {
            match binding.command() {
                Some(id) => {
                    if Verb::from_id(id).is_some() {
                        layer.push(binding.clone().in_mode(HISTORY_MODE));
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

    /// Whether a multi-stroke sequence is part-way through.
    ///
    /// Exposed so a face can say so on screen, as the editor's pending-chord
    /// indicator does.
    #[must_use]
    pub fn has_pending_keys(&self) -> bool {
        !self.pending.is_empty()
    }
}
