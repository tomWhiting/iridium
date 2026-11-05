//! Parsing on a thread of its own, so a keystroke never waits for a parser.
//!
//! # Why the parse cannot stay on the keystroke
//!
//! The retained tree is reparsed incrementally — every edit reaches
//! `Tree::edit` before the reparse, and the reparse is handed the edited tree —
//! but an incremental reparse is still not proportional to the edit. Measured on
//! a 500,000-line Rust file of 25,000 functions, one typed character costs about
//! 23 ms of reparse and 25 ms of `changed_ranges`, both inside tree-sitter:
//!
//! - **The reparse.** Tree-sitter keeps a top-level `repeat` as a balanced tree
//!   of hidden nodes, which ought to let a reparse reuse half the file in one
//!   step. It cannot here: `tree-sitter-rust` reduces those nodes with a
//!   lookahead its parse table has more than one action for, and tree-sitter
//!   marks every node built under such a reduction *fragile* and never reuses
//!   it. Reuse falls back to one function at a time, so the reparse shifts
//!   all 25,000 of them and rebuilds the spine above them.
//! - **The changed ranges.** The rebuilt spine is entirely new nodes, so the
//!   side-by-side walk `changed_ranges` makes cannot skip it, and it too visits
//!   every function.
//!
//! Neither is ours to change — one is the grammar, the other the library — and
//! an edit that really does restructure the file, such as an unclosed brace or
//! quote, costs a reparse of everything after it whatever the grammar. So the
//! parse moves here, and the keystroke keeps only what is proportional to the
//! edit.
//!
//! # What the keystroke still does
//!
//! It shifts the tree it has by the edit (`Tree::edit`, O(depth)) and the fold
//! regions with it, and the frame draws from that shifted tree straight away.
//! Every span drawn is at the right offset, because the tree they come from was
//! moved by exactly the edit that moved the text. What a shifted tree cannot
//! know is structure the edit created — a new `{`, a string that now runs on —
//! and that arrives when the parse lands: within a frame on an ordinary file,
//! and on the 500,000-line one above after the ~50 ms the reparse and its
//! changed ranges take, during which the frames keep coming.
//!
//! # The protocol
//!
//! One job is in flight at a time. A job carries the text (a rope clone,
//! which shares rather than copies), the tree as shifted up to that text, and
//! the edits made since the previous job. The worker reparses, brings its own
//! fold cache up to date with the new tree, and sends both back. Edits made
//! while the job ran are replayed onto the tree it returns — the same
//! `Tree::edit` the keystroke applied to the old one — and if there were any,
//! the next job goes out at once. Typing faster than the parser therefore
//! coalesces: the worker parses the latest text, not every intermediate one.
//!
//! A *generation* names the document-and-language a job belongs to. Replacing
//! the content or changing the language moves it on; anything still in flight
//! for the old generation is abandoned — the parse is cancelled through its
//! progress check — and anything that lands for it anyway is thrown away.
//!
//! # Nothing large is freed on the editing thread
//!
//! A tree that a parse rebuilt from scratch is millions of nodes, and freeing
//! it is a walk of all of them: 115 ms for the 500,000-line file. Every tree
//! the editing thread lets go of is therefore sent here to be dropped, so that
//! walk happens off the keystroke too. It is not entirely free even here: the
//! allocator is shared, and while millions of nodes go back to it the editing
//! thread's own allocations can wait — measured as one frame of about 20 ms
//! just after such a file is closed, where dropping it in place stalled for
//! the whole 115.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use iridium_lang::Language;
use iridium_syntax::{FoldCache, InputEdit, ParseOutcome, SyntaxParser, Tree, changed_ranges};
use ropey::Rope;

use super::state::{SyntaxWake, rope_reader};
use crate::document::Document;

/// What the editing thread sends the worker.
enum Request {
    /// Parse this.
    Parse(Job),
    /// Drop this tree here rather than there.
    Dispose(Tree),
    /// Call this after each landing from now on.
    Wake(Option<SyntaxWake>),
}

