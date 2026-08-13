//! Turning a keypress into a verb the overlay can run.
//!
//! The overlay resolves against [its own stack](super::SearchOverlay):
//! [`super::keymap::default_keymap`] at the base, and the user's `[keys]`
//! bindings pushed on top by the face that read the configuration file.
//!
//! # ⭐ Only this overlay's own commands are taken from the user's layer
//!
//! A user's `[keys]` layer holds all their bindings, and most are editor ones
//! with no mode. Pushed into this stack whole, a mode-free binding would apply
//! in **every** mode — so somebody who bound `end` to `edit.deleteToLineEnd` for
//! the document would find `End` had stopped reaching the end of the query,
//! having never mentioned the search panel.
//!
//! [`SearchOverlay::set_user_keymap`] therefore keeps only the bindings whose
//! command is one of this overlay's verbs, and scopes each one to
//! [`SEARCH_MODE`]. The mode is a property of the command the user named, not a
//! second thing for them to keep in step.
//!
//! ⚠️ **Two of those verbs are mode-free document commands** —
//! `search.nextMatch` and `search.previousMatch`. Scoping them here scopes only
//! *this overlay's copy*: the editor's own stack still holds the user's
//! unscoped binding, so `ctrl+g = "search.nextMatch"` reaches the next match
//! both in the document and in the panel. That is what the line asked for.
//!
//! # ⚠️ An unbind is the one thing that cannot be scoped, and is not dropped
//!
//! `"escape" = ""` suppresses a sequence and carries **no command**, so there is
//! nothing to read a verb off. Dropping it would mean a user could unbind a key
//! everywhere except this overlay; guessing a verb for it would be inventing
//! intent. It is passed through mode-free, so it suppresses that sequence here
//! too — the honest reading of what was written.

use iridium_editor::commands::builtin::SEARCH_MODE;
use iridium_editor::{KeyEvent, KeyPress, Keymap, KeymapStack, Modifiers};

use super::SearchOverlay;
use super::keymap::default_keymap;
use super::verb::Verb;

/// The name the user's bindings are pushed under inside the overlay.
///
/// Distinct from `iridium_config`'s `"user"` because this is a *filtered*
/// projection of that layer, and a diagnostic naming it should not claim to be
/// quoting the layer the editor holds.
pub const USER_LAYER_NAME: &str = "user-terminal-search";

/// What one keypress came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Resolved {
    /// A binding matched, and named a verb this overlay owns.
    Verb(Verb),
    /// The strokes so far begin a longer sequence; nothing has run yet.
    Pending,
    /// A binding matched and named no command: the sequence is bound to nothing.
    ///
    /// ⚠️ **Distinct from [`Self::Unclaimed`], and the distinction is the whole
    /// point of having it.** This overlay has two text fields, so an unclaimed
    /// printable key becomes text. A key the user wrote `"a" = ""` against must
    /// *not* — reporting a suppression as unclaimed would type the very
    /// character they asked the editor to stop reacting to, and in the
    /// replacement field that character is what `Ctrl+Alt+R` writes into the
    /// document.
    Suppressed,
    /// Nothing claimed the key.
    ///
    /// A printable key with no chord becomes text here. Anything else is
    /// **the host's** — unlike the palette and the undo tree, this overlay is
    /// not modal, which is what keeps a binding such as save alive while it is
    /// open.
    Unclaimed,
}

impl SearchOverlay {
    /// Resolves `event` against the overlay's keymap.
    ///
    /// Pushes the stroke onto the pending sequence first, so a multi-stroke
    /// binding a user wrote is reachable. Almost always the sequence is one
    /// stroke long and is cleared again before returning.
    pub(super) fn resolve_key(&mut self, event: &KeyEvent) -> Resolved {
        self.pending.push(KeyPress::new(event.key, event.modifiers));

        if let Some(binding) = self.keys.exact_match(&self.pending, Some(&SEARCH_MODE)) {
            let verb = binding.command().and_then(Verb::from_id);
            self.pending.clear();
            return verb.map_or(Resolved::Suppressed, Resolved::Verb);
        }

        if self
            .keys
            .has_continuation(&self.pending, Some(&SEARCH_MODE))
        {
            return Resolved::Pending;
        }

        self.pending.clear();
        Resolved::Unclaimed
    }

    /// Replaces the user's bindings inside the overlay with those in `user`.
    ///
    /// Only bindings naming one of this overlay's verbs are kept, each scoped to
    /// [`SEARCH_MODE`]; suppressions are kept unscoped. See the module
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
                        layer.push(binding.clone().in_mode(SEARCH_MODE));
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

/// Whether a modifier set is one that types text rather than naming a chord.
///
/// Shift is not consulted: it decides *which* character a printable key
/// produced, and the input adapter has already resolved that. `AltGraph` is not
/// consulted for the same reason — on a great many layouts it is how the
/// character is typed at all.
pub(super) const fn types_text(modifiers: Modifiers) -> bool {
    !modifiers.ctrl && !modifiers.alt && !modifiers.meta
}
