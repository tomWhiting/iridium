//! The desktop face's side of the compositor's highlight seam: the kernel's
//! tree-sitter spans, cached per parse generation, resolved per frame.
//!
//! Nothing here parses. The kernel keeps one parse tree for the document and
//! refreshes it after every content change (`EditorState::refresh_syntax`,
//! which every mutating command runs); [`HighlightCache`] reads that tree,
//! turns it into an indexed span set once per generation, and
//! [`FrameHighlights`] resolves those spans onto one frame's visible content.
//! This is the terminal face's design (`iridium-tui/src/frame/highlight.rs`)
//! carried to this face's seam: the same [`Highlighter`], the same
//! [`SpanIndex`], the same generation check — only the last step differs,
//! because a GPU frame wants coloured text runs where a cell frame wants
//! per-cluster styles.
//!
//! # Colours are the kernel's
//!
//! Each span's [`HighlightType`](iridium_editor::syntax::HighlightType) is
//! mapped through the kernel's own [`highlight_to_color`] against the theme's
//! `SyntaxColors` — the mapping the terminal face paints with. The
//! [`HighlightContext`]'s capture-name string map is deliberately not
//! consulted: it exists for the web face, whose spans arrive from JavaScript
//! as capture-name strings; this face's spans are the kernel's typed spans,
//! and a string detour would add a lookup that can miss to a mapping that
//! cannot.
//!
//! # What a frame's resolution promises
//!
//! The runs cover the visible content exactly — gaps take the theme
//! foreground — and are flat: where spans nest, the first span to claim a
//! byte keeps it (at equal starts the innermost, i.e. shortest, wins, which
//! is the ordering [`HighlightSpan`]'s own `Ord` produces). Span offsets are
//! document-absolute while the content is the compositor's fold-collapsed
//! extraction, so offsets past a collapsed fold can drift by the placeholder
//! text's length; they are clamped and snapped to character boundaries, never
//! trusted — the same honesty clause the web face's resolver carries.

use iridium_editor::Editor;
use iridium_editor::render::{HighlightContext, HighlightSource};
use iridium_editor::span_index::SpanIndex;
use iridium_editor::syntax::{HighlightSpan, Highlighter, Language, highlight_to_color};
use iridium_editor::theme::{Color, SyntaxColors};

/// The cached parse-derived state frames are resolved from.
///
/// One per session, owned by the app beside the editor. [`Self::refresh`]
/// runs before every frame and costs a generation comparison when nothing
/// changed; the span rebuild happens only when the kernel actually reparsed.
#[derive(Debug, Default)]
pub struct HighlightCache {
    /// The spans and the provenance they were produced under, when a language
    /// with a working highlighter is set.
    entry: Option<Spans>,
    /// How many times the spans have been rebuilt — the observable that
    /// proves the generation gate: a frame without an edit must not move it.
    rebuilds: u64,
    /// The resolver generation handed to the compositor's retained-shaping
    /// key: moves whenever a subsequent resolution could answer differently.
    ///
    /// `rebuilds` alone is not enough — removing the language clears the
    /// spans *without* a rebuild, and the next resolution answers `None`
    /// where it answered runs. So this moves on every span rebuild *and*
    /// on every path that empties the entry while it held spans.
    generation: u64,
}

/// One generation's worth of spans.
#[derive(Debug)]
struct Spans {
    /// The language the spans were produced for.
    language: Language,
    /// The rules, borrowed from the process-wide query cache.
    highlighter: Highlighter,
    /// The document's spans, indexed for viewport queries.
    index: SpanIndex,
    /// The tree and document state the spans were produced from.
    generation: Generation,
}

/// How current a set of highlight spans is.
///
/// The parse count moves whenever the kernel reparses, so it is what actually
/// invalidates the cache in every path measured so far. The revision is
/// carried as defence in depth — a mutation reaching the document without
/// reaching the tree must not leave last parse's colours on screen — exactly
/// as the terminal face's cache carries it, with the same caveat recorded
/// there: no test discriminates on the revision half alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Generation {
    /// Full plus incremental parses the kernel has run.
    parses: u64,
    /// The document revision.
    revision: u64,
}

