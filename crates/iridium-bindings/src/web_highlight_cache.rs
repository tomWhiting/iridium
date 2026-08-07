//! The web face's per-document highlight cache.
//!
//! Tree-sitter runs in a JavaScript worker for the browser face, so the spans
//! arrive over the boundary rather than from the kernel. This owns them, the
//! query index built from them, and the generation counter the compositor
//! keys its retained shapes on.
//!
//! # Why these four live together
//!
//! They were four separate fields on `WebEditor`, and the rule binding them —
//! *changing the spans moves the generation* — was written out three times in
//! prose, once beside each mutation. Three careful restatements of one rule is
//! a rule the type system should carry: nothing stopped a fourth mutation path
//! from forgetting, and a consumer keyed on the generation would then serve a
//! stale colour resolution for content that had moved.
//!
//! Every mutation here is a method, and every method bumps the generation. A
//! caller cannot get it wrong because a caller is not asked to get it right.

use crate::web_span_index::{WebSpan, WebSpanIndex};

/// A highlight span from tree-sitter, as delivered by the JavaScript worker.
///
/// Plain Rust with no binding surface — it crosses the JS boundary by being
/// built from a `JsValue` elsewhere, not by being exported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsHighlightSpan {
    /// Start byte offset
    pub start: usize,
    /// End byte offset
    pub end: usize,
    /// Highlight type as string (e.g., "keyword", "string", "comment")
    pub highlight_type: String,
}

/// Worker-delivered highlight spans, their query index, and the generation
/// the compositor keys on.
#[derive(Debug, Default)]
pub struct WebHighlightCache {
    /// Flat span list. The fallback resolution path reads this directly when
    /// the index is empty.
    spans: Vec<JsHighlightSpan>,
    /// The same spans as an interval index, for O(log n + k) viewport queries.
    index: WebSpanIndex,
    /// Whether worker highlights are in use at all.
    ///
    /// Distinct from "the spans are empty": a document genuinely without
    /// highlights and a document whose worker has not answered yet resolve
    /// differently, and only this tells them apart.
    active: bool,
    /// Bumped by every mutation. The compositor's retained shapes recolour
    /// when it moves, so it must move whenever a resolution would answer
    /// differently for identical content.
    generation: u64,
}

impl WebHighlightCache {
    /// An empty cache: no spans, not active, generation zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces every span with `spans` and marks the cache active.
    ///
    /// `index_spans` is the same set already converted for the index; the
    /// caller builds it while parsing the incoming `JsValue`, so it is passed
    /// rather than recomputed.
    pub fn set(&mut self, spans: Vec<JsHighlightSpan>, index_spans: Vec<WebSpan>) {
        self.spans = spans;
        self.index = WebSpanIndex::new(index_spans);
        self.active = true;
        self.bump();
    }

    /// Drops every span and falls back to the non-worker resolution paths.
    ///
    /// The index is cleared alongside the flat list. It did not used to be:
    /// `clear_tree_sitter_highlights` reset the list and the flag and left the
    /// index populated with spans for a document state that no longer applied.
    /// That was unreachable rather than harmless — every read of the index is
    /// guarded by the active flag — but "unreachable" was a property of the
    /// two call sites, not of the data, and it is the exact shape this type
    /// exists to remove.
    pub fn clear(&mut self) {
        self.spans.clear();
        self.index = WebSpanIndex::empty();
        self.active = false;
        self.bump();
    }

    /// Applies `shift` to every span, dropping those it deletes, and rebuilds
    /// the index in one pass.
    ///
    /// Returns without touching the generation when there is nothing to
    /// shift: no spans means no resolution changed, and a generation that
    /// moves for a no-op forces the compositor to re-shape for nothing.
    pub fn retain_shifted<F>(&mut self, mut shift: F)
    where
        F: FnMut(usize, usize) -> Option<(usize, usize)>,
    {
        if self.spans.is_empty() {
            return;
        }
        self.spans
            .retain_mut(|span| match shift(span.start, span.end) {
                Some((start, end)) => {
                    span.start = start;
                    span.end = end;
                    true
                },
                None => false,
            });
        self.index = WebSpanIndex::new(
            self.spans
                .iter()
                .map(|span| WebSpan {
                    start: span.start,
                    end: span.end,
                    highlight_type: span.highlight_type.clone(),
                })
                .collect(),
        );
        self.bump();
    }

    /// The flat span list, for the fallback resolution path.
    #[must_use]
    pub fn spans(&self) -> &[JsHighlightSpan] {
        &self.spans
    }

    /// The query index, for the viewport resolution path.
    #[must_use]
    pub const fn index(&self) -> &WebSpanIndex {
        &self.index
    }

    /// Whether worker highlights are in use.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    /// The current generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Wrapping so a long-lived session cannot panic on overflow. Consumers
    /// compare for inequality rather than ordering, so a wrap costs at worst
    /// one missed recolour after 2^64 mutations.
    const fn bump(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests;
