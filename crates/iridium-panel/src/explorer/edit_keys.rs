//! What the explorer does with a key while its rows are being edited.
//!
//! The second dispatch. [`super::keys`] holds the browsing one, and both act on
//! a [`Verb`] the keymap already resolved — so every chord named below is a
//! **default** that a `[keys]` line can move, not a fixed fact.
//!
//! # Three screens, not one
//!
//! An editing session shows one of three things, each its own mode, and each
//! taking a different set of keys:
//!
//! | mode | what is on it | what answers it |
//! | --- | --- | --- |
//! | `explorer.edit` | the buffer, one row carrying a caret | everything below |
//! | `explorer.refused` | why the buffer cannot be applied | `explorer.refused.dismiss` |
//! | `explorer.confirm` | the operations, in the order they will happen | apply and cancel |
//!
//! The last two replace the row list wholesale, so a key that edited a row
//! from behind one of them would change something nobody can see. That is why
//! the refusal screen takes one key and the confirmation two.
//!
//! # The four ruled keys, as defaults
//!
//! | default key | command | what it does |
//! | --- | --- | --- |
//! | `Tab` | `explorer.beginEdit` | enters edit mode — bound in [`super::keys`], because it is pressed while browsing |
//! | `Ctrl+D` | `explorer.edit.strikeRow` | strikes the row under the cursor through, or unstrikes it |
//! | `Ctrl+Enter` | `explorer.edit.newRow` | types a new row below the cursor |
//! | `⌘S` | `explorer.edit.askApply` | asks for the confirmation. **It does not apply**; `y` does |
//!
//! `Ctrl+N` and `Ctrl+P` were already the movement pair before any of this,
//! which is why a new row is `Ctrl+Enter` and not `Ctrl+N`.
//!
//! # What is deliberately *not* bound in these modes
//!
//! - **`Enter`.** In the browse table it returns
//!   [`ExplorerOutcome::Open`](super::panel::ExplorerOutcome::Open), and the
//!   host answers that by dropping the panel — which would take a dirty buffer
//!   with it and no keystroke would have meant it. Unbound here, so it is
//!   swallowed.
//! - **`⌘↑` and `⌘↓`.** Re-rooting replaces the whole panel, mode and all.
//! - **`←` and `→` as tree keys.** Expanding a folder changes the row source
//!   the buffer is a snapshot of. They move the caret in the name instead, and
//!   `Home` and `End` go with them, because a field that took two of the four
//!   motions and gave the others to the list would be a field with no rule.
//!
//! ⚠️ Unbound in a mode is not the same as absent: a user may bind any of them,
//! and the dispatch below answers whatever arrives. What the defaults decline to
//! do is offer them.

use std::collections::BTreeSet;
use std::path::Path;

use iridium_editor::commands::builtin::EXPLORER_TOGGLE_PANEL;
use iridium_editor::{KeyCode, KeyEvent};

use super::apply;
use super::mode::{Confirmation, Leaving};
use super::panel::{ExplorerOutcome, FileExplorer};
use super::plan::{Operation, Plan};
use super::resolve::{Resolved, types_text};
use super::verb::Verb;
use crate::Entry;

/// The verb a resolution names, if it named one this panel owns.
fn verb_of(resolved: &Resolved) -> Option<Verb> {
    match resolved {
        Resolved::Command(id) => Verb::from_id(id),
        _ => None,
    }
}

impl FileExplorer {
    /// Acts on a key resolved while an editing session is open.
    ///
    /// Split by **what is actually on the screen**, because that is what
    /// decides which keys can honestly do anything — and each of the three
    /// questions is asked of the state that can only be true on one of them: a
    /// plan exists only while confirming, a refusal only while one is being
    /// shown, a cursor only while the rows are.
    ///
    /// A session that answers none of the three is browsing, and the caller
    /// only comes here when it is not. The key is swallowed rather than passed
    /// back to the browse dispatch anyway, so a future caller getting the guard
    /// wrong cannot turn a filename into a filter.
    pub(super) fn dispatch_edit(
        &mut self,
        resolved: &Resolved,
        event: &KeyEvent,
    ) -> ExplorerOutcome {
        if self.mode.plan().is_some() {
            return self.confirming(resolved, event);
        }
        if !self.mode.refusals().is_empty() {
            return self.refused(resolved);
        }
        if self.mode.cursor().is_some() {
            return self.editing(resolved, event);
        }
        ExplorerOutcome::Handled
    }

