//! The editor's retained syntax tree, and the rules for keeping it honest.
//!
//! One document, one tree. Before this, fold detection and highlighting each
//! parsed independently and the editor's own copy was refreshed only on load —
//! so folds were correct until the first keystroke and wrong from then on.
//!
//! # The contract
//!
//! Every text mutation must be reported through [`SyntaxState::note_edit`],
//! which shifts the retained tree's positions in O(tree depth) and parses
//! nothing. [`SyntaxState::refresh`] and [`SyntaxState::sync`] are what parse.
//!
//! Correctness does not depend on that reporting. Both compare the document
//! revision last seen with the one they are given: if a mutation slipped past
//! `note_edit`, the revisions disagree and the document is parsed whole. The
//! incremental path is an optimisation that can be missed; it is never the only
//! thing standing between the tree and the truth.
//!
//! # Where the parse runs
//!
//! Inline by default: `refresh` parses on the calling thread, so the tree is
//! current the moment it returns. That is what a test, a script or a binding
//! that cannot be woken wants. A face that draws frames asks for
//! [`SyntaxState::parse_in_background`] instead, and from then on `refresh`
//! never waits for a parser: the tree it leaves is the last one parsed, shifted
//! by every edit since, and the parse catches up on a thread of its own. See
//! the `background` module for why that is necessary rather than merely nice.

mod selection;
mod spatial;
mod staging;

use std::sync::Arc;

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::{
    Highlighter, InputEdit, ParseOutcome, SyntaxParser, Tree, changed_ranges,
};
use iridium_lang::Language;
#[cfg(feature = "syntax")]
use iridium_syntax::{Highlighter, InputEdit, ParseOutcome, SyntaxParser, Tree, changed_ranges};
use ropey::Rope;

use super::CaretScopes;
use super::ExpandStack;
use super::background::{Background, Gone, Landed, LandedFolds};
use super::delta::SyntaxDelta;
use super::expand::selection_for;
use crate::document::{CursorState, Document, EditSpan, byte_point};
use crate::input::keyboard::AstRequest;

/// Compact, document-independent record of a staged syntax edit.
pub(in crate::editor) struct PreparedSyntaxEdit {
    edit: InputEdit,
    revision: u64,
}

impl PreparedSyntaxEdit {
    pub(in crate::editor) fn new(document: &Document, span: &EditSpan) -> Option<Self> {
        let start = byte_point(document, span.start_byte)?;
        let end = byte_point(document, span.new_end_byte)?;
        Some(Self {
            edit: edit_for(span, start, end),
            revision: document.revision(),
        })
    }
}

/// Called from the background parser's thread each time a parse lands, so a
/// face that sleeps between events knows to draw again.
///
/// It must be cheap and must not block: it runs on the parser's thread, and
/// the next parse waits for it to return. Waking an event loop is the job.
pub type SyntaxWake = Arc<dyn Fn() + Send + Sync>;

/// The parse tree for the document being edited, and how current it is.
#[derive(Debug, Default)]
pub struct SyntaxState {
    /// The parser, when a language with a linked grammar is set.
    ///
    /// Present even while parsing runs in the background: it is what answers
    /// "is there a grammar at all", and what a structural verb falls back on
    /// if the background thread is ever lost.
    parser: Option<SyntaxParser>,
    /// The tree: the last one parsed, shifted by every edit reported since.
    tree: Option<Tree>,
    /// The compiled highlight query for [`Self::language`], or `None` for a
    /// language that ships no `highlights.scm` — every vendored one does, so
    /// this is `None` only in the parser-free build and before a language is
    /// set.
    ///
    /// Held here rather than built per lookup because
    /// [`Highlighter::new`] allocates a per-capture mapping table, and the one
    /// caller — [`Self::caret_scopes`] — is on the typing path. The query
    /// itself is already process-wide; this is the small table beside it.
    highlighter: Option<Highlighter>,
    /// The language the tree is parsed with.
    language: Option<Language>,
    /// The document revision the tree has been *told about*, whether or not it
    /// has been reparsed since.
    ///
    /// A disagreement between this and the document's own revision is the
    /// signal that an edit was never reported, and the reason a missed
    /// `note_edit` degrades to a full parse instead of to a wrong tree.
    tracked_revision: u64,
    /// True when edits have been applied to the tree but no inline parse has
    /// run. The background parser keeps its own account of this.
    dirty: bool,
    /// True when the document was replaced wholesale rather than edited.
    ///
    /// A revision comparison cannot catch that on its own: a replacement
    /// document starts its count again, so a tree parsed from the old text at
    /// revision 3 and a new document at revision 3 look identical. This says
    /// outright that the two are unrelated.
    replaced: bool,
    /// How many times the document has been parsed whole.
    full_parses: u64,
    /// How many times the tree has been reparsed against an edited copy.
    incremental_parses: u64,
    /// The selections an expansion walked out through, so a shrink can retrace
    /// them.
    ///
    /// Syntax state rather than keyboard state: it describes where in the *tree*
    /// the selection came from, and it must survive a keymap being pushed or
    /// popped underneath it.
    expand: ExpandStack,
    /// Where syntax navigation last left the selection, for the next step.
    navigation: super::navigation::NavigationState,
    /// What has happened to the tree since a consumer last took the delta.
    pending_delta: SyntaxDelta,
    /// Fold regions a background parse delivered, not yet taken.
    ///
    /// When this is present, [`Self::pending_delta`] describes what happened
    /// *after* the parse these regions belong to, not after the consumer's
    /// own regions — see [`Self::take_landed_folds`].
    landed_folds: Option<LandedFolds>,
    /// The background parser, once a face has asked for one.
    background: Option<Background>,
}

