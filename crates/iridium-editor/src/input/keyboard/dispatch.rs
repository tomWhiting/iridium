//! Keypress → command → handler, and the bookkeeping that follows it.
//!
//! Dispatch used to be a `match` on `(KeyCode, Modifiers)`, which made every
//! binding a branch in the kernel: unswappable, unlistable, and invisible to a
//! command palette. It is now three steps, none of which knows what key was
//! pressed except the first:
//!
//! 1. [`KeymapResolver`] turns the keypress into a
//!    [`Resolution`] against the handler's [`KeymapStack`](crate::KeymapStack) —
//!    a command id, a pending multi-key sequence, an abort, or no match;
//! 2. [`action_for`] turns the id into a [`KeyboardAction`], which
//!    [`KeyboardHandler::run_action`] executes;
//! 3. [`KeyboardHandler::note_operation`] invalidates the transient handler state
//!    (sticky columns, the multi-cursor addition-order stack) that the command
//!    just ran did not itself maintain.
//!
//! # Fall-through is what keeps typing working
//!
//! [`Resolution::NoMatch`] means the keypress is the caller's. Self-insert is the
//! one behaviour that cannot be a binding — no key sequence stands for "whatever
//! character the user typed" — so it lives here, gated exactly as the old
//! `Char(_)` arms were: a [`KeyCode::Char`] with none of `ctrl`, `alt` or `meta`
//! held inserts, and everything else is [`KeyResult::Ignored`]. The character
//! comes from the **original** event, never from the normalized [`KeyPress`] the
//! resolver matched on, because normalization lowercases `A` into `a` for lookup
//! purposes only.
//!
//! # Why a pending chord returns `Handled`
//!
//! [`Resolution::Pending`], [`Resolution::Aborted`] and
//! [`Resolution::ModeEntered`] consumed the keypress. It must not reach the
//! document: the first stroke of `Ctrl+K Ctrl+D` is not text, and neither is the
//! stroke that abandons a half-typed sequence or the one that enters a mode.
//!
//! A keystroke that *dead-ends* a sequence is not lost: the resolver retries it
//! from scratch (see [`KeymapResolver::resolve`](crate::KeymapResolver::resolve)),
//! so a mistyped chord costs the leader and nothing else.
//!
//! # A resolved command the kernel does not implement
//!
//! [`action_for`] returns `None` for an id no `KeyboardAction` covers, which is
//! exactly what a host command looks like. The id is then reported as
//! [`KeyResult::HostCommand`] together with its arguments, never discarded: a
//! command that the registry lists and a keymap binds must reach *somebody*, and
//! for a multi-stroke sequence the host has no other way to learn which sequence
//! completed.

use crate::commands::{CommandArgs, KeyPress, Resolution};
use crate::document::{CursorState, Document};
use crate::editor::EditorConfig;
use crate::history::UndoTree;

use super::KeyboardHandler;
use super::actions::{CommandContext, KeyboardAction, action_for};
use super::types::{ClipboardOperation, KeyCode, KeyEvent, KeyResult};