    /// A key while the refusals are on screen.
    ///
    /// One binding, and everything else swallowed. The rows are not showing,
    /// so there is no row for a character to land in and no cursor for an
    /// arrow to move; a key that edited the buffer from here would change
    /// something the user cannot see while they are reading why the last
    /// change was turned down.
    ///
    /// What that one binding does is go back to the rows rather than out of
    /// the session — the edits are what needs fixing, so throwing them away is
    /// the opposite of the help being offered.
    fn refused(&mut self, resolved: &Resolved) -> ExplorerOutcome {
        if verb_of(resolved) == Some(Verb::RefusedDismiss) {
            self.mode.note_edit();
        }
        ExplorerOutcome::Handled
    }

    /// A key while the rows are on screen.
    fn editing(&mut self, resolved: &Resolved, event: &KeyEvent) -> ExplorerOutcome {
        let verb = verb_of(resolved);
        // Every key but the one that leaves answers the discard question with
        // "no, I am still working". Cleared here, once, rather than in each
        // arm — an arm that forgot would leave a later press able to throw the
        // buffer away without asking again.
        //
        // ⭐ Keyed on the *verb* rather than on the chord, which is what makes
        // it survive rebinding: a user who moves "stop editing" to another key
        // moves the arming with it, and `esc` becomes an ordinary key that
        // disarms like every other.
        if verb != Some(Verb::EditLeave) {
            self.mode.disarm_discard();
        }

        let Some(verb) = verb else {
            return match resolved {
                // ⚠️ The toggle chord closes the panel from inside an editing
                // session, and is answered here rather than by the browse
                // dispatch — it has to be, because leaving with a dirty buffer
                // is refused rather than allowed. It is not a [`Verb`]: it is
                // the editor's command, which this panel names only because it
                // is modal and would otherwise swallow it.
                Resolved::Command(id) if id == &EXPLORER_TOGGLE_PANEL => self.close_from_edit(),
                // ⭐ The case the whole mode exists for: this character reaches
                // the row being edited, not the query.
                Resolved::Unclaimed => self.unclaimed_edit_key(event),
                _ => ExplorerOutcome::Handled,
            };
        };

        match verb {
            Verb::EditLeave => self.leave_edit(),
            Verb::EditStrikeRow => self.strike_row(event.is_repeat),
            Verb::EditNewRow => self.type_row(event.is_repeat),
            Verb::EditAskApply => self.ask_to_apply(event.is_repeat),
            Verb::EditCursorUp => {
                self.move_cursor_up();
                ExplorerOutcome::Handled
            },
            Verb::EditCursorDown => {
                self.move_cursor_down();
                ExplorerOutcome::Handled
            },
            Verb::EditCaretLeft => {
                self.mode.move_caret(Entry::move_left);
                ExplorerOutcome::Handled
            },
            Verb::EditCaretRight => {
                self.mode.move_caret(Entry::move_right);
                ExplorerOutcome::Handled
            },
            Verb::EditCaretHome => {
                self.mode.move_caret(Entry::move_home);
                ExplorerOutcome::Handled
            },
            Verb::EditCaretEnd => {
                self.mode.move_caret(Entry::move_end);
                ExplorerOutcome::Handled
            },
            Verb::EditBackspace => self.backspace_in_row(),
            Verb::EditDelete => {
                self.mode.edit_name(Entry::delete);
                ExplorerOutcome::Handled
            },
            // Modal, and more so than in browse: `Enter` and the re-rooting
            // pair are *deliberately* unbound in this mode rather than bound to
            // something harmless, because what they do while browsing would
            // drop the buffer.
            _ => ExplorerOutcome::Handled,
        }
    }

    /// A key nothing claimed while the rows are being edited: it types.
    fn unclaimed_edit_key(&mut self, event: &KeyEvent) -> ExplorerOutcome {
        if let KeyCode::Char(character) = event.key
            && types_text(event.modifiers)
        {
            self.mode.edit_name(|entry| entry.insert(character));
        }
        ExplorerOutcome::Handled
    }

    /// A key while the confirmation is on screen.
    ///
    /// Two bindings, and they are what [`super::confirm::plan_rows`] prints at
    /// the bottom of it. Applying is **inert under key repeat**: a held key
    /// repeating into an apply is the one way a confirmation can be answered by
    /// an accident of timing rather than by a decision.
    fn confirming(&mut self, resolved: &Resolved, event: &KeyEvent) -> ExplorerOutcome {
        match verb_of(resolved) {
            Some(Verb::ConfirmApply) if !event.is_repeat => self.apply_confirmed(),
            Some(Verb::ConfirmCancel) => {
                self.mode.back_to_edit();
                ExplorerOutcome::Handled
            },
            _ => ExplorerOutcome::Handled,
        }
    }