impl Drop for SyntaxState {
    fn drop(&mut self) {
        // A tree can be millions of nodes, and the last owner frees them all.
        // With a worker running, that owner is made to be the worker.
        if let (Some(background), Some(tree)) = (self.background.as_ref(), self.tree.take()) {
            background.dispose(tree);
        }
    }
}

impl SyntaxState {
    /// Creates a syntax state with no language and no tree.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the language the tree is parsed with, if any.
    #[must_use]
    pub const fn language(&self) -> Option<Language> {
        self.language
    }

    /// Moves parsing onto a background thread, so that [`Self::refresh`] never
    /// waits for a parser.
    ///
    /// `wake` is called from that thread whenever a parse lands; a face that
    /// blocks on input passes something that wakes it, and one that draws
    /// continuously can pass `None` and pick results up on its next frame.
    /// Calling this again replaces the wake.
    ///
    /// Returns whether parsing now runs in the background. It does not where
    /// the build has no parser or the target no threads, or if the system
    /// refused a thread — and parsing then stays inline, which is slower and
    /// still right.
    pub fn parse_in_background(&mut self, wake: Option<SyntaxWake>) -> bool {
        if let Some(background) = self.background.as_ref() {
            if background.set_wake(wake).is_ok() {
                return true;
            }
            self.abandon_background();
            return false;
        }
        // A tree parsed inline and not edited since is current: nothing is
        // owed until the next edit, and the first job then builds on it. The
        // worker's fold cache has seen no tree at all, so that first job
        // rebuilds it — on the worker. Anything less than current is parsed
        // from scratch there instead.
        let current = self.tree.is_some() && !self.dirty && !self.replaced;
        let Some(background) = Background::spawn(wake, current) else {
            return false;
        };
        self.background = Some(background);
        true
    }

    /// Whether parsing runs in the background.
    #[must_use]
    pub const fn parses_in_background(&self) -> bool {
        self.background.is_some()
    }

    /// Whether the tree is behind the document: an edit has been shifted into
    /// it that no finished parse has read yet.
    ///
    /// Always false straight after an inline [`Self::refresh`] or a
    /// [`Self::sync`]. With the parse in the background, true until the parse
    /// that covers the latest edit lands.
    #[must_use]
    pub fn is_behind(&self) -> bool {
        if self.parser.is_none() {
            return false;
        }
        self.background
            .as_ref()
            .map_or(self.dirty, Background::behind)
    }

    /// Sets the language, discarding any tree parsed with the previous one.
    ///
    /// A tree is only as meaningful as the grammar that produced it, so there
    /// is nothing here worth carrying across a language change.
    pub fn set_language(&mut self, language: Language) {
        self.language = Some(language);
        self.parser = SyntaxParser::new(language).ok();
        self.discard_tree();
        // Built here rather than lazily so that a language with no
        // `highlights.scm` is `None` from the moment it is set, instead of
        // costing a failed compile on every keystroke that asks for a scope.
        self.highlighter = Highlighter::new(language).ok();
        self.dirty = false;
        self.replaced = false;
        self.expand.clear();
        self.navigation.clear();
        // Nothing derived from the discarded tree survives a grammar change.
        self.pending_delta = SyntaxDelta::Full;
        self.landed_folds = None;
        // Nothing has been parsed for this language yet. Zero cannot collide
        // with a live document revision, which starts above it and only rises.
        self.tracked_revision = 0;
    }