/// One parse, and everything the worker needs to describe its result.
struct Job {
    /// The generation the job belongs to.
    generation: u64,
    /// The language to parse with.
    language: Language,
    /// The text to parse.
    text: Rope,
    /// The tree to build on, already shifted to `text`; `None` for a parse
    /// from scratch.
    old: Option<Tree>,
    /// The edits applied to the previous job's tree to produce `old`, in
    /// order. The worker's fold cache follows them. Empty for a parse from
    /// scratch.
    edits: Vec<InputEdit>,
}

/// A finished job.
pub(super) struct Landed {
    /// The generation of the job.
    generation: u64,
    /// What the job produced, or `None` if tree-sitter produced no tree.
    parsed: Option<Parsed>,
}

impl Landed {
    /// The new tree, its fold regions, and whether it was parsed from scratch
    /// — or `None` if tree-sitter produced no tree.
    ///
    /// The tree is in the coordinates of the text the job carried: edits made
    /// since are the caller's to replay onto it.
    pub(super) fn unpack(self) -> Option<(Tree, LandedFolds, bool)> {
        self.parsed
            .map(|parsed| (parsed.tree, LandedFolds(parsed.folds), parsed.from_scratch))
    }
}

/// The products of a successful parse.
struct Parsed {
    /// The new tree, in the coordinates of the text the job carried.
    tree: Tree,
    /// The fold regions of `tree`, maintained by the worker.
    folds: FoldCache,
    /// Whether the tree was parsed from scratch rather than incrementally.
    from_scratch: bool,
}

/// Fold regions computed by the background parser, for the editing thread's
/// fold state to take over whole.
///
/// The worker keeps its own fold cache in step with the trees it parses, so a
/// parse that restructured half the file is re-walked there and not on the
/// keystroke. These are the regions of the tree the parse produced, *before*
/// any edits made while it ran; the syntax state's pending delta then carries
/// exactly those edits.
#[derive(Debug)]
pub struct LandedFolds(FoldCache);

impl LandedFolds {
    /// Hands the regions to `fold_state`, returning whether it took them and,
    /// if it did, whether its published regions changed.
    ///
    /// `None` means the fold state is set up for a different language and kept
    /// what it had; the caller must then treat the tree as wholly new.
    pub fn adopt_into(self, fold_state: &mut crate::editor::FoldState) -> Option<bool> {
        fold_state.adopt_cache(self.0)
    }
}

/// Why the worker can no longer be reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Gone;

/// The editing thread's handle on the background parser.
pub(super) struct Background {
    /// Requests to the worker.
    requests: Sender<Request>,
    /// Finished jobs from the worker.
    landed: Receiver<Landed>,
    /// The generation the worker should still be working for, read by its
    /// progress check so an abandoned parse stops rather than finishes.
    wanted: Arc<AtomicU64>,
    /// The current generation.
    generation: u64,
    /// Whether a job of the current generation is out.
    in_flight: bool,
    /// Whether the next job must parse from scratch.
    from_scratch_owed: bool,
    /// Edits applied to the tree since the last job went out, in order.
    edits: Vec<InputEdit>,
}

impl std::fmt::Debug for Background {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Background")
            .field("generation", &self.generation)
            .field("in_flight", &self.in_flight)
            .field("from_scratch_owed", &self.from_scratch_owed)
            .field("edits", &self.edits.len())
            .finish_non_exhaustive()
    }
}