    /// `esc`: stop editing, or ask about the work first.
    ///
    /// ⚠️ **This is the answer to [`Leaving::Unsaved`]**, and the panel owns it
    /// because nothing else can. The first press reports the refusal and arms
    /// the second; the second throws the edits away. Two presses of one key,
    /// with a sentence between them, rather than a prompt — the existing
    /// desktop face's `Prompt` is a single line with a `Deed` of two
    /// variants, both about the document, and bending it around a panel that
    /// deletes files would make one confirmation mechanism answer for two very
    /// different questions. It is also a *face's* strip, which this panel now
    /// deliberately cannot reach.
    ///
    /// A clean buffer needs none of it: `leave` reports `Left` and the panel is
    /// browsing again, which is what `esc` means everywhere else here.
    fn leave_edit(&mut self) -> ExplorerOutcome {
        match self.mode.leave() {
            Leaving::Left => ExplorerOutcome::Handled,
            Leaving::Unsaved => {
                if self.mode.arm_discard() {
                    self.mode.discard();
                    return ExplorerOutcome::Handled;
                }
                ExplorerOutcome::Failed(
                    "there are unapplied edits — esc again to throw them away".to_owned(),
                )
            },
        }
    }

    /// The toggle chord, which closes the panel — once the buffer allows it.
    ///
    /// Refused rather than routed through the two-press discard: closing the
    /// panel and discarding a buffer are two decisions, and a chord that did
    /// both on its second press would do the second one to a user who only
    /// meant the first.
    fn close_from_edit(&mut self) -> ExplorerOutcome {
        match self.mode.leave() {
            Leaving::Left => ExplorerOutcome::Closed,
            Leaving::Unsaved => ExplorerOutcome::Failed(
                "there are unapplied edits — esc twice to throw them away first".to_owned(),
            ),
        }
    }

    /// `Ctrl+D`: strike the row under the cursor through, or unstrike it.
    ///
    /// A toggle rather than a removal, which is what makes it safe to press:
    /// nothing is destroyed until the whole buffer is applied, so a strike
    /// pressed by mistake is undone by pressing it again. On a row the user
    /// typed it reads the same way — struck means "this will not exist
    /// afterwards", and for a row that is not on disk that means it will not be
    /// created.
    ///
    /// **The root is refused here as well as in [`super::plan`].** The plan
    /// owns the rule and states it in the confirmation, but a strike that is
    /// only turned down at `⌘S` leaves the folder the panel is showing drawn as
    /// doomed until then, which is a sentence the screen tells for as long as
    /// the user keeps editing.
    ///
    /// Key repeat is ignored. Held down, a toggle settles on the parity of
    /// however many repeats the keyboard sent, which is not a state anybody
    /// chose.
    fn strike_row(&mut self, is_repeat: bool) -> ExplorerOutcome {
        if is_repeat {
            return ExplorerOutcome::Handled;
        }
        if self.mode.cursor() == Some(0) {
            return ExplorerOutcome::Failed(
                "the folder the panel is showing cannot be deleted from inside it".to_owned(),
            );
        }
        self.mode.toggle_deleted();
        ExplorerOutcome::Handled
    }

    /// `Ctrl+Enter`: a new, empty row below the cursor, with the caret on it.
    ///
    /// Where it lands — inside the row above or beside it — is
    /// [`super::mode::Mode::insert_row`]'s decision, made from what the panel
    /// was drawing when the session began.
    ///
    /// Key repeat is ignored for the same reason as the strike, and a sharper
    /// one: held down, this fills the buffer with nameless rows, and a single
    /// nameless row refuses the entire plan.
    fn type_row(&mut self, is_repeat: bool) -> ExplorerOutcome {
        if is_repeat {
            return ExplorerOutcome::Handled;
        }
        self.mode.insert_row();
        ExplorerOutcome::Handled
    }

    /// `Backspace`: a character out of the name, or the typed row itself.
    ///
    /// A row the user typed and has now emptied is a row they are taking back,
    /// and the next backspace takes it. Only a *typed* row: one that came from
    /// the tree leaves the buffer by being struck through, so that the rows
    /// keep saying what is on the disk.
    fn backspace_in_row(&mut self) -> ExplorerOutcome {
        if self.mode.edit_name(Entry::backspace) {
            return ExplorerOutcome::Handled;
        }
        // Nothing was removed, so the caret is at the start of the name. Only
        // an *empty* name means the row is finished with — a caret parked at
        // the start of a name the user is still holding is not.
        let empty = self.mode.name().is_some_and(|name| name.text().is_empty());
        if empty && self.mode.cursor_is_typed() {
            self.mode.remove_typed_row();
        }
        ExplorerOutcome::Handled
    }

