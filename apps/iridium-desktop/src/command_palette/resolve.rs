//! Turning a keypress into a verb the palette can run.
//!
//! The panel resolves against [its own stack](super::panel::CommandPalette):
//! [`super::keymap::default_keymap`] at the base, and the user's `[keys]`
//! bindings pushed on top by the face that read the configuration file.
//!
//! # ⭐ Only the palette's own commands are taken from the user's layer
//!
//! A user's `[keys]` layer holds all their bindings, and most are editor ones
//! with no mode. Pushed into this stack whole, a mode-free binding would apply
//! in **every** mode — so somebody who bound `backspace` to `edit.deleteToLineEnd`
//! for the document would find `Backspace` had stopped deleting in the palette's
//! query, having never mentioned the palette.
//!
//! [`CommandPalette::set_user_keymap`] therefore keeps only the bindings whose
//! command is one of this panel's verbs, and scopes each one to
//! [`PALETTE_MODE`]. The user writes
//!
//! ```toml
//! [keys]
//! "ctrl+j" = "palette.selectNext"
//! ```
//!
//! and gets a binding that fires in the palette and nowhere else, without having
//! said so — because the mode is a property of the command they named, not a
//! second thing for them to keep in step.
//!
//! # ⚠️ An unbind is the one thing that cannot be scoped, and is not dropped
//!
//! `"escape" = ""` suppresses a sequence and carries **no command**, so there is
//! nothing to read a verb off. Dropping it would mean a user could unbind a key
//! everywhere except this panel, which is the surprise this task exists to
//! remove; guessing a verb for it would be inventing intent.
//!
//! It is passed through mode-free, so it suppresses that sequence in the palette
//! too — the honest reading of what was written. The palette stays closable
//! regardless, because `palette.open` is the editor stack's and the face
//! answers it whatever the panel thinks.

use iridium_editor::commands::builtin::PALETTE_MODE;
use iridium_editor::{KeyEvent, KeyPress, Keymap, KeymapStack, Modifiers};

use super::keymap::default_keymap;
use super::panel::CommandPalette;
use super::verb::Verb;

/// The name the user's bindings are pushed under inside the panel.
///
/// Distinct from `iridium_config`'s `"user"` because this is a *filtered*
/// projection of that layer, and a diagnostic naming it should not claim to be
/// quoting the layer the editor holds.
pub const USER_LAYER_NAME: &str = "user-palette";

/// What one keypress came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Resolved {
    /// A binding matched, and named a verb this panel owns.
    Verb(Verb),
    /// The strokes so far begin a longer sequence; nothing has run yet.
    Pending,
    /// Nothing claimed the key. The panel decides what an unclaimed key means.
    Unclaimed,
    /// A sequence in progress was abandoned by a key that cannot continue it.
    ///
    /// Distinct from [`Self::Unclaimed`] on purpose: that key was typed as part
    /// of a chord, not as text, and feeding it to the query field would put a
    /// character in a query the user was not editing.
    Abandoned,
}

impl CommandPalette {
    /// Resolves `event` against the panel's keymap.
    ///
    /// Pushes the stroke onto the pending sequence first, so a multi-stroke
    /// binding a user wrote is reachable. Almost always the sequence is one
    /// stroke long and is cleared again before returning.
    ///
    /// A binding naming a command that is not one of this panel's verbs resolves
    /// [`Unclaimed`](Resolved::Unclaimed) rather than being run: the palette is
    /// modal, and a user layer reaching it can only carry palette commands, so
    /// this is the suppression case and the unknown-command case at once.
    pub(super) fn resolve_key(&mut self, event: &KeyEvent) -> Resolved {
        self.pending.push(KeyPress::new(event.key, event.modifiers));

        if let Some(binding) = self.keys.exact_match(&self.pending, Some(&PALETTE_MODE)) {
            // A suppression matches and names no command: the sequence is
            // deliberately bound to nothing, which is not the same as nothing
            // having claimed it. Both end the sequence; only one reaches the
            // query field.
            let verb = binding.command().and_then(Verb::from_id);
            self.pending.clear();
            return verb.map_or(Resolved::Unclaimed, Resolved::Verb);
        }

        if self
            .keys
            .has_continuation(&self.pending, Some(&PALETTE_MODE))
        {
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
    /// Only bindings naming one of this panel's verbs are kept, each scoped to
    /// [`PALETTE_MODE`]; suppressions are kept unscoped. See the module
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
                        layer.push(binding.clone().in_mode(PALETTE_MODE));
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
    /// ⚠️ **Called whenever the palette opens**, for the reason the editor's own
    /// resolver is reset on blur: a leader pressed before the palette closed
    /// would otherwise still be waiting when it reopened, and the next key typed
    /// would be read as its continuation.
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
/// palette is up carries `KeyCode::Char('s')`, and a fall-through that read the
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