impl Background {
    /// Starts the worker thread, or returns `None` if the system will not
    /// give one — the caller then keeps parsing inline, which is slower and
    /// still right.
    ///
    /// `current` says the caller's tree already matches its document, so the
    /// first job can build on it rather than parse from scratch.
    pub(super) fn spawn(wake: Option<SyntaxWake>, current: bool) -> Option<Self> {
        let (requests, inbox) = mpsc::channel();
        let (outbox, landed) = mpsc::channel();
        let wanted = Arc::new(AtomicU64::new(0));
        let worker_wanted = Arc::clone(&wanted);
        thread::Builder::new()
            .name("iridium-syntax".to_owned())
            .spawn(move || work(&inbox, &outbox, &worker_wanted, wake))
            .ok()?;
        Some(Self {
            requests,
            landed,
            wanted,
            generation: 0,
            in_flight: false,
            from_scratch_owed: !current,
            edits: Vec::new(),
        })
    }

    /// Replaces the function called after each landing.
    pub(super) fn set_wake(&self, wake: Option<SyntaxWake>) -> Result<(), Gone> {
        self.requests.send(Request::Wake(wake)).map_err(|_| Gone)
    }

    /// Abandons everything in flight and owes a parse from scratch.
    pub(super) fn restart(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.wanted.store(self.generation, Ordering::Release);
        self.in_flight = false;
        self.from_scratch_owed = true;
        self.edits.clear();
    }

    /// Records an edit the editing thread has applied to its tree.
    pub(super) fn note_edit(&mut self, edit: InputEdit) {
        self.edits.push(edit);
    }

    /// The edits applied since the last job went out.
    pub(super) fn edits(&self) -> &[InputEdit] {
        &self.edits
    }

    /// Whether the tree is behind the text: a job is out, or one is owed.
    pub(super) fn behind(&self) -> bool {
        self.in_flight || self.from_scratch_owed || !self.edits.is_empty()
    }

    /// Whether a job is out.
    pub(super) const fn in_flight(&self) -> bool {
        self.in_flight
    }

    /// Sends `tree` to be dropped on the worker.
    ///
    /// If the worker is gone the tree is dropped here instead: slower, and
    /// nothing else could be done with it.
    pub(super) fn dispose(&self, tree: Tree) {
        if let Err(unsent) = self.requests.send(Request::Dispose(tree)) {
            drop(unsent);
        }
    }

    /// Sends the next job if one is owed and none is out.
    ///
    /// `tree` is the editing thread's tree, shifted to `document`; it is
    /// copied (a reference-count bump, not a copy of the nodes) for the worker
    /// to build on.
    pub(super) fn dispatch(
        &mut self,
        language: Language,
        document: &Document,
        tree: Option<&Tree>,
    ) -> Result<(), Gone> {
        if self.in_flight || (!self.from_scratch_owed && self.edits.is_empty()) {
            return Ok(());
        }
        let from_scratch = self.from_scratch_owed || tree.is_none();
        let job = Job {
            generation: self.generation,
            language,
            text: document.rope().clone(),
            old: if from_scratch { None } else { tree.cloned() },
            edits: std::mem::take(&mut self.edits),
        };
        self.requests.send(Request::Parse(job)).map_err(|_| Gone)?;
        self.in_flight = true;
        self.from_scratch_owed = false;
        Ok(())
    }

    /// The finished job of the current generation, if it has landed.
    ///
    /// Anything that lands for an older generation is disposed of here and
    /// never surfaces.
    pub(super) fn try_landed(&mut self) -> Result<Option<Landed>, Gone> {
        loop {
            match self.landed.try_recv() {
                Ok(landed) => {
                    if let Some(current) = self.accept(landed) {
                        return Ok(Some(current));
                    }
                },
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => return Err(Gone),
            }
        }
    }

    /// Waits for the job in flight to land.
    pub(super) fn wait_landed(&mut self) -> Result<Landed, Gone> {
        loop {
            let landed = self.landed.recv().map_err(|_| Gone)?;
            if let Some(current) = self.accept(landed) {
                return Ok(current);
            }
        }
    }

