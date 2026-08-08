//! What the auto-pair insertion wrote, remembered so Backspace can undo
//! exactly that and nothing else.
//!
//! ⭐⭐ **Ruling B-13 (`docs/design/AUTO-PAIR-MAP.md` §10.12).** The
//! multi-character collapse must fire only where the editor itself wrote the
//! closer, and *"did the editor write this"* is a fact about **what happened**
//! while a buffer records only **what is**. Three rounds of positional probes —
//! at the caret, in front of the opener, past the closer — were each refuted by
//! a case at a scope boundary the position could not see, because every one of
//! them was a proxy for history reconstructed from a post-edit buffer. No such
//! reconstruction can work; this module is the fact itself.
//!
//! # The record
//!
//! One record per keystroke, because one keystroke produces one command across
//! every caret. It carries three things and is validated against all three:
//!
//! - the **document identity**, so a record cannot survive a switch to another
//!   file whose revision counter happens to agree ([`Document::id`] exists for
//!   precisely this, and says so). ⚠️ **Belt and braces today, and this sentence
//!   used to overstate it.** The only document swap a handler can see is
//!   [`EditorState::set_content`](crate::editor::EditorState), which builds the
//!   replacement with [`Document::continuing_from`] — and that bumps the
//!   revision, so on every *reachable* swap the revision term has already
//!   refused. The identity term is what keeps that true if a future path ever
//!   installs a document without bumping it, which is a mechanism this crate
//!   does not currently have. It is pinned directly by
//!   `b13_the_document_identity_term_refuses_a_different_file_at_one_revision`
//!   rather than through a keystroke, because no keystroke can reach it;
//!   deleting the term leaves every end-to-end test green, which is exactly the
//!   shape of defect that refuted rounds 2 through 4 of this ruling;
//! - the **content revision** the insertion produced, so any later edit —
//!   including one the host makes without moving a caret, and including undo or
//!   redo, which bump the counter even when they restore identical text —
//!   leaves the record unable to match;
//! - the **exact cursor state** the insertion produced, so a motion, a click, a
//!   selection change, a cursor added or removed, or an edit at another caret
//!   all leave it unable to match.
//!
//! ⭐ **The discipline is [`super::KeyboardHandler::preferred_columns`]'s**, whose doc
//! comment already argues it: *"validating by full cursor-state identity
//! (rather than by cursor count) is what makes any intervening path
//! self-invalidate the sticky column without an explicit reset."* The same
//! sentence holds here, which is why nothing in this crate resets the record.
//! There is no path that must remember to.
//!
//! ⚠️ **A round trip that restores the exact state does revive the record, and
//! that is correct here** — where for sticky columns it was not, hence
//! `sticky_dirty`. The two differ because they claim different things. A sticky
//! column claims *what the user meant*, which a left-then-right trip destroys.
//! This record claims *the editor wrote this closer at this caret*, and a
//! document with the same identity and the same revision is the same bytes, so
//! that claim is still true. The delete it authorises removes exactly the text
//! the insertion added.
//!
//! ⭐ **That revival is pinned, not merely argued.** It reads as a bug to a
//! reader meeting it cold, so the obvious "fix" is to reset the record on any
//! motion — and without a test asserting the revival *direction*, that change
//! passes every gate. `b13_a_motion_round_trip_back_to_the_recorded_state_revives`
//! is the test that fails instead. The invalidation tests all assert refusal,
//! and refusal tests cannot catch a guard that became too eager.
//!
//! # Why the state is measured rather than predicted
//!
//! [`AutoPairInsertion::capture`] applies the command to a throwaway clone and reads the
//! revision and cursor state off it. The alternative — deriving the cursor
//! state from the command's trailing `SetSelection` and the revision by
//! counting which sub-commands [`Command::apply`] bumps for — would put a
//! second, silently-drifting copy of `apply`'s rules in this file. A rope clone
//! is a cheap structural share and the command is at most a handful of
//! characters, so the measurement costs less than the tree-sitter query the
//! approach it replaces ran on every Backspace.

use crate::document::{CursorState, Document, Position, Selection};
use crate::history::Command;

/// The closers one auto-pair keystroke wrote, and the exact editor state it
/// wrote them into.
///
/// Held by [`KeyboardHandler`](super::KeyboardHandler); see the module docs for
/// what each field is validated against and why.
#[derive(Debug, Clone)]
pub(super) struct AutoPairInsertion {
    /// [`Document::id`] of the document the insertion was made in.
    document: u64,

    /// [`Document::revision`] immediately after the insertion command applied.
    revision: u64,

    /// The full cursor state immediately after the insertion command applied.
    state: CursorState,

    /// The closer written at each caret, keyed by that caret's post-insertion
    /// head.
    ///
    /// Only carets the editor actually wrote a multi-character closer at appear
    /// here: one keystroke can pair at one caret and be refused at another (a
    /// different scope, a different character in front), and the caret that was
    /// refused must get the single-character answer on Backspace.
    ///
    /// A `Vec` rather than a map because it holds one entry per caret that
    /// paired — one, in every session that is not multi-cursor — and a linear
    /// scan over that beats hashing a [`Position`].
    closers: Vec<(Position, &'static str)>,
}

impl AutoPairInsertion {
    /// Records what `command` is about to write, or `None` when it writes no
    /// multi-character closer at all.
    ///
    /// `landed` is one post-edit selection per input cursor **in `cursor`'s own
    /// order**, as
    /// [`build_multi_cursor_command_reporting`](super::editing::build_multi_cursor_command_reporting)
    /// reports them, and `closers` is the closer each of those cursors had
    /// written for it — `None` where the insertion path refused. The two are
    /// zipped, so a length disagreement (a caller bug) yields no record rather
    /// than a misaligned one.
    ///
    /// `None` is the safe answer everywhere: without a record the collapse
    /// falls through to the single-character rule, which under-deletes at
    /// worst.
    pub(super) fn capture(
        document: &Document,
        cursor: &CursorState,
        command: &Command,
        landed: &[Selection],
        closers: &[Option<&'static str>],
    ) -> Option<Self> {
        if landed.len() != closers.len() {
            return None;
        }
        let written: Vec<(Position, &'static str)> = landed
            .iter()
            .zip(closers)
            .filter_map(|(selection, closer)| Some((selection.head, (*closer)?)))
            .collect();
        if written.is_empty() {
            return None;
        }

        // Measured, not predicted — see the module docs. A command that cannot
        // apply here would not apply for the caller either, and leaving no
        // record is the safe response to that.
        let mut scratch = document.clone();
        let mut state = cursor.clone();
        command.apply(&mut scratch, &mut state).ok()?;

        Some(Self {
            document: document.id(),
            revision: scratch.revision(),
            state,
            closers: written,
        })
    }

    /// Whether this record describes `document` and `cursor` exactly.
    ///
    /// All three keys must agree. Any disagreement means something happened
    /// between the insertion and now, and the record says nothing about the
    /// buffer as it stands.
    pub(super) fn describes(&self, document: &Document, cursor: &CursorState) -> bool {
        self.document == document.id()
            && self.revision == document.revision()
            && self.state == *cursor
    }

    /// The closer the insertion wrote at `head`, or `None` when it wrote none
    /// there.
    ///
    /// Only meaningful once [`Self::describes`] has agreed; a caret this record
    /// does not name gets the single-character answer.
    pub(super) fn closer_at(&self, head: Position) -> Option<&'static str> {
        self.closers
            .iter()
            .find(|(position, _)| *position == head)
            .map(|&(_, closer)| closer)
    }
}