    /// How many times this state has parsed the document whole.
    ///
    /// Exposed because "typing never parses" and "an edit reparses
    /// incrementally, not wholly" are claims about *how often* work happens,
    /// and a claim nothing can observe is a claim nothing can hold to. A
    /// background parse counts when it lands, not when it starts.
    #[must_use]
    pub const fn full_parses(&self) -> u64 {
        self.full_parses
    }

    /// How many times this state has reparsed against an edited tree.
    #[must_use]
    pub const fn incremental_parses(&self) -> u64 {
        self.incremental_parses
    }

    /// Marks the tree as describing a document that no longer exists.
    ///
    /// Call this whenever the document is replaced rather than edited. The next
    /// [`SyntaxState::refresh`] then parses from scratch instead of trusting a
    /// revision number that started over.
    pub fn invalidate(&mut self) {
        self.replaced = true;
        self.expand.clear();
        self.navigation.clear();
        self.pending_delta = SyntaxDelta::Full;
        self.landed_folds = None;
    }

    /// Clears the language and drops the tree.
    pub fn clear_language(&mut self) {
        self.language = None;
        self.parser = None;
        self.discard_tree();
        self.highlighter = None;
        self.dirty = false;
        self.replaced = false;
        self.expand.clear();
        self.navigation.clear();
        self.pending_delta = SyntaxDelta::Full;
        self.landed_folds = None;
        self.tracked_revision = 0;
    }

    /// Takes the description of everything that has happened to the tree since
    /// this was last called, leaving [`SyntaxDelta::Unchanged`] behind.
    ///
    /// Taken rather than read, so that two consumers cannot both believe they
    /// are seeing the change for the first time. There is one consumer today —
    /// fold detection — and it is the only thing that recomputes from the tree
    /// on every keystroke.
    ///
    /// Take [`Self::take_landed_folds`] first: when a background parse has
    /// landed, this delta is relative to the regions it delivered.
    pub fn take_delta(&mut self) -> SyntaxDelta {
        std::mem::take(&mut self.pending_delta)
    }

    /// Takes the fold regions a background parse delivered, if one landed
    /// since this was last called.
    ///
    /// When this returns regions, the fold consumer must take them over whole
    /// and then apply [`Self::take_delta`] to them; that delta holds exactly
    /// the edits made while the parse ran. A consumer that cannot take them
    /// over must treat the delta as [`SyntaxDelta::Full`] instead. Never
    /// returns anything while parsing is inline.
    pub const fn take_landed_folds(&mut self) -> Option<LandedFolds> {
        self.landed_folds.take()
    }

    /// Returns the retained tree without parsing.
    ///
    /// The tree may be behind the document — it is whatever the last parse
    /// produced, shifted by any edits reported since. Shifted, not stale: every
    /// node outside the edits sits at its right offset in the current text.
    /// Callers that need the structure an edit created must call
    /// [`Self::sync`] first.
    #[must_use]
    pub const fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// A resolver for what the text at a byte offset is syntactically inside,
    /// over the tree as it currently stands.
    ///
    /// Built over [`Self::tree`] and so **never parses** — see
    /// [`CaretScopes`] for why the resulting one-edit staleness is the
    /// direction that costs at most an over-fire, and why calling
    /// [`Self::sync`] here instead would put a reparse on every keystroke.
    ///
    /// Returns [`CaretScopes::none`] — a resolver that answers nothing, which
    /// consumers must read as *unknown* rather than *nothing* — when there is
    /// no tree yet, no language, or no highlight query for the language.
    ///
    /// `document` must be the document this state is tracking. Nothing here can
    /// check that, and the offsets a lookup returns are indices into its text.
    #[must_use]
    pub const fn caret_scopes<'a>(&'a self, document: &'a Document) -> CaretScopes<'a> {
        let (Some(tree), Some(highlighter)) = (self.tree(), self.highlighter.as_ref()) else {
            return CaretScopes::none();
        };
        CaretScopes::over(tree, highlighter, document)
    }

