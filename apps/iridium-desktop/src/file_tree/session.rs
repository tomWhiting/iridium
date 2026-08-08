//! One editing session: the rows being edited, where the cursor is in them,
//! and the verbs that change either.
//!
//! Split from [`super::mode`] by what each answers. That module owns the
//! *transitions* — when a session starts, when it may be left, when a plan is
//! computed and kept — and this one owns everything that is true **while** a
//! session is open. A defect here presents as "the caret is on the wrong row"
//! or "the row I typed went in the wrong folder"; a defect there presents as
//! "my work was thrown away".
//!
//! # Everything here dies with the session
//!
//! [`Editing`] lives inside [`Mode::Edit`](super::mode::Mode::Edit), which is
//! the same argument that module makes about the buffer: a cursor hung off the
//! panel would be a second piece of state that has to agree with a buffer that
//! may not exist. Here it cannot outlive one.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::buffer::Buffer;
use super::plan::{Plan, Refusal};
use crate::prompt::Entry;

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
    /// Which row of the buffer is being edited.
    ///
    /// ⚠️ **An index into [`Buffer::rows`], and nothing else.** The panel's
    /// browse selections — the tree's and the filtered view's — index the
    /// *source* lists, which stop agreeing with the buffer the moment a row is
    /// typed. Acting on the wrong one strikes through the wrong file.
    pub cursor: usize,
    /// The name of the row under the cursor, with a caret in it.
    ///
    /// `None` when that row's name holds something an [`Entry`] cannot: the
    /// field refuses control characters, and a filesystem does not. Seeding it
    /// with the control characters quietly removed would make the very next
    /// keystroke write a name the user never typed back over the row, so the
    /// row is left uneditable instead.
    pub name: Option<Entry>,
    /// The folders that were drawn **showing their contents** when the session
    /// began, by the path they had then.
    ///
    /// Keyed by path rather than by row index because indices shift under
    /// every insertion and removal, and an origin path is the one thing about
    /// a row that an editing session never changes. See
    /// [`SourceRow::open`](super::buffer::SourceRow::open) for what the answer
    /// decides.
    pub open: BTreeSet<PathBuf>,
    /// Whether an escape has already been refused for unapplied edits, so the
    /// next one throws them away.
    ///
    /// [`Mode::leave`](super::mode::Mode::leave) says a dirty buffer is never
    /// dropped without being asked about, and this is the asking: the first
    /// press reports the refusal, the second answers it. Cleared by every
    /// other key, so a question asked a minute ago cannot be answered by an
    /// escape meant for something else.
    pub discard_armed: bool,
}

/// The state of a confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirming {
    /// The session that produced the plan, kept whole so that going back
    /// returns to the edits — and to the row the cursor was on — rather than
    /// to the rows as they were loaded.
    pub editing: Editing,
    /// What would happen.
    pub plan: Plan,
}

impl Editing {
    /// A session over `source`, with the cursor on its first row.
    pub fn new(source: &[super::buffer::SourceRow]) -> Self {
        let mut editing = Self {
            buffer: Buffer::load(source),
            refusals: Vec::new(),
            cursor: 0,
            name: None,
            open: source
                .iter()
                .filter(|row| row.open)
                .map(|row| row.path.clone())
                .collect(),
            discard_armed: false,
        };
        editing.seat(0);
        editing
    }

    /// Moves the cursor to `index` and reseats the field on that row's name.
    ///
    /// **The only place the cursor moves**, so the field and the row it
    /// describes cannot drift apart: every caller that changes which row is
    /// being edited goes through here, and there is no way to set one without
    /// the other.
    ///
    /// Clamped rather than refused. A session starts from the panel's browse
    /// selection, which indexes a list that can be shorter than the buffer, and
    /// a removal shortens the buffer under a cursor already placed. A cursor
    /// landing on the last row is a mistake the user can see and correct; one
    /// silently left where there is no row is not.
    ///
    /// A name the field cannot hold leaves it at `None` rather than seeded
    /// with a filtered copy. [`Entry::insert`] refuses control characters and
    /// a filesystem does not, so a lossy seed would mean the first keystroke
    /// silently renamed the row to the field's idea of it — a rename the user
    /// never asked for, of a file they only meant to walk past.
    pub fn seat(&mut self, index: usize) {
        let last = self.buffer.rows().len().saturating_sub(1);
        self.cursor = index.min(last);
        self.name = self.buffer.rows().get(self.cursor).and_then(|row| {
            let mut entry = Entry::new();
            for character in row.name.chars() {
                if !entry.insert(character) {
                    return None;
                }
            }
            Some(entry)
        });
    }