impl HighlightCache {
    /// An empty cache: no language, no spans, resolvers that answer `None`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entry: None,
            rebuilds: 0,
            generation: 0,
        }
    }

    /// Brings the cache up to date with the kernel's tree, reusing everything
    /// it can.
    ///
    /// With no language set the cache empties — the compositor's built-in
    /// fallback takes over. A language whose bundled highlight query does not
    /// compile does the same: unhighlighted text is a degraded editor, a
    /// missing frame is no editor at all. Otherwise the spans are rebuilt
    /// only when the parse count or document revision moved, and the compiled
    /// highlighter survives every rebuild for the same language.
    pub fn refresh(&mut self, editor: &Editor) {
        let had_entry = self.entry.is_some();
        let state = editor.state();
        let Some(language) = state.syntax.language() else {
            self.entry = None;
            if had_entry {
                // The spans are gone without a rebuild — the next resolution
                // answers `None` where it answered runs, and the exposed
                // generation must say so (the `rebuilds`-is-not-enough path).
                self.generation = self.generation.wrapping_add(1);
            }
            return;
        };
        let generation = Generation {
            parses: state.syntax.full_parses() + state.syntax.incremental_parses(),
            revision: state.document.revision(),
        };

        let reusable = match self.entry.take() {
            Some(existing) if existing.language == language => {
                if existing.generation == generation {
                    self.entry = Some(existing);
                    return;
                }
                Some(existing.highlighter)
            },
            _ => None,
        };
        let Some(highlighter) = reusable.or_else(|| Highlighter::try_new(language)) else {
            // The entry was taken above and stays empty: a language whose
            // highlight query does not compile degrades to the fallback. If
            // spans were on screen, that is a change of answer too.
            if had_entry {
                self.generation = self.generation.wrapping_add(1);
            }
            return;
        };

        let index = state.syntax.tree().map_or_else(SpanIndex::empty, |tree| {
            SpanIndex::new(highlighter.spans_in(tree, &state.document.text()))
        });
        self.rebuilds = self.rebuilds.saturating_add(1);
        self.generation = self.generation.wrapping_add(1);
        self.entry = Some(Spans {
            language,
            highlighter,
            index,
            generation,
        });
    }

    /// How many times the spans have been rebuilt since the cache was
    /// created.
    ///
    /// Exists so the generation gate is testable from outside: refreshing
    /// without an intervening edit must leave this unchanged.
    #[must_use]
    pub const fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// The generation the per-frame resolver reports to the compositor —
    /// see the field documentation for what moves it.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// One frame's resolver over the cached spans, coloured from `colors`.
    ///
    /// Borrows the cache immutably, so it can be handed to
    /// [`FrameCompositor::compose`](iridium_editor::render::FrameCompositor)
    /// alongside mutable borrows of the surface and compositor.
    ///
    /// The resolver's generation is this cache's. `colors` is a second
    /// input to the resolution that the generation does not cover: today
    /// the desktop face sets its theme once at startup and never again, so
    /// the colours cannot change under a retained frame — if runtime theme
    /// switching ever lands, the switch must move this generation too, or
    /// retained frames keep the old palette.
    #[must_use]
    pub fn resolver<'a>(&'a self, colors: &'a SyntaxColors) -> FrameHighlights<'a> {
        FrameHighlights {
            index: self.entry.as_ref().map(|entry| &entry.index),
            colors,
            generation: self.generation,
        }
    }
}

/// The per-frame [`HighlightSource`]: the cached span index and the theme's
/// syntax colours, borrowed for exactly one `compose` call.
#[derive(Debug)]
pub struct FrameHighlights<'a> {
    /// The indexed spans, absent when no language (or no working highlighter)
    /// is set.
    index: Option<&'a SpanIndex>,
    /// The theme's syntax colours, mapped per highlight by the kernel.
    colors: &'a SyntaxColors,
    /// The owning [`HighlightCache`]'s generation at construction.
    generation: u64,
}

impl HighlightSource for FrameHighlights<'_> {
    fn resolve<'a>(&mut self, context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>> {
        let index = self.index?;
        Some(rich_spans(index, context, self.colors))
    }

    fn generation(&self) -> u64 {
        self.generation
    }
}

