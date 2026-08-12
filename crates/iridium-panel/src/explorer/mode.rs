//! What the panel is doing: browsing, editing, or confirming.
//!
//! Step 4b of `docs/IN-FLIGHT-oil.md`. The build order there puts the
//! dangerous logic first and fully tested, before any of it is reachable from
//! a keyboard — `plan` and `apply` were built that way and this followed them.
//! **Nothing in this module names a key**: [`super::edit_keys`] is where the
//! four ruled bindings turn into calls, and it can be rewritten without any of
//! the answers here changing.
//!
//! # Why the buffer lives inside the mode
//!
//! It would be simpler to hang an `Option<Buffer>` off the panel beside a
//! `mode` field. That shape has a failure this one cannot have: two pieces of
//! state that must agree, and nothing making them. A panel in `Browse` holding
//! a buffer, or in `Edit` holding `None`, are both representable, and the
//! second is a crash or a silently empty edit session.
//!
//! Here the buffer is *inside* the variants that have one. Returning to
//! [`Mode::Browse`] drops it by construction rather than by a line somebody has
//! to remember to write, and there is no way to be editing without one.
//!
//! # The one rule worth stating out loud
//!
//! ⚠️ **A dirty buffer is never dropped without being asked about.** Every exit
//! from an editing mode goes through [`Mode::leave`], which refuses and reports
//! [`Leaving::Unsaved`] while there are unapplied edits. The caller then either
//! carries the refusal to the user or calls [`Mode::discard`], which is the
//! only thing in this module that throws work away and is named so it cannot be
//! reached by accident.
//!
//! That rule is what makes step 5's apply-or-discard prompt on a filter change
//! a *use* of this module rather than a second implementation of it: a filter
//! change is just another caller of `leave`.

use super::buffer::{Buffer, SourceRow};
use super::plan::{Plan, Refusal};
use super::session::{Confirming, Editing};
use crate::Entry;

/// What the panel is doing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Mode {
    /// Moving around the tree. The only mode that holds no buffer, and the
    /// only one the panel starts in.
    #[default]
    Browse,
    /// Editing the rows as text.
    Edit(Editing),
    /// Looking at what the edits would do, before any of it happens.
    Confirm(Confirming),
}

/// What came of asking to confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmation {
    /// The plan validated; the mode is now [`Mode::Confirm`].
    Ready,
    /// The plan was refused. The mode is unchanged and the reasons are on
    /// [`Editing::refusals`].
    Refused,
    /// There is nothing to do — the buffer holds no changes. The mode is
    /// unchanged, deliberately: a confirmation screen listing no operations
    /// tells the user nothing and costs them a keystroke to dismiss.
    NothingToDo,
    /// Not editing, so there was nothing to confirm.
    NotEditing,
}

/// What came of asking to leave an editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leaving {
    /// The mode is now [`Mode::Browse`] and nothing was lost — either there
    /// was nothing to lose, or there was nothing being edited to begin with.
    Left,
    /// ⚠️ **The mode is unchanged.** There are edits that have not been
    /// applied, and dropping them is not this module's decision. The caller
    /// asks, then calls [`Mode::discard`] or does not.
    Unsaved,
}

impl Mode {
    /// Whether the panel is in an editing mode, and so owns a buffer.
    ///
    /// The question every caller outside this module actually has: browse-mode
    /// keys and edit-mode keys are disjoint sets, and the row composer draws
    /// names differently once they are editable.
    #[must_use]
    pub const fn is_editing(&self) -> bool {
        matches!(self, Self::Edit(_) | Self::Confirm(_))
    }

    /// The buffer, if there is one.
    #[must_use]
    pub const fn buffer(&self) -> Option<&Buffer> {
        match self {
            Self::Browse => None,
            Self::Edit(editing) => Some(&editing.buffer),
            Self::Confirm(confirming) => Some(&confirming.editing.buffer),
        }
    }

    /// Which buffer row is being edited, if one is.
    ///
    /// Answers only in [`Mode::Edit`]. A confirmation draws the plan rather
    /// than the rows, so it has no cursor to place and nothing may move one.
    #[must_use]
    pub const fn cursor(&self) -> Option<usize> {
        match self {
            Self::Edit(editing) => Some(editing.cursor),
            Self::Browse | Self::Confirm(_) => None,
        }
    }