    /// Reports an edit against the tree, without parsing.
    ///
    /// `document` must be the document **after** the edit, and `span` must have
    /// been computed against it **before** — that is what
    /// [`crate::document::compute_edit_span`] returns, and why [`EditSpan`]
    /// carries the old end position rather than deriving it later from text
    /// that no longer contains it.
    ///
    /// If either position cannot be resolved, the edit is left unreported: the
    /// revision then disagrees at the next parse, which reads the document
    /// whole. Silently shifting the tree by a guessed amount is the one outcome
    /// this must never produce.
    pub fn note_edit(&mut self, document: &Document, span: &EditSpan) {
        // Unconditionally, and before anything can return early: the recorded
        // frames describe ranges in a document that no longer exists. The lazy
        // check inside the stack would catch this too, but only at the next
        // request — which would leave `expansion_depth` reporting frames that
        // are already dead.
        self.expand.clear();
        self.navigation.clear();

        if self.parser.is_none() {
            return;
        }
        if let Some(edit) = PreparedSyntaxEdit::new(document, span) {
            self.note_prepared_edit(&edit);
        }
    }

    /// Whether the retained tree accounts for every edit before a replay.
    pub(in crate::editor) const fn tracks_revision(&self, revision: u64) -> bool {
        !self.replaced && self.tracked_revision == revision
    }

    /// Publishes an edit whose post-edit coordinates were captured during a
    /// successful staged replay. No temporary document needs to be retained.
    pub(in crate::editor) fn note_prepared_edit(&mut self, prepared: &PreparedSyntaxEdit) {
        self.expand.clear();
        self.navigation.clear();
        if self.parser.is_none() {
            return;
        }
        let edit = prepared.edit;
        // With no tree yet there is nothing to shift, but the edit is still
        // recorded: a parse already running against the earlier text replays
        // it onto the tree it returns, and an inline parse from scratch reads
        // the text as it now is.
        if let Some(tree) = self.tree.as_mut() {
            tree.edit(&edit);
        }
        if let Some(background) = self.background.as_mut() {
            background.note_edit(edit);
        }
        self.pending_delta =
            std::mem::take(&mut self.pending_delta).then(SyntaxDelta::edited(edit));
        self.tracked_revision = prepared.revision;
        self.dirty = true;
    }

    /// Brings the tree as far up to date as can be done without waiting, and
    /// returns it.
    ///
    /// Inline, that is all the way: this is [`Self::sync`]. In the background,
    /// it installs a parse that has landed, replays onto it the edits made
    /// while it ran, and sends the next job if one is owed — and never blocks.
    /// The tree it returns is then the latest parse shifted by every edit
    /// since, which is what a frame should draw from.
    ///
    /// Returns `None` when no language is set, and — in the background — until
    /// the first parse of a document lands.
    pub fn refresh(&mut self, document: &Document) -> Option<&Tree> {
        self.parser.as_ref()?;
        if self.background.is_none() {
            return self.sync(document);
        }
        if self.advance_background(document, false).is_err() {
            self.abandon_background();
            return self.sync(document);
        }
        self.tree.as_ref()
    }

    /// Brings the tree fully up to date with `document` and returns it,
    /// waiting for a background parse if one is running.
    ///
    /// Parses only when something has changed: an incremental reparse when
    /// every edit was reported, a full parse when one was missed or when there
    /// is no tree yet, and nothing at all when the document has not moved since
    /// the last call.
    ///
    /// Returns `None` when no language is set, which is not an error — it means
    /// there is no structure to reason about and callers should do nothing.
    pub fn sync(&mut self, document: &Document) -> Option<&Tree> {
        self.parser.as_ref()?;
        if self.background.is_some() {
            if self.advance_background(document, true).is_err() {
                self.abandon_background();
            } else {
                return self.tree.as_ref();
            }
        }

        let revision = document.revision();
        let stale = self.replaced || self.tracked_revision != revision;
        let parser = self.parser.as_mut()?;

        let observed = if stale || self.tree.is_none() {
            // Either an edit never reached `note_edit` or there is no tree to
            // build on. Both are answered the same way, and the answer is
            // always right.
            self.tree = parsed(parser.parse(rope_reader(document.rope()), None, || true));
            if self.tree.is_some() {
                self.full_parses += 1;
            }
            SyntaxDelta::Full
        } else if self.dirty {
            // The reparse builds on the edited tree and leaves it intact, which
            // is what makes `changed_ranges` possible afterwards.
            let previous = self.tree.take();
            self.tree =
                parsed(parser.parse(rope_reader(document.rope()), previous.as_ref(), || true));
            match (previous.as_ref(), self.tree.as_ref()) {
                (Some(previous), Some(tree)) => {
                    self.incremental_parses += 1;
                    SyntaxDelta::reparsed(changed_ranges(previous, tree))
                },
                _ => SyntaxDelta::Full,
            }
        } else {
            SyntaxDelta::Unchanged
        };

        self.pending_delta = std::mem::take(&mut self.pending_delta).then(observed);
        self.tracked_revision = revision;
        self.dirty = false;
        self.replaced = false;
        self.tree.as_ref()
    }