/// Resolves the indexed spans against one frame's visible content: coloured
/// runs in order, gaps filled with the theme foreground, the whole content
/// covered.
fn rich_spans<'a>(
    index: &SpanIndex,
    context: &HighlightContext<'a>,
    colors: &SyntaxColors,
) -> Vec<(&'a str, Color)> {
    let visible = context.content;
    let start_byte = context.content_start_byte;
    let end_byte = start_byte.saturating_add(visible.len());

    let mut spans: Vec<HighlightSpan> = index.query(start_byte, end_byte).collect();
    // `HighlightSpan`'s order: start ascending, then end ascending — so at
    // equal starts the innermost span comes first and wins the flat run.
    spans.sort_unstable();

    let mut runs = Vec::with_capacity(spans.len().saturating_mul(2).saturating_add(1));
    let mut last_end = 0_usize;
    for span in spans {
        let span_start = snap_down(visible, span.start.saturating_sub(start_byte));
        let span_end = snap_down(visible, span.end.saturating_sub(start_byte));
        // Flat runs: a byte already claimed stays claimed, and a span that
        // clamping or fold drift emptied contributes nothing.
        if span_start < last_end || span_start >= span_end {
            continue;
        }
        if span_start > last_end {
            runs.push((&visible[last_end..span_start], context.foreground));
        }
        runs.push((
            &visible[span_start..span_end],
            highlight_to_color(span.highlight, colors),
        ));
        last_end = span_end;
    }
    if last_end < visible.len() {
        runs.push((&visible[last_end..], context.foreground));
    }
    runs
}