    /// The edited name under the cursor, for drawing its caret.
    ///
    /// `None` both when nothing is being edited and when the row under the
    /// cursor holds a name the field cannot; see [`Editing::name`].
    #[must_use]
    pub const fn name(&self) -> Option<&Entry> {
        match self {
            Self::Edit(editing) => editing.name.as_ref(),
            Self::Browse | Self::Confirm(_) => None,
        }
    }

    /// The plan being confirmed, if the panel is confirming one.
    #[must_use]
    pub const fn plan(&self) -> Option<&Plan> {
        match self {
            Self::Confirm(confirming) => Some(&confirming.plan),
            Self::Browse | Self::Edit(_) => None,
        }
    }

    /// Why the last confirmation attempt was turned down.
    ///
    /// Empty in every mode but [`Mode::Edit`], and empty there until something
    /// is refused.
    #[must_use]
    pub fn refusals(&self) -> &[Refusal] {
        match self {
            Self::Edit(editing) => &editing.refusals,
            Self::Browse | Self::Confirm(_) => &[],
        }
    }

    /// Starts editing the rows the panel is drawing.
    ///
    /// Returns whether the mode changed. Asking to edit while already editing
    /// is **ignored rather than treated as a reload**, and that is the whole
    /// point: a second press of the edit key must not silently replace a
    /// buffer full of work with the rows as they are on disk now.
    pub fn begin_edit(&mut self, source: &[SourceRow]) -> bool {
        if self.is_editing() {
            return false;
        }
        *self = Self::Edit(Editing::new(source));
        true
    }

    /// The session being edited, if the mode is one that may be edited.
    ///
    /// ⚠️ `Confirm` deliberately answers `None` even though it holds a session.
    /// A confirmation screen states what *will* happen; letting a key mutate
    /// the rows underneath it would leave the displayed plan describing
    /// something else, and the plan is not recomputed until the user goes back.
    /// So the two are separated here rather than by a rule in `edit_keys`.
    ///
    /// The verbs on the far side of it are [`Editing`]'s, and every one of them
    /// is a no-op for the panel when this is `None` — which is what the thin
    /// wrappers below are for: a call site that had to unwrap this first would
    /// be a call site that could forget which mode it was in.
    pub const fn editing_mut(&mut self) -> Option<&mut Editing> {
        match self {
            Self::Edit(editing) => Some(editing),
            Self::Browse | Self::Confirm(_) => None,
        }
    }

    /// Puts the cursor on `index`; see [`Editing::seat`].
    pub fn seat(&mut self, index: usize) {
        if let Some(editing) = self.editing_mut() {
            editing.seat(index);
        }
    }

    /// Whether the row under the cursor was typed; see
    /// [`Editing::cursor_is_typed`].
    #[must_use]
    pub fn cursor_is_typed(&self) -> bool {
        matches!(self, Self::Edit(editing) if editing.cursor_is_typed())
    }

    /// Changes the name under the cursor; see [`Editing::edit_name`].
    pub fn edit_name(&mut self, change: impl FnOnce(&mut Entry) -> bool) -> bool {
        self.editing_mut()
            .is_some_and(|editing| editing.edit_name(change))
    }

    /// Moves the caret within that name; see [`Editing::move_caret`].
    pub fn move_caret(&mut self, motion: impl FnOnce(&mut Entry) -> bool) -> bool {
        self.editing_mut()
            .is_some_and(|editing| editing.move_caret(motion))
    }

    /// Strikes the row under the cursor through, or unstrikes it; see
    /// [`Editing::toggle_deleted`].
    pub fn toggle_deleted(&mut self) -> bool {
        self.editing_mut().is_some_and(Editing::toggle_deleted)
    }

    /// Types a new row below the cursor; see [`Editing::insert_row`].
    pub fn insert_row(&mut self) -> Option<usize> {
        self.editing_mut().and_then(Editing::insert_row)
    }

    /// Takes a typed row back out; see [`Editing::remove_typed_row`].
    pub fn remove_typed_row(&mut self) -> bool {
        self.editing_mut().is_some_and(Editing::remove_typed_row)
    }