    /// The background half of [`Self::refresh`] and [`Self::sync`]: install
    /// what has landed, send what is owed, and — when `wait` — repeat until
    /// the tree has caught up with `document`.
    fn advance_background(&mut self, document: &Document, wait: bool) -> Result<(), Gone> {
        loop {
            self.restart_if_stale(document);
            while let Some(landed) = self.background_mut()?.try_landed()? {
                self.install(landed)?;
            }
            if let Some(language) = self.language.filter(|_| self.parser.is_some()) {
                let tree = self.tree.as_ref();
                if let Some(background) = self.background.as_mut() {
                    background.dispatch(language, document, tree)?;
                }
            }
            let background = self.background_mut()?;
            if !wait || !background.in_flight() {
                return Ok(());
            }
            let landed = background.wait_landed()?;
            self.install(landed)?;
        }
    }

    /// The background parser, or [`Gone`] if there is none.
    fn background_mut(&mut self) -> Result<&mut Background, Gone> {
        self.background.as_mut().ok_or(Gone)
    }

    /// Starts over from scratch if the document moved without telling the
    /// tree, or was replaced.
    fn restart_if_stale(&mut self, document: &Document) {
        let revision = document.revision();
        if !self.replaced && self.tracked_revision == revision {
            return;
        }
        // The tree describes text that is gone: drawing from it would put
        // colours at offsets that mean nothing now. No tree is honest; the
        // parse from scratch replaces it.
        self.discard_tree();
        self.pending_delta = SyntaxDelta::Full;
        self.landed_folds = None;
        self.tracked_revision = revision;
        self.replaced = false;
        self.dirty = false;
    }

    /// Takes over a landed parse: replays the edits made while it ran, swaps
    /// it in for the shifted tree, and keeps its fold regions for the fold
    /// consumer.
    fn install(&mut self, landed: Landed) -> Result<(), Gone> {
        let Some((mut tree, folds, from_scratch)) = landed.unpack() else {
            // Tree-sitter documents no way to reach this with a language set
            // and no cancellation asked for. Rather than retry something that
            // should not have failed, parse inline from here on.
            return Err(Gone);
        };
        // Field by field, so the tree can be swapped while the worker handle
        // is borrowed.
        let background = self.background.as_ref().ok_or(Gone)?;
        let since: Vec<InputEdit> = background.edits().to_vec();
        for edit in &since {
            tree.edit(edit);
        }
        if let Some(previous) = self.tree.replace(tree) {
            background.dispose(previous);
        }
        if from_scratch {
            self.full_parses += 1;
        } else {
            self.incremental_parses += 1;
        }
        // The regions the parse delivered describe its own tree; the edits
        // made since are what bring them up to the tree as it now stands.
        self.landed_folds = Some(folds);
        self.pending_delta = if since.is_empty() {
            SyntaxDelta::Unchanged
        } else {
            SyntaxDelta::Incremental {
                edits: since,
                changed: Vec::new(),
            }
        };
        Ok(())
    }

    /// Stops using the background parser, after it was lost.
    ///
    /// The tree is kept: it has been shifted by every edit, so an inline
    /// reparse can build on it. Fold regions a parse delivered but nobody took
    /// are dropped, and the delta with them, because the fold consumer's own
    /// regions are still the ones that delta does not describe.
    fn abandon_background(&mut self) {
        self.background = None;
        if self.landed_folds.take().is_some() {
            self.pending_delta = SyntaxDelta::Full;
        }
        self.dirty = true;
    }

    /// Drops the tree — on the worker, when there is one.
    fn discard_tree(&mut self) {
        let tree = self.tree.take();
        if let Some(background) = self.background.as_mut() {
            if let Some(tree) = tree {
                background.dispose(tree);
            }
            background.restart();
        }
    }
}

/// The tree a parse produced, or `None` if it produced none.
#[cfg_attr(
    not(feature = "syntax"),
    expect(
        clippy::missing_const_for_fn,
        clippy::needless_pass_by_value,
        reason = "the stub tree is `Copy`, so the stub outcome is neither consumed \
        nor dropped here; the real one owns a tree with a destructor, and this one \
        function serves both"
    )
)]
fn parsed(outcome: ParseOutcome) -> Option<Tree> {
    match outcome {
        ParseOutcome::Parsed(tree) => Some(tree),
        ParseOutcome::Cancelled | ParseOutcome::Failed => None,
    }
}