/// Clamps `index` into `content` and moves it down to the nearest character
/// boundary.
///
/// Span offsets are document bytes; with folds collapsed the content past a
/// placeholder no longer lines up with them, so an offset landing inside a
/// multi-byte character is possible and must slice nothing rather than panic.
fn snap_down(content: &str, index: usize) -> usize {
    let mut index = index.min(content.len());
    while index > 0 && !content.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use iridium_editor::render::{HighlightContext, HighlightSource, ViewportConfig};
    use iridium_editor::theme::{Color, Theme};
    use iridium_editor::{Editor, Language};

    use super::HighlightCache;

    /// A context over `content` as an unfolded document starting at byte 0,
    /// which is what the compositor hands the resolver for a short unscrolled
    /// document.
    fn context_over<'a>(
        content: &'a str,
        syntax_theme: &'a HashMap<String, Color>,
        viewport: &'a ViewportConfig,
        foreground: Color,
    ) -> HighlightContext<'a> {
        HighlightContext {
            content,
            content_start_byte: 0,
            content_width: 800.0,
            line_height: 20.0,
            viewport_config: viewport,
            syntax_theme,
            foreground,
        }
    }

    /// An editor holding `source` as a Rust document, parsed.
    fn rust_editor(source: &str) -> Editor {
        let mut editor = Editor::with_defaults();
        editor.set_content(source);
        editor.set_language(Language::Rust);
        editor
    }

    #[test]
    fn rust_source_resolves_to_keyword_and_string_coloured_runs() {
        let source = "fn main() { let answer = \"forty-two\"; }";
        let editor = rust_editor(source);
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 1, "the first refresh builds the spans");

        let theme = Theme::default();
        let syntax_theme = HashMap::new();
        let viewport = ViewportConfig::default();
        let foreground = theme.editor.foreground;
        let context = context_over(source, &syntax_theme, &viewport, foreground);
        let runs = cache
            .resolver(&theme.syntax)
            .resolve(&context)
            .expect("a parsed Rust document resolves spans");

        let rebuilt: String = runs.iter().map(|(text, _)| *text).collect();
        assert_eq!(rebuilt, source, "the runs cover the content exactly");

        let colour_of = |needle: &str| {
            runs.iter()
                .find(|(text, _)| text.contains(needle))
                .map(|(_, colour)| *colour)
        };
        assert_eq!(
            colour_of("fn"),
            Some(theme.syntax.keyword),
            "`fn` paints as a keyword"
        );
        assert_eq!(
            colour_of("forty-two"),
            Some(theme.syntax.string),
            "the literal paints as a string"
        );
        assert!(
            runs.iter().any(|(_, colour)| *colour == foreground),
            "gaps between captures take the foreground"
        );
    }

    #[test]
    fn refreshing_without_an_edit_rebuilds_nothing() {
        let editor = rust_editor("fn main() {}\n");
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);
        cache.refresh(&editor);
        cache.refresh(&editor);
        assert_eq!(
            cache.rebuilds(),
            1,
            "an unchanged document is a generation comparison, not a rebuild"
        );
    }

    #[test]
    fn an_edit_moves_the_generation_and_rebuilds_once() {
        let mut editor = rust_editor("fn main() {}\n");
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 1);

        editor.paste("// note\n");
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 2, "the edit's reparse invalidates once");
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 2, "and only once");
    }

    #[test]
    fn generation_is_stable_across_no_op_refreshes_and_moves_on_edits() {
        let mut editor = rust_editor("fn main() {}\n");
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);
        let after_build = cache.generation();

        cache.refresh(&editor);
        cache.refresh(&editor);
        assert_eq!(
            cache.generation(),
            after_build,
            "an unchanged document must not move the resolver generation"
        );

        editor.paste("// note\n");
        cache.refresh(&editor);
        assert_ne!(
            cache.generation(),
            after_build,
            "a rebuild is a new answer and must move the generation"
        );

        let theme = Theme::default();
        assert_eq!(
            cache.resolver(&theme.syntax).generation(),
            cache.generation(),
            "the per-frame resolver reports the cache's generation"
        );
    }

    /// The `rebuilds`-is-not-enough path: removing the language clears the
    /// spans without a rebuild, and the next resolution answers `None`
    /// where it answered runs — a retained frame keyed on the generation
    /// must notice.
    #[test]
    fn removing_the_language_moves_the_generation_without_a_rebuild() {
        let mut editor = rust_editor("fn main() {}\n");
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 1);
        let with_spans = cache.generation();

        editor.state_mut().syntax.clear_language();
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 1, "clearing the entry is not a rebuild");
        assert_ne!(
            cache.generation(),
            with_spans,
            "the cleared spans change the resolution answer"
        );

        let theme = Theme::default();
        let syntax_theme = HashMap::new();
        let viewport = ViewportConfig::default();
        let context = context_over(
            "fn main() {}\n",
            &syntax_theme,
            &viewport,
            theme.editor.foreground,
        );
        assert!(
            cache.resolver(&theme.syntax).resolve(&context).is_none(),
            "and the answer really did change: the fallback takes over"
        );

        let cleared = cache.generation();
        cache.refresh(&editor);
        assert_eq!(
            cache.generation(),
            cleared,
            "staying without a language moves nothing further"
        );
    }

    #[test]
    fn without_a_language_the_resolver_defers_to_the_fallback() {
        let mut editor = Editor::with_defaults();
        editor.set_content("plain text, no grammar\n");
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);

        let theme = Theme::default();
        let syntax_theme = HashMap::new();
        let viewport = ViewportConfig::default();
        let context = context_over(
            "plain text, no grammar\n",
            &syntax_theme,
            &viewport,
            theme.editor.foreground,
        );
        assert!(
            cache.resolver(&theme.syntax).resolve(&context).is_none(),
            "no language means no answer, which selects the compositor's fallback"
        );
        assert_eq!(cache.rebuilds(), 0);
    }

    #[test]
    fn multibyte_content_is_covered_without_splitting_characters() {
        let source = "fn greet() { let s = \"héllo 🦀 ẑ\"; }";
        let editor = rust_editor(source);
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);

        let theme = Theme::default();
        let syntax_theme = HashMap::new();
        let viewport = ViewportConfig::default();
        let context = context_over(source, &syntax_theme, &viewport, theme.editor.foreground);
        let runs = cache
            .resolver(&theme.syntax)
            .resolve(&context)
            .expect("the document resolves");
        let rebuilt: String = runs.iter().map(|(text, _)| *text).collect();
        assert_eq!(rebuilt, source, "every byte is covered, on char boundaries");
    }

    #[test]
    fn a_span_past_the_visible_content_is_clamped_not_panicked() {
        // The visible content is a prefix of the document: spans whose ranges
        // run past it must clamp to what is on screen.
        let source = "fn main() { let answer = \"forty-two\"; }";
        let editor = rust_editor(source);
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);

        let theme = Theme::default();
        let syntax_theme = HashMap::new();
        let viewport = ViewportConfig::default();
        let visible = &source[..14];
        let context = context_over(visible, &syntax_theme, &viewport, theme.editor.foreground);
        let runs = cache
            .resolver(&theme.syntax)
            .resolve(&context)
            .expect("the document resolves");
        let rebuilt: String = runs.iter().map(|(text, _)| *text).collect();
        assert_eq!(rebuilt, visible, "the runs stop where the content stops");
    }

    #[test]
    fn snap_down_lands_on_character_boundaries() {
        let text = "a🦀b";
        assert_eq!(super::snap_down(text, 0), 0);
        assert_eq!(super::snap_down(text, 2), 1, "inside the crab snaps back");
        assert_eq!(super::snap_down(text, 5), 5);
        assert_eq!(
            super::snap_down(text, 99),
            text.len(),
            "past the end clamps"
        );
    }
}