    /// Whether the row under the cursor is one the user typed rather than one
    /// that came from the tree.
    ///
    /// The distinction the keys need: an existing row is struck through so the
    /// buffer keeps saying what is on disk, and a typed row is simply taken
    /// back out.
    pub fn cursor_is_typed(&self) -> bool {
        self.buffer
            .rows()
            .get(self.cursor)
            .is_some_and(|row| row.origin.is_none())
    }

    /// Changes the name under the cursor with `change`, and writes it back.
    ///
    /// Returns whether anything changed. **The write-back is why this takes a
    /// closure rather than handing out the field.** The buffer is what gets
    /// planned and the field is what gets drawn, so a caller that mutated one
    /// and forgot the other would show a name that is not the one applied —
    /// and the forgetting would be silent. Here it cannot be forgotten, and
    /// the refusal-clearing that every edit owes goes with it.
    pub fn edit_name(&mut self, change: impl FnOnce(&mut Entry) -> bool) -> bool {
        let Some(name) = self.name.as_mut() else {
            return false;
        };
        if !change(name) {
            return false;
        }
        let text = name.text().to_owned();
        self.buffer.rename(self.cursor, &text);
        self.edited();
        true
    }

    /// Moves the caret within the name under the cursor.
    ///
    /// Separate from [`Self::edit_name`] because a motion changes nothing that
    /// gets applied: routing it through the write-back would clear a refusal
    /// the rows still deserve, and tell the user a problem had been fixed by
    /// pressing the left arrow.
    pub fn move_caret(&mut self, motion: impl FnOnce(&mut Entry) -> bool) -> bool {
        self.name.as_mut().is_some_and(motion)
    }

    /// Strikes the row under the cursor through, or unstrikes it.
    ///
    /// Returns whether there was a row to strike. The root is **not** refused
    /// here — [`super::plan`] owns that rule and states it in the confirmation
    /// — but the keys refuse it a keystroke earlier so the message is not one
    /// press late; see [`super::edit_keys`].
    pub fn toggle_deleted(&mut self) -> bool {
        if self.buffer.rows().get(self.cursor).is_none() {
            return false;
        }
        self.buffer.toggle_deleted(self.cursor);
        self.edited();
        true
    }

    /// Types a new, empty row below the cursor and moves the cursor onto it.
    ///
    /// Returns where it landed. ⚠️ **Whether it goes *inside* the row above or
    /// beside it is decided from [`Self::open`]**, which is what the panel was
    /// drawing when the session began — not from the row's kind alone. A closed
    /// folder's contents are not on screen, so a row typed under it is drawn
    /// beside it and must be created beside it. This is the one placement
    /// mistake [`super::plan`] cannot catch: a typed row has no origin, so
    /// there is no real nesting to check the drawn nesting against.
    ///
    /// A folder the user typed is always open: it does not exist yet, has
    /// nothing in it, and the only reason to type a row under one is to put
    /// something in it.
    pub fn insert_row(&mut self) -> Option<usize> {
        let row = self.buffer.rows().get(self.cursor)?;
        let into_folder = row.directory
            && row
                .origin
                .as_ref()
                .is_none_or(|origin| self.open.contains(&origin.path));
        let at = self.buffer.insert_below(self.cursor, into_folder);
        self.seat(at);
        self.edited();
        Some(at)
    }

    /// Takes the typed row under the cursor back out, and everything drawn
    /// inside it.
    ///
    /// Returns whether it did. Refuses a row that came from the tree: an
    /// existing file leaves the buffer only by being struck through, so that
    /// the rows keep saying what is on the disk.
    pub fn remove_typed_row(&mut self) -> bool {
        if !self.buffer.remove_typed(self.cursor) {
            return false;
        }
        // The row that was there is gone, so the cursor lands on whatever took
        // its place — or on the row above when it was the last. `seat` clamps,
        // so the arithmetic only has to name the row that is wanted.
        let cursor = self.cursor;
        self.seat(cursor);
        self.edited();
        true
    }

    /// Arms the next escape to throw the edits away, and reports whether it
    /// was already armed.
    ///
    /// The two halves of asking, in one call: the caller gets `true` when the
    /// user has already been told and is answering, and `false` when this is
    /// the telling.
    pub const fn arm_discard(&mut self) -> bool {
        let armed = self.discard_armed;
        self.discard_armed = true;
        armed
    }

    /// What every mutation above owes: the refusals named a problem in rows
    /// that have just changed, and the discard question was asked of a buffer
    /// the user has since carried on working in.
    fn edited(&mut self) {
        self.refusals.clear();
        self.discard_armed = false;
    }
}