/// Reads a rope one chunk at a time, for a parser.
///
/// Each read returns the rest of the chunk holding `byte`, and nothing at or
/// past the end. A rope's chunks are whole strings, so every piece ends on a
/// character boundary, which tree-sitter needs; and no piece is ever copied,
/// which is the point — building the whole text as one string copies the
/// document on every parse.
pub(super) fn rope_reader<'rope>(rope: &'rope Rope) -> impl FnMut(usize) -> &'rope [u8] {
    move |byte| {
        if byte >= rope.len() {
            return &[][..];
        }
        rope.get_chunk(byte)
            .and_then(|(chunk, start)| chunk.as_bytes().get(byte.checked_sub(start)?..))
            .unwrap_or(&[][..])
    }
}

impl SyntaxState {
    /// Computes the cursor state a structural request should produce.
    ///
    /// Brings the tree up to date first, because a structural verb is exactly
    /// the moment the tree has to be right — this is the "something needs the
    /// tree" that [`SyntaxState::sync`] exists to serve, and the only reason
    /// typing itself never parses.
    ///
    /// Returns `None` when nothing should change: no language, no tree, a
    /// selection already at the outermost node, a shrink with nothing left. The
    /// caller applies a selection command only when there is one to apply, so a
    /// held key neither errors nor fills the undo history with no-ops.
    pub fn apply_ast_request(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        request: AstRequest,
    ) -> Option<CursorState> {
        if super::navigation::is_navigation(request) {
            self.sync(document);
            self.expand.clear();
            return self
                .navigation
                .apply(document, cursor, self.tree.as_ref(), request);
        }
        self.navigation.clear();
        self.sync(document)?;

        // Copied out before the tree is borrowed. The named-region verbs need it
        // — the vendored queries are per-language files — and this field already
        // holds it, so nothing new has to be stored to answer them.
        let language = self.language;

        // Disjoint field borrows: the tree is read while the stack is written.
        let tree = self.tree.as_ref()?;
        selection_for(request, language, tree, document, cursor, &mut self.expand)
    }

    /// How many expansions the current stack can still undo.
    #[must_use]
    pub fn expansion_depth(&self) -> usize {
        self.expand.depth()
    }
}

/// Builds the tree-sitter edit descriptor for a span.
///
/// Split out so the coordinate juggling is in one place: the start position is
/// valid in both documents because everything before it is untouched, the old
/// end comes from the span (pre-edit), and the new end from the caller's
/// measurement of the document as it is now.
#[cfg(feature = "syntax")]
const fn edit_for(
    span: &EditSpan,
    start: (usize, usize),
    new_end: (usize, usize),
) -> iridium_syntax::InputEdit {
    use iridium_syntax::{InputEdit, Point};

    InputEdit {
        start_byte: span.start_byte,
        old_end_byte: span.old_end_byte,
        new_end_byte: span.new_end_byte,
        start_position: Point {
            row: start.0,
            column: start.1,
        },
        old_end_position: Point {
            row: span.old_end_row,
            column: span.old_end_column,
        },
        new_end_position: Point {
            row: new_end.0,
            column: new_end.1,
        },
    }
}

/// Builds the stub edit descriptor.
///
/// Identical arithmetic to the tree-sitter one, and it has to be: with no parser
/// the fold detector is a brace scan of the document text, and the only thing
/// that keeps that scan off the whole document is knowing which rows this edit
/// moved. The descriptor carried nothing until it needed to, which is why the
/// scan read everything.
#[cfg(not(feature = "syntax"))]
const fn edit_for(
    span: &EditSpan,
    start: (usize, usize),
    new_end: (usize, usize),
) -> crate::syntax_stubs::InputEdit {
    use crate::syntax_stubs::{InputEdit, Point};

    InputEdit {
        start_byte: span.start_byte,
        old_end_byte: span.old_end_byte,
        new_end_byte: span.new_end_byte,
        start_position: Point {
            row: start.0,
            column: start.1,
        },
        old_end_position: Point {
            row: span.old_end_row,
            column: span.old_end_column,
        },
        new_end_position: Point {
            row: new_end.0,
            column: new_end.1,
        },
    }
}

// The tests exercise real parsing against real grammars; with the stub they
// would assert that nothing does nothing.
#[cfg(test)]
#[cfg(feature = "syntax")]
mod tests;