    /// Arms the next escape to discard; see [`Editing::arm_discard`].
    pub const fn arm_discard(&mut self) -> bool {
        match self.editing_mut() {
            Some(editing) => editing.arm_discard(),
            None => false,
        }
    }

    /// Forgets that an escape was refused, so the next one asks again.
    ///
    /// Called for every key that is not the second escape. A question the user
    /// walked away from must not still be answerable later by an escape they
    /// meant as "stop editing".
    pub const fn disarm_discard(&mut self) {
        if let Some(editing) = self.editing_mut() {
            editing.discard_armed = false;
        }
    }

    /// Records that the buffer was edited.
    ///
    /// Clears any refusal, because a refusal names a problem in the rows and
    /// the rows have just changed. The verbs on [`Editing`] do this for
    /// themselves; this is for the caller that has changed nothing and only
    /// wants the refusal off the screen — the escape that dismisses one.
    pub fn note_edit(&mut self) {
        if let Self::Edit(editing) = self {
            editing.refusals.clear();
        }
    }

    /// Asks to move on to the confirmation.
    ///
    /// The plan is computed here and *kept*, rather than recomputed when the
    /// confirmation is drawn or when it is applied. Recomputing would let the
    /// screen and the action disagree the moment anything changed between
    /// them, which is exactly the window a confirmation exists to close.
    pub fn request_confirm(&mut self) -> Confirmation {
        let Self::Edit(editing) = self else {
            return Confirmation::NotEditing;
        };
        if !editing.buffer.is_dirty() {
            return Confirmation::NothingToDo;
        }
        match editing.buffer.plan() {
            Ok(plan) if plan.is_empty() => Confirmation::NothingToDo,
            Ok(plan) => {
                *self = Self::Confirm(Confirming {
                    editing: editing.clone(),
                    plan,
                });
                Confirmation::Ready
            },
            Err(refusals) => {
                editing.refusals = refusals;
                editing.discard_armed = false;
                Confirmation::Refused
            },
        }
    }

    /// Goes back from the confirmation to the edits that produced it.
    ///
    /// Returns whether there was a confirmation to go back from. The buffer
    /// survives, which is the reason [`Confirming`] carries one at all.
    pub fn back_to_edit(&mut self) -> bool {
        let Self::Confirm(confirming) = self else {
            return false;
        };
        let mut editing = confirming.editing.clone();
        editing.refusals.clear();
        editing.discard_armed = false;
        *self = Self::Edit(editing);
        true
    }

    /// Asks to stop editing.
    ///
    /// ⚠️ **Refuses while there is unapplied work**, reporting
    /// [`Leaving::Unsaved`] and leaving the mode alone. This is the module's
    /// one rule and every exit runs through it — the escape key, the filter
    /// change of step 5, and closing the panel are all the same caller.
    ///
    /// Leaving from [`Mode::Browse`] is [`Leaving::Left`] rather than an error:
    /// "make sure we are not editing" is a reasonable thing to ask, and a
    /// caller that has to check the mode first would be a caller that can
    /// forget to.
    pub fn leave(&mut self) -> Leaving {
        if self.buffer().is_some_and(Buffer::is_dirty) {
            return Leaving::Unsaved;
        }
        *self = Self::Browse;
        Leaving::Left
    }

    /// Throws the edits away and returns to browsing.
    ///
    /// The only thing here that loses work, and named for it. Nothing calls
    /// this on its own behalf: it exists for a caller that has asked and been
    /// told yes.
    pub fn discard(&mut self) {
        *self = Self::Browse;
    }

    /// Returns to browsing after a plan has been carried out.
    ///
    /// Distinct from [`Self::discard`] despite doing the same thing to the
    /// mode, because the two are opposite events and a reader of a call site
    /// should not have to work out which one happened. It also refuses to run
    /// from anywhere but a confirmation, so "applied" cannot be recorded for a
    /// plan that was never confirmed.
    pub fn applied(&mut self) -> bool {
        if !matches!(self, Self::Confirm(_)) {
            return false;
        }
        *self = Self::Browse;
        true
    }
}