impl KeyboardHandler {
    /// Resolves `event` through the keymap and runs whatever it names.
    ///
    /// Returns the result together with the action that produced it, which
    /// [`Self::note_operation`] needs: a pending or aborted chord ran nothing, so
    /// it reports `None` and invalidates nothing.
    ///
    /// Split out from [`Self::handle_key`] so the post-dispatch bookkeeping runs
    /// uniformly for every key, on every path.
    pub(super) fn dispatch_key(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
        history: &UndoTree,
        config: &EditorConfig,
    ) -> (KeyResult, Option<KeyboardAction>) {
        // `resolver` and `keymap` are disjoint fields, so the mutable borrow of
        // the state machine coexists with the immutable borrow of the bindings.
        let resolution = self.resolver.resolve_repeat(
            &self.keymap,
            KeyPress::from_event(event),
            event.is_repeat,
        );

        match resolution {
            Resolution::Matched(invocation) => {
                let (id, args) = invocation.into_parts();
                let ctx = CommandContext {
                    event: Some(event),
                    args,
                    document,
                    cursor,
                    history,
                    config,
                };
                match action_for(id.as_str()) {
                    Some(action) => (self.run_action(action, &ctx), Some(action)),
                    // A keymap may bind a command this kernel does not implement
                    // — a host command — and the host then owns it, by name.
                    None => (
                        KeyResult::HostCommand {
                            command: id,
                            args: ctx.args,
                        },
                        None,
                    ),
                }
            },
            // The keypress was consumed by the sequence state machine. Nothing
            // ran, so nothing is invalidated.
            Resolution::Pending | Resolution::Aborted | Resolution::ModeEntered(_) => {
                (KeyResult::Handled, None)
            },
            Resolution::NoMatch => {
                let ctx = CommandContext {
                    event: Some(event),
                    args: CommandArgs::NONE,
                    document,
                    cursor,
                    history,
                    config,
                };
                self.fall_through(&ctx)
            },
        }
    }

    /// Handles a keypress no binding claimed.
    ///
    /// The only fall-through behaviour is self-insert, and it applies under
    /// exactly the condition the pre-registry `Char(_)` arms used: a character
    /// key with none of `ctrl`, `alt` or `meta` held. `Shift` is deliberately not
    /// consulted — the host already resolved it into the character. `AltGraph` is
    /// likewise not consulted here: layouts that report `AltGr` as `Ctrl+Alt`
    /// fail the `ctrl`/`alt` test and pass through untouched, which is what lets
    /// the host compose the character itself.
    fn fall_through(&mut self, ctx: &CommandContext<'_>) -> (KeyResult, Option<KeyboardAction>) {
        let Some(event) = ctx.event else {
            return (KeyResult::Ignored, None);
        };
        let modifiers = event.modifiers;
        if matches!(event.key, KeyCode::Char(_))
            && !modifiers.ctrl
            && !modifiers.alt
            && !modifiers.meta
        {
            // Routed through the same action as an explicit binding would be, so
            // self-insert has exactly one implementation.
            let result = self.run_action(KeyboardAction::InsertCharacter, ctx);
            return (result, Some(KeyboardAction::InsertCharacter));
        }
        (KeyResult::Ignored, None)
    }

    /// Post-dispatch bookkeeping: invalidates transient handler state after a
    /// cursor-mutating operation that is not the corresponding verb.
    ///
    /// - A cursor mutation that is not a sticky-column vertical op
    ///   ([`Self::uses_sticky_columns`]) invalidates the sticky columns, but
    ///   *how* depends on whether the operation is undoable:
    ///   - A **content edit** only marks the columns dirty (dormant). The edit
    ///     may be undone, and an undo that restores the exact pre-edit cursor
    ///     state should also revive the columns captured before it (see
    ///     [`Self::revalidate_vertical_columns`]).
    ///   - A **selection-only** move (a horizontal motion, Home/End, collapse to
    ///     primary) permanently *discards* the columns. Such moves are never
    ///     recorded in the undo history, so no later undo can legitimately revive
    ///     them; a Left-then-Right round trip that returns the caret to the
    ///     captured coordinates must not leave a dirty snapshot for a subsequent
    ///     undo of an *unrelated* content edit to resurrect. Destroying (not
    ///     merely dirtying) is what closes that resurrection window.
    /// - Any cursor mutation that is not a multi-cursor add verb
    ///   ([`Self::is_add_order_verb`]) clears the addition-order stack, so a
    ///   motion or edit between add verbs cannot let remove-last-cursor or skip
    ///   remove the wrong cursor even if the state round-trips.
    ///
    /// Operations that leave the cursor untouched (copy, undo/redo
    /// acknowledgements, search requests, pending chords, ignored keys)
    /// invalidate nothing.
    ///
    /// # Why this is keyed on the command, not on the keypress
    ///
    /// The predicates below used to sniff modifiers off the [`KeyEvent`], which
    /// only worked while the bindings were hard-coded in dispatch. Keyed on the
    /// resolved command they keep working when a user keymap moves
    /// add-cursor-above off `Ctrl+Alt+Up`. The mapping is one-to-one with the old
    /// predicates for the default keymap, and `None` (a pending chord, an
    /// unimplemented command, an ignored key) satisfies neither — matching the
    /// old behaviour, where such keys produced no cursor mutation and so returned
    /// early anyway.
    pub(super) fn note_operation(&mut self, action: Option<KeyboardAction>, result: &KeyResult) {
        if !Self::result_mutates_cursor(result) {
            return;
        }
        if !Self::uses_sticky_columns(action) {
            if Self::result_modifies_content(result) {
                // Undoable edit: keep the columns dormant so an undo that
                // restores the exact prior state can revive them.
                self.sticky_dirty = true;
            } else {
                // Non-undoable selection-only move: destroy the columns so no
                // later undo (of an unrelated edit) can resurrect a snapshot a
                // horizontal round trip already invalidated.
                self.reset_vertical_state();
            }
        }
        if !Self::is_add_order_verb(action) {
            self.invalidate_cursor_order();
        }
    }

