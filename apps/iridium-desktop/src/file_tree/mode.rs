//! What the panel is doing: browsing, editing, or confirming.
//!
//! Step 4b of `docs/IN-FLIGHT-oil.md`, minus its keys. The build order there
//! puts the dangerous logic first and fully tested, before any of it is
//! reachable from a keyboard — `plan` and `apply` were built that way and this
//! follows them. **Nothing in this module names a key**, so the three bindings
//! still waiting on a ruling are one table row each whenever they arrive, and
//! none of them can change what is written here.
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

/// The state of an editing session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editing {
    /// The rows as loaded and as they now read.
    pub buffer: Buffer,
    /// Why the last attempt to confirm was turned down, or empty.
    ///
    /// Held rather than returned and forgotten: a refusal is something the
    /// panel keeps *showing* until the edit that caused it is fixed, so it is
    /// state. Cleared on every edit, because a refusal that outlives its cause
    /// is worse than none — it points at a problem that is no longer there.
    pub refusals: Vec<Refusal>,
}

/// The state of a confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirming {
    /// The buffer that produced the plan, kept so that going back returns to
    /// the edits rather than to the rows as they were loaded.
    pub buffer: Buffer,
    /// What would happen.
    pub plan: Plan,
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
            Self::Confirm(confirming) => Some(&confirming.buffer),
        }
    }

    /// The buffer to edit, if the mode is one that may be edited.
    ///
    /// ⚠️ `Confirm` deliberately answers `None` even though it holds a buffer.
    /// A confirmation screen states what *will* happen; letting a key mutate
    /// the buffer underneath it would leave the displayed plan describing
    /// something else, and the plan is not recomputed until the user goes back.
    /// So the two are separated here rather than by a rule in `keys`.
    pub const fn buffer_mut(&mut self) -> Option<&mut Buffer> {
        match self {
            Self::Edit(editing) => Some(&mut editing.buffer),
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
        *self = Self::Edit(Editing {
            buffer: Buffer::load(source),
            refusals: Vec::new(),
        });
        true
    }

    /// Records that the buffer was edited.
    ///
    /// Clears any refusal, because a refusal names a problem in the rows and
    /// the rows have just changed. Callers that reach the buffer through
    /// [`Self::buffer_mut`] call this straight after; it is separate from that
    /// accessor because a borrow cannot both hand out the buffer and observe
    /// what was done to it.
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
                    buffer: editing.buffer.clone(),
                    plan,
                });
                Confirmation::Ready
            },
            Err(refusals) => {
                editing.refusals = refusals;
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
        *self = Self::Edit(Editing {
            buffer: confirming.buffer.clone(),
            refusals: Vec::new(),
        });
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
