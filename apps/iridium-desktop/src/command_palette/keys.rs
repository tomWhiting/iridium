//! What a key press does while the palette is open.
//!
//! One entry point — [`CommandPalette::handle_key`] — and it does three things
//! in order: resolve the press against the panel's keymap, run whatever verb
//! that named, and give an unclaimed printable character to the query field.
//!
//! # ⛔ The fall-through checks modifiers before it treats a `Char` as text
//!
//! `Ctrl+S` while the palette is open carries `KeyCode::Char('s')`. A
//! fall-through that read the character alone would put an `s` in the query —
//! the panel inventing input from a chord it was asked to swallow. #91 shipped
//! exactly that bug in its first cut, which is why the guard is a named
//! function ([`types_text`]) with its reasoning attached rather than an
//! inlined condition.
//!
//! # Why an abandoned sequence is not text either
//!
//! A user can bind a multi-stroke sequence into this panel. When the second
//! stroke cannot continue it, that stroke was typed *as part of a chord* — so
//! it is swallowed rather than typed into the query, which is the one thing
//! that separates [`Resolved::Abandoned`] from [`Resolved::Unclaimed`].

use iridium_editor::commands::palette::CommandMru;
use iridium_editor::{Editor, KeyCode, KeyEvent};

use super::panel::{CommandPalette, PaletteOutcome, isize_of};
use super::resolve::{Resolved, types_text};
use super::verb::Verb;
use crate::prompt::Entry;

impl CommandPalette {
    /// Handles one key press. Every key is consumed; see the module docs.
    pub fn handle_key(
        &mut self,
        event: &KeyEvent,
        editor: &Editor,
        mru: &CommandMru,
    ) -> PaletteOutcome {
        match self.resolve_key(event) {
            Resolved::Verb(verb) => self.run(verb, editor, mru),
            // Modal: a stroke part-way through a sequence, one that abandoned
            // it, and one a binding deliberately bound to nothing are all
            // swallowed rather than passed on. ⛔ `Suppressed` belongs here and
            // not with `Unclaimed`: it was claimed by a binding, so sending it
            // to the query would type a character the user unbound.
            Resolved::Pending | Resolved::Abandoned | Resolved::Suppressed => {
                PaletteOutcome::Handled
            },
            Resolved::Unclaimed => self.unclaimed(event),
        }
    }

    /// Carries out one verb.
    fn run(&mut self, verb: Verb, editor: &Editor, mru: &CommandMru) -> PaletteOutcome {
        // Read here rather than in the caller so the page is whatever the last
        // composition actually showed at the moment the key was pressed.
        let page = isize_of(self.page());
        match verb {
            Verb::Dismiss => PaletteOutcome::Closed,
            Verb::Accept => self.accept(editor, mru),
            Verb::SelectPrevious => self.move_selection(editor, mru, -1),
            Verb::SelectNext => self.move_selection(editor, mru, 1),
            Verb::SelectPageUp => self.move_selection(editor, mru, -page),
            Verb::SelectPageDown => self.move_selection(editor, mru, page),
            Verb::CaretLeft => self.edit(Entry::move_left, false),
            Verb::CaretRight => self.edit(Entry::move_right, false),
            Verb::CaretHome => self.edit(Entry::move_home, false),
            Verb::CaretEnd => self.edit(Entry::move_end, false),
            Verb::QueryBackspace => self.edit(Entry::backspace, true),
            Verb::QueryDelete => self.edit(Entry::delete, true),
        }
    }

    /// What a key nothing bound means: a character, or nothing at all.
    ///
    /// ⚠️ **Not a binding, and deliberately not one.** Twenty-six letters as
    /// twenty-six bindings would make a keymap that could not express a
    /// *field*, and a user who does bind a bare letter to a command takes that
    /// letter out of the query — which is the honest consequence of what they
    /// asked for, and is why this arm runs only after resolution has declined.
    fn unclaimed(&mut self, event: &KeyEvent) -> PaletteOutcome {
        match event.key {
            KeyCode::Char(character) if types_text(event.modifiers) => {
                self.edit(|field| field.insert(character), true)
            },
            // Modal: everything else is swallowed, not passed to the document.
            _ => PaletteOutcome::Handled,
        }
    }
}
