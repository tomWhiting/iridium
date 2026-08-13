//! Turning a keypress into a verb the menu can run.
//!
//! The menu resolves against [its own stack](super::ContextMenu):
//! [`super::keymap::default_keymap`] at the base, and the user's `[keys]`
//! bindings pushed on top by the face that read the configuration file.
//!
//! # ⭐ Only this menu's own commands are taken from the user's layer
//!
//! A user's `[keys]` layer holds all their bindings, and most are editor ones
//! with no mode. Pushed into this stack whole, a mode-free binding would apply
//! in **every** mode — so somebody who bound `end` to `edit.deleteToLineEnd` for
//! the document would find `End` had stopped reaching the last row, having never
//! mentioned the context menu.
//!
//! [`stack_for`] therefore keeps only the bindings whose command is one of this
//! menu's verbs, and scopes each one to [`CONTEXT_MENU_MODE`]. The mode is a
//! property of the command the user named, not a second thing for them to keep
//! in step.
//!
//! # ⚠️ An unbind is the one thing that cannot be scoped, and is not dropped
//!
//! `"escape" = ""` suppresses a sequence and carries **no command**, so there is
//! nothing to read a verb off. Dropping it would mean a user could unbind a key
//! everywhere except this menu; guessing a verb for it would be inventing
//! intent. It is passed through mode-free, so it suppresses that sequence here
//! too — the honest reading of what was written. The menu stays closable
//! regardless, because a click outside it closes it and that is not a key.
//!
//! # ⚠️ The stack is built once, in [`ContextMenu::open`](super::ContextMenu::open)
//!
//! Every other panel is long-lived and gains a `set_user_keymap` a configuration
//! reload calls. This one exists only between a right-click and the next key, so
//! there is no live menu for a reload to reach: the next right-click builds a
//! fresh stack from the face's current layer. `config.reload` cannot be run from
//! inside the menu either — the menu is modal, and its own rows are Cut, Copy,
//! Paste, Select All and the palette.

use iridium_editor::commands::builtin::CONTEXT_MENU_MODE;
use iridium_editor::{KeyEvent, KeyPress, Keymap, KeymapStack};

use super::ContextMenu;
use super::keymap::default_keymap;
use super::verb::Verb;

/// The name the user's bindings are pushed under inside the menu.
///
/// Distinct from `iridium_config`'s `"user"` because this is a *filtered*
/// projection of that layer, and a diagnostic naming it should not claim to be
/// quoting the layer the editor holds.
pub const USER_LAYER_NAME: &str = "user-context-menu";

/// What one keypress came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Resolved {
    /// A binding matched, and named a verb this menu owns.
    Verb(Verb),
    /// The strokes so far begin a longer sequence; nothing has run yet.
    Pending,
    /// Nothing claimed the key.
    ///
    /// ⚠️ Unlike the palette and the file explorer, this menu has no field, so
    /// there is nothing for an unclaimed key to fall through *to*. It is
    /// swallowed, which is what being modal means — and that is why this enum
    /// has no separate `Suppressed`: the two would be acted on identically, and
    /// a distinction nothing reads is a distinction that will drift.
    Unclaimed,
}

/// Builds the menu's keymap stack: its own defaults, then the user's bindings.
///
/// Only bindings naming one of this menu's verbs are kept, each scoped to
/// [`CONTEXT_MENU_MODE`]; suppressions are kept unscoped. See the module
/// documentation for why both of those are the way they are.
pub(super) fn stack_for(user: &Keymap) -> KeymapStack {
    let mut layer = Keymap::new(USER_LAYER_NAME);
    for binding in user.bindings() {
        match binding.command() {
            Some(id) => {
                if Verb::from_id(id).is_some() {
                    layer.push(binding.clone().in_mode(CONTEXT_MENU_MODE));
                }
            },
            None => layer.push(binding.clone()),
        }
    }

    let mut stack = KeymapStack::with_base(default_keymap());
    if !layer.is_empty() {
        stack.push(layer);
    }
    stack
}

impl ContextMenu {
    /// Resolves `event` against the menu's keymap.
    ///
    /// Pushes the stroke onto the pending sequence first, so a multi-stroke
    /// binding a user wrote is reachable. Almost always the sequence is one
    /// stroke long and is cleared again before returning.
    pub(super) fn resolve_key(&mut self, event: &KeyEvent) -> Resolved {
        self.pending.push(KeyPress::new(event.key, event.modifiers));

        if let Some(binding) = self
            .keys
            .exact_match(&self.pending, Some(&CONTEXT_MENU_MODE))
        {
            // A suppression matches and names no command: the sequence is
            // deliberately bound to nothing, which the menu treats exactly as it
            // treats a key nobody claimed — both do nothing, and here that is
            // the same nothing.
            let verb = binding.command().and_then(Verb::from_id);
            self.pending.clear();
            return verb.map_or(Resolved::Unclaimed, Resolved::Verb);
        }

        if self
            .keys
            .has_continuation(&self.pending, Some(&CONTEXT_MENU_MODE))
        {
            return Resolved::Pending;
        }

        self.pending.clear();
        Resolved::Unclaimed
    }
}