    /// Returns true when the result of a keyboard operation modifies document
    /// content (as opposed to a selection-only change or a no-op).
    fn result_modifies_content(result: &KeyResult) -> bool {
        match result {
            KeyResult::Command(command) => command.modifies_content(),
            KeyResult::Clipboard(ClipboardOperation::Cut { command, .. }) => {
                command.modifies_content()
            },
            KeyResult::Handled
            | KeyResult::Ignored
            | KeyResult::HostCommand { .. }
            | KeyResult::Clipboard(_)
            | KeyResult::Search(_) => false,
        }
    }

    /// Returns true when the result of a keyboard operation changes the cursor
    /// or selection state (a selection command, a content edit, or a cut whose
    /// deletion mutates the document).
    fn result_mutates_cursor(result: &KeyResult) -> bool {
        match result {
            KeyResult::Command(command) => {
                command.modifies_selection() || command.modifies_content()
            },
            KeyResult::Clipboard(ClipboardOperation::Cut { command, .. }) => {
                command.modifies_content()
            },
            KeyResult::Handled
            | KeyResult::Ignored
            | KeyResult::HostCommand { .. }
            | KeyResult::Clipboard(_)
            | KeyResult::Search(_) => false,
        }
    }

    /// Returns true when the command consumes and re-establishes the sticky
    /// preferred columns: vertical caret movement (plain or selection-extending)
    /// and vertical add-cursor.
    ///
    /// The line operations are excluded — they move whole lines and must not
    /// preserve sticky columns.
    const fn uses_sticky_columns(action: Option<KeyboardAction>) -> bool {
        matches!(
            action,
            Some(
                KeyboardAction::LineUp
                    | KeyboardAction::LineUpSelect
                    | KeyboardAction::LineDown
                    | KeyboardAction::LineDownSelect
                    | KeyboardAction::AddCursorAbove
                    | KeyboardAction::AddCursorBelow
            )
        )
    }

    /// Returns true when the command is a multi-cursor add-order verb, i.e. one
    /// that maintains the addition-order stack itself and must not have it
    /// cleared underneath it by [`Self::note_operation`].
    ///
    /// [`KeyboardAction::SkipLastOccurrence`] is in this set because it rewrites
    /// the stack in place (dropping one occurrence entry and pushing its
    /// replacement); clearing it afterwards would make a second skip fall back to
    /// the "no added cursors" branch and move the primary instead of advancing
    /// the cursor it just added.
    const fn is_add_order_verb(action: Option<KeyboardAction>) -> bool {
        matches!(
            action,
            Some(
                KeyboardAction::AddSelectionToNextMatch
                    | KeyboardAction::SelectAllOccurrences
                    | KeyboardAction::RemoveLastCursor
                    | KeyboardAction::AddCursorAbove
                    | KeyboardAction::AddCursorBelow
                    | KeyboardAction::SkipLastOccurrence
            )
        )
    }
}
