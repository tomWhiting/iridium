//! Tests for the web highlight cache.
//!
//! These exist because nothing else in this face can run. `wasm.rs` is 3,160
//! lines behind `all(feature = "web", target_arch = "wasm32")` with no test
//! module, and the only gate that compiles it is a `check` — which compiles
//! without running a single test. Extracting this module under
//! `any(target_arch = "wasm32", test)`, the gate `web_span_index` already
//! uses, is what makes the generation contract assertable rather than merely
//! commented.

use super::{JsHighlightSpan, WebHighlightCache};
use crate::web_span_index::WebSpan;

fn span(start: usize, end: usize) -> JsHighlightSpan {
    JsHighlightSpan {
        start,
        end,
        highlight_type: "keyword".to_string(),
    }
}

fn index_span(start: usize, end: usize) -> WebSpan {
    WebSpan {
        start,
        end,
        highlight_type: "keyword".to_string(),
    }
}

#[test]
fn a_new_cache_is_empty_inactive_and_at_generation_zero() {
    let cache = WebHighlightCache::new();

    assert!(cache.spans().is_empty());
    assert!(!cache.is_active());
    assert_eq!(cache.generation(), 0);
}

/// The contract the type exists to hold: every mutation moves the generation.
///
/// Asserted as strict inequality against the value captured immediately
/// before each call, rather than against literals. A test that says "after
/// three mutations the generation is 3" is testing the test's arithmetic; this
/// asks the only question a consumer asks — *did it change?*
#[test]
fn every_mutation_moves_the_generation() {
    let mut cache = WebHighlightCache::new();

    let before_set = cache.generation();
    cache.set(vec![span(0, 4)], vec![index_span(0, 4)]);
    assert_ne!(cache.generation(), before_set, "set must move it");

    let before_shift = cache.generation();
    cache.retain_shifted(|start, end| Some((start + 1, end + 1)));
    assert_ne!(cache.generation(), before_shift, "a shift must move it");

    let before_clear = cache.generation();
    cache.clear();
    assert_ne!(cache.generation(), before_clear, "clear must move it");
}

/// The one case that must *not* move it, and the reason it is separate from
/// the test above: a generation that moves for a no-op makes the compositor
/// re-shape for nothing, every frame an edit lands on an unhighlighted
/// document.
#[test]
fn shifting_an_empty_cache_leaves_the_generation_alone() {
    let mut cache = WebHighlightCache::new();
    let before = cache.generation();

    cache.retain_shifted(|start, end| Some((start + 1, end + 1)));

    assert_eq!(cache.generation(), before);
}

/// The latent defect this type removes.
///
/// `WebEditor::clear_tree_sitter_highlights` cleared the flat list and the
/// active flag and left the span index populated. It was never *reachable* —
/// every read of the index is guarded by the active flag — but that was a
/// property of two call sites both being careful, not of the data. A third
/// reader, or a path that set the flag without rebuilding, would have served
/// spans for a document state that no longer applied.
#[test]
fn clearing_empties_the_index_and_not_just_the_flat_list() {
    let mut cache = WebHighlightCache::new();
    cache.set(vec![span(0, 4)], vec![index_span(0, 4)]);
    assert!(cache.index().query(0, 100).next().is_some());

    cache.clear();

    assert!(cache.spans().is_empty());
    assert!(!cache.is_active());
    assert!(
        cache.index().query(0, 100).next().is_none(),
        "the index must not outlive the spans it indexes"
    );
}

/// Active is not "has spans". A document whose worker returned nothing and a
/// document whose worker has not answered resolve differently, and only this
/// flag separates them.
#[test]
fn setting_no_spans_still_marks_the_cache_active() {
    let mut cache = WebHighlightCache::new();

    cache.set(Vec::new(), Vec::new());

    assert!(cache.spans().is_empty());
    assert!(cache.is_active(), "the worker answered; it answered 'none'");
}

#[test]
fn a_shift_moves_surviving_spans_and_rebuilds_the_index() {
    let mut cache = WebHighlightCache::new();
    cache.set(
        vec![span(0, 4), span(10, 14)],
        vec![index_span(0, 4), index_span(10, 14)],
    );

    cache.retain_shifted(|start, end| Some((start + 5, end + 5)));

    assert_eq!(cache.spans(), &[span(5, 9), span(15, 19)]);
    let indexed: Vec<_> = cache.index().query(0, 100).collect();
    assert_eq!(
        indexed.len(),
        2,
        "the index must be rebuilt from the shifted spans, not the originals"
    );
    assert!(
        indexed.iter().all(|found| found.start >= 5),
        "a stale index would still hold the pre-shift span at 0..4: {indexed:?}"
    );
}

/// A shift that deletes every span leaves the cache active with nothing in
/// it — which is a different state from cleared, and the flag is what says so.
#[test]
fn a_shift_can_drop_every_span_without_deactivating_the_cache() {
    let mut cache = WebHighlightCache::new();
    cache.set(vec![span(0, 4)], vec![index_span(0, 4)]);

    cache.retain_shifted(|_, _| None);

    assert!(cache.spans().is_empty());
    assert!(cache.index().query(0, 100).next().is_none());
    assert!(
        cache.is_active(),
        "the spans were deleted by an edit, not by the worker withdrawing them"
    );
}