    /// `⌘S`: work out what the buffer says, and show it.
    ///
    /// ⛔ **This does not touch the disk.** It reaches the confirmation and
    /// stops there; `y` on that screen is what applies. Two keys, because the
    /// gap between them is the whole of ruling 7.
    ///
    /// The four answers are all worth a different screen. `Ready` is now
    /// showing the plan, and `Refused` the reasons, so both are drawn rather
    /// than reported. `NothingToDo` has neither to show and would otherwise be
    /// a key that visibly did nothing.
    fn ask_to_apply(&mut self, is_repeat: bool) -> ExplorerOutcome {
        if is_repeat {
            return ExplorerOutcome::Handled;
        }
        match self.mode.request_confirm() {
            // `Ready` and `Refused` are both now on screen — the plan and the
            // reasons respectively — so neither needs a message. `NotEditing`
            // cannot be reached from here at all, and is swallowed with them
            // rather than reported, because a message about a state the user
            // is not in is a message they can do nothing with.
            Confirmation::Ready | Confirmation::Refused | Confirmation::NotEditing => {
                ExplorerOutcome::Handled
            },
            Confirmation::NothingToDo => {
                ExplorerOutcome::Failed("nothing to apply — the rows are as they were".to_owned())
            },
        }
    }

    /// `y`: carry the confirmed plan out.
    ///
    /// ⚠️ **Runs on the frame thread.** Every other disk read in this panel is
    /// posted to a worker, and this one is not: an apply must not interleave
    /// with the polling that would redraw the rows underneath it, and a
    /// half-applied plan being drawn as though it had finished is worse than a
    /// window that pauses. A large `remove_dir_all` therefore holds the frame
    /// for as long as it takes. That is a known cost, written down rather than
    /// discovered.
    ///
    /// **The panel returns to browsing whichever way it went, and both are
    /// deliberate.** On success there is nothing left to confirm. On failure
    /// the plan describes a filesystem that has partly moved — some of its
    /// operations happened and the rest did not — and a confirmation still
    /// offering to run it would be offering to repeat what already did.
    /// [`super::apply`] does not roll back, and says why; what the user gets
    /// instead is the message and a re-read of the folders that were touched.
    fn apply_confirmed(&mut self) -> ExplorerOutcome {
        let Some(plan) = self.mode.plan().cloned() else {
            return ExplorerOutcome::Handled;
        };
        let outcome = apply::apply(&plan);
        // Recorded as an apply rather than a discard even when it failed:
        // work happened, and the two are not interchangeable.
        self.mode.applied();
        self.reload_touched(&plan);

        match outcome {
            Ok(_) => ExplorerOutcome::Handled,
            Err(failure) => ExplorerOutcome::Failed(failure.describe()),
        }
    }

    /// Re-reads every directory the plan touched.
    ///
    /// ⚠️ **Not optional, and not cosmetic.** A row's origin is captured from
    /// the node it was drawn from, so a second editing session over stale rows
    /// would carry paths that have moved or gone — and a create-then-delete
    /// that reused a name would produce a delete naming a *different* file.
    /// [`iridium_explorer::FileTree::reload`]'s own documentation names this
    /// feature as the reason it keeps node ids across a refresh.
    ///
    /// The directories are those an operation happened *in*, both ends of a
    /// rename included: that is the exact set whose membership changed. The
    /// listings arrive on the worker like any other, so the rows fill back in
    /// over the following frames rather than here.
    fn reload_touched(&mut self, plan: &Plan) {
        let mut touched = BTreeSet::new();
        let mut note = |path: &Path| {
            if let Some(directory) = path.parent() {
                touched.insert(directory.to_path_buf());
            }
        };
        for operation in &plan.operations {
            match operation {
                Operation::Create { path, .. } | Operation::Delete { path } => note(path),
                Operation::Rename { from, to } => {
                    note(from);
                    note(to);
                },
            }
        }

        // A walk of what has been read, rather than a path lookup: the arena's
        // index by path is its own, and asking for a node by path would be a
        // second way to identify a row — the one thing this feature does not
        // do.
        let mut pending = vec![self.files.root()];
        let mut stale = Vec::new();
        while let Some(node) = pending.pop() {
            pending.extend_from_slice(self.files.listed_children(node));
            if self
                .files
                .info(node)
                .is_some_and(|info| touched.contains(info.path))
            {
                stale.push(node);
            }
        }
        for node in stale {
            self.files.reload(node);
        }
    }

    /// Moves the cursor one row towards the top of the buffer.
    fn move_cursor_up(&mut self) {
        if let Some(cursor) = self.mode.cursor() {
            self.mode.seat(cursor.saturating_sub(1));
        }
    }

    /// Moves the cursor one row towards the bottom. `seat` clamps, so the last
    /// row is where this stops.
    fn move_cursor_down(&mut self) {
        if let Some(cursor) = self.mode.cursor() {
            self.mode.seat(cursor.saturating_add(1));
        }
    }
}