    /// Keeps `landed` if it belongs to the job in flight.
    fn accept(&mut self, landed: Landed) -> Option<Landed> {
        if landed.generation == self.generation && self.in_flight {
            self.in_flight = false;
            return Some(landed);
        }
        if let Some(parsed) = landed.parsed {
            self.dispose(parsed.tree);
        }
        None
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        // Abandon whatever is running. Dropping `requests` then ends the
        // worker's loop once it has drained what was already sent — the
        // disposals included — so nothing here waits for it.
        self.wanted.store(u64::MAX, Ordering::Release);
    }
}

/// The worker's loop: parse, fold, send, until the editing thread hangs up.
fn work(
    inbox: &Receiver<Request>,
    outbox: &Sender<Landed>,
    wanted: &AtomicU64,
    mut wake: Option<SyntaxWake>,
) {
    let mut parser: Option<SyntaxParser> = None;
    // The fold cache, with the generation whose trees it has followed.
    let mut folds: Option<(u64, FoldCache)> = None;

    while let Ok(request) = inbox.recv() {
        let job = match request {
            Request::Parse(job) => job,
            Request::Dispose(tree) => {
                drop(tree);
                continue;
            },
            Request::Wake(next) => {
                wake = next;
                continue;
            },
        };
        let generation = job.generation;
        if wanted.load(Ordering::Acquire) != generation {
            // Abandoned before it started; the editing thread has stopped
            // waiting for it.
            continue;
        }

        let reusable = parser.take().filter(|p| p.language() == job.language);
        let Some(mut active) = reusable.or_else(|| SyntaxParser::new(job.language).ok()) else {
            if outbox
                .send(Landed {
                    generation,
                    parsed: None,
                })
                .is_err()
            {
                return;
            }
            notify(wake.as_ref());
            continue;
        };

        let outcome = active.parse(rope_reader(&job.text), job.old.as_ref(), || {
            wanted.load(Ordering::Acquire) == generation
        });
        parser = Some(active);

        let landed = match outcome {
            ParseOutcome::Parsed(tree) => {
                let cache = follow_folds(&mut folds, &job, &tree);
                Landed {
                    generation,
                    parsed: Some(Parsed {
                        tree,
                        folds: cache,
                        from_scratch: job.old.is_none(),
                    }),
                }
            },
            // Asked to stop: the editing thread already moved on, and the fold
            // cache never saw this tree, so the next job — from scratch, since
            // a new generation always starts that way — rebuilds it.
            ParseOutcome::Cancelled => continue,
            ParseOutcome::Failed => Landed {
                generation,
                parsed: None,
            },
        };
        // The old tree is dropped here, on this thread, before the result is
        // even sent: it is the editing thread's copy that must not be the last.
        drop(job);
        if outbox.send(landed).is_err() {
            return;
        }
        notify(wake.as_ref());
    }
}

/// Brings the worker's fold cache up to date with `tree` and returns a copy for
/// the editing thread.
///
/// A parse from scratch, a new language or a new generation all rebuild: the
/// cache's regions describe some other tree. Otherwise the cache follows the
/// job's edits and the ranges the reparse restructured, which is the whole of
/// what moved since the cache last saw a tree of this generation.
fn follow_folds(folds: &mut Option<(u64, FoldCache)>, job: &Job, tree: &Tree) -> FoldCache {
    let continuing = folds.as_ref().is_some_and(|(generation, cache)| {
        *generation == job.generation && cache.language() == job.language
    });
    match (folds.as_mut(), job.old.as_ref()) {
        (Some((_, cache)), Some(old)) if continuing => {
            let changed: Vec<Range<usize>> = changed_ranges(old, tree);
            cache.apply(tree, &job.edits, &changed);
            cache.clone()
        },
        _ => {
            let mut cache = FoldCache::new(job.language);
            cache.rebuild(tree);
            *folds = Some((job.generation, cache.clone()));
            cache
        },
    }
}

/// Tells the face something landed.
fn notify(wake: Option<&SyntaxWake>) {
    if let Some(wake) = wake {
        wake();
    }
}
