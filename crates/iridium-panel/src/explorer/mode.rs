//! What the panel is doing: browsing, editing, or confirming.
//!
//! The session lives *inside* the variants that have one, so returning to
//! [`Mode::Browse`] drops it by construction and there is no way to be editing
//! without one.
//!
//! ⚠️ **Unapplied edits are never dropped without being asked about.** Every
//! exit from an editing mode goes through [`Mode::leave`], which refuses with
//! [`Leaving::Unsaved`] while the text says anything; [`Mode::discard`] is the
//! one call that throws work away, and it is named for it.

use super::buffer::Listing;
use super::plan::{Plan, Refusal};
use super::session::{Confirming, Editing};

/// What the panel is doing.
#[derive(Debug, Default)]
pub enum Mode {
    /// Moving around the tree. Holds no session.
    #[default]
    Browse,
    /// Editing the rows as text.
    Edit(Box<Editing>),
    /// Looking at what the edits would do, before any of it happens.
    Confirm(Confirming),
}

/// What came of asking to confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmation {
    /// The plan validated; the mode is now [`Mode::Confirm`].
    Ready,
    /// The plan was refused; the reasons are on the session.
    Refused,
    /// The text says nothing that would be applied. The mode is unchanged.
    NothingToDo,
    /// Not editing, so there was nothing to confirm.
    NotEditing,
}

/// What came of asking to leave an editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leaving {
    /// The mode is now [`Mode::Browse`] and nothing was lost.
    Left,
    /// ⚠️ The mode is unchanged: there are unapplied edits, and dropping them
    /// is the caller's question to ask.
    Unsaved,
}

impl Mode {
    /// Whether the panel is in an editing mode, and so owns a session.
    #[must_use]
    pub const fn is_editing(&self) -> bool {
        matches!(self, Self::Edit(_) | Self::Confirm(_))
    }

    /// The listing, if a session is open.
    #[must_use]
    pub fn listing(&self) -> Option<&Listing> {
        self.session().map(|editing| &editing.listing)
    }

    /// The session, in either editing mode.
    #[must_use]
    pub fn session(&self) -> Option<&Editing> {
        match self {
            Self::Browse => None,
            Self::Edit(editing) => Some(editing),
            Self::Confirm(confirming) => Some(&confirming.editing),
        }
    }

    /// The session whose text is on screen and may be edited.
    ///
    /// ⚠️ `Confirm` answers `None`: a confirmation states what *will* happen,
    /// and text changed underneath it would make the statement false.
    pub fn editing_mut(&mut self) -> Option<&mut Editing> {
        match self {
            Self::Edit(editing) => Some(editing),
            Self::Browse | Self::Confirm(_) => None,
        }
    }

    /// The session whose text is on screen, read-only.
    #[must_use]
    pub fn editing(&self) -> Option<&Editing> {
        match self {
            Self::Edit(editing) => Some(editing),
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

    /// Why the last confirmation attempt was turned down; empty outside
    /// [`Mode::Edit`] and until something is refused.
    #[must_use]
    pub fn refusals(&self) -> &[Refusal] {
        match self {
            Self::Edit(editing) => &editing.refusals,
            Self::Browse | Self::Confirm(_) => &[],
        }
    }

    /// Starts `editing`, unless a session is already open.
    ///
    /// Returns whether the mode changed. A second request is ignored rather
    /// than treated as a reload: it must not replace a session full of work.
    pub fn begin_edit(&mut self, editing: Editing) -> bool {
        if self.is_editing() {
            return false;
        }
        *self = Self::Edit(Box::new(editing));
        true
    }

    /// Forgets that an escape was refused, so the next one asks again.
    pub const fn disarm_discard(&mut self) {
        if let Self::Edit(editing) = self {
            editing.discard_armed = false;
        }
    }

    /// Takes the refusals off the screen, leaving the text as it is.
    pub fn dismiss_refusals(&mut self) {
        if let Self::Edit(editing) = self {
            editing.refusals.clear();
        }
    }

    /// Asks to move on to the confirmation.
    ///
    /// The plan is computed here and *kept*, so what the screen shows and what
    /// `y` applies cannot differ.
    pub fn request_confirm(&mut self) -> Confirmation {
        let Self::Edit(editing) = self else {
            return Confirmation::NotEditing;
        };
        editing.sync();
        if !editing.listing.is_dirty() {
            return Confirmation::NothingToDo;
        }
        match editing.listing.plan() {
            Ok(plan) if plan.is_empty() => Confirmation::NothingToDo,
            Ok(plan) => {
                if let Self::Edit(editing) = std::mem::take(self) {
                    *self = Self::Confirm(Confirming { editing, plan });
                }
                Confirmation::Ready
            },
            Err(refusals) => {
                editing.refusals = refusals;
                editing.discard_armed = false;
                Confirmation::Refused
            },
        }
    }

    /// Goes back from the confirmation to the text that produced it.
    ///
    /// Returns whether there was a confirmation to go back from.
    pub fn back_to_edit(&mut self) -> bool {
        if !matches!(self, Self::Confirm(_)) {
            return false;
        }
        if let Self::Confirm(confirming) = std::mem::take(self) {
            let mut editing = confirming.editing;
            editing.refusals.clear();
            editing.discard_armed = false;
            *self = Self::Edit(editing);
        }
        true
    }

    /// Asks to stop editing; ⚠️ refuses while there is unapplied work.
    pub fn leave(&mut self) -> Leaving {
        if self.listing().is_some_and(Listing::is_dirty) {
            return Leaving::Unsaved;
        }
        *self = Self::Browse;
        Leaving::Left
    }

    /// Throws the edits away and returns to browsing. The only thing here
    /// that loses work; nothing calls it without having asked.
    pub fn discard(&mut self) {
        *self = Self::Browse;
    }

    /// Returns to browsing after a plan has been carried out. Refuses from
    /// anywhere but a confirmation, so an unconfirmed plan cannot be recorded
    /// as applied.
    pub fn applied(&mut self) -> bool {
        if !matches!(self, Self::Confirm(_)) {
            return false;
        }
        *self = Self::Browse;
        true
    }
}
