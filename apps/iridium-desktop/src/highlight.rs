//! The desktop face's side of the compositor's highlight seam: the kernel's
//! tree-sitter spans, cached per parse generation, resolved per frame.
//!
//! Nothing here parses. The kernel keeps one parse tree for the document and
//! refreshes it after every content change (`EditorState::refresh_syntax`,
//! which every mutating command runs); [`HighlightCache`] reads that tree,
//! turns it into an indexed span set once per generation, and
//! [`FrameHighlights`] resolves those spans onto one frame's visible content.
//!
//! # The spans cover a window, not the document
//!
//! Deriving every span of a 10k-line file on every parse generation was the
//! parser tax (docs/design/PARSER-TAX-MAP.md): ~30ms of each keystroke spent
//! walking the whole tree for a viewport that paints ~90 lines. The windowed
//! derive that fixed it — viewport widened by [`OVERSCAN_LINES`] each way,
//! rebuild only when the parse generation moves *or* the requested viewport
//! escapes the covered window — is the kernel's [`WindowedSpanCache`],
//! shared with the terminal face (the parser-tax map's R2 ruling);
//! [`HighlightCache`] is this face's thin handle on it. Spans straddling
//! the window's edges arrive whole from the kernel
//! ([`Highlighter::spans_in_range`](iridium_editor::syntax::Highlighter::spans_in_range)'s
//! boundary contract) and the resolver clamps them to visible content
//! exactly as it always has.
//!
//! Only the resolution step is this face's own — a GPU frame wants coloured
//! text runs where the terminal's cell frame wants per-cluster styles — so
//! [`FrameHighlights`] is what this module keeps beside the handle.
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
//! foreground — and are flat: where spans nest, **the innermost span owns the
//! byte** and the span around it keeps everything the inner one does not take.
//! That collapse is the kernel's
//! [`flatten_spans`](iridium_editor::render::flatten_spans), not this face's,
//! because the web face needs the identical rule and had the identical bug.
//!
//! Span offsets are document-absolute while the content is the compositor's
//! fold-collapsed extraction, so offsets past a collapsed fold can drift by the
//! placeholder text's length; they are clamped and snapped to character
//! boundaries, never trusted. That guard is the kernel's
//! [`snap_down`](iridium_editor::render::snap_down) now, for the same reason —
//! the web resolvers did not have it and would have panicked on the first fold
//! drift over non-ASCII text.

use std::ops::Range;

use iridium_editor::Editor;
use iridium_editor::render::{
    HighlightContext, HighlightSource, SpanRun, flatten_spans, snap_down,
};
use iridium_editor::span_index::{SpanIndex, WindowedSpanCache};
use iridium_editor::syntax::highlight_to_color;
use iridium_editor::theme::{Color, SyntaxColors};

// The kernel's overscan margin, re-exported so this face's viewport docs
// (`App::viewport_window`) can name it where its slack is absorbed. One
// definition, kernel-owned — the parser-tax map's R1 ruling.
pub use iridium_editor::span_index::OVERSCAN_LINES;

/// The cached parse-derived state frames are resolved from: this face's
/// handle on the kernel's [`WindowedSpanCache`].
///
/// One per session, owned by the app beside the editor.
/// [`Self::refresh_windowed`] runs before every frame and costs a generation
/// and window comparison when nothing changed; the span rebuild — over the
/// covered window only, never the whole document — happens only when the
/// kernel actually reparsed or the viewport escaped the cover. The gates,
/// the window arithmetic and the derive all live in the kernel; what this
/// type adds is [`Self::resolver`], the compositor-facing seam.
#[derive(Debug, Default)]
pub struct HighlightCache {
    /// The hoisted cache: spans, covered window, generation gates.
    spans: WindowedSpanCache,
}

impl HighlightCache {
    /// An empty cache: no language, no spans, resolvers that answer `None`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            spans: WindowedSpanCache::new(),
        }
    }

    /// Brings the cache up to date with the kernel's tree over the whole
    /// document — [`Self::refresh_windowed`] with a window covering every
    /// line.
    ///
    /// The convenience form for callers without a viewport (headless states,
    /// tests): its derive is O(document), which is exactly the cost the
    /// windowed form exists to avoid, so the per-frame path must pass its
    /// real viewport instead.
    pub fn refresh(&mut self, editor: &Editor) {
        self.spans.refresh(editor);
    }

    /// Brings the cache up to date with the kernel's tree, reusing everything
    /// it can, deriving spans for `viewport_lines` (document lines, half-open)
    /// widened by [`OVERSCAN_LINES`] each way.
    ///
    /// The rebuild gates and their meaning for this face: with no language
    /// set the cache empties and reports the language inactive — the
    /// compositor renders the plain foreground. A language whose bundled
    /// highlight query does not compile empties the cache but keeps the
    /// language active — it degrades to the compositor's keyword bridge.
    /// Otherwise the spans are rebuilt only when the parse count or document
    /// revision moved *or* the requested viewport escaped the covered
    /// window; see [`WindowedSpanCache::refresh_windowed`].
    pub fn refresh_windowed(&mut self, editor: &Editor, viewport_lines: Range<usize>) {
        self.spans.refresh_windowed(editor, viewport_lines);
    }

    /// How many times the spans have been rebuilt since the cache was
    /// created.
    ///
    /// Exists so the generation gate is testable from outside: refreshing
    /// without an intervening edit must leave this unchanged.
    #[must_use]
    pub const fn rebuilds(&self) -> u64 {
        self.spans.rebuilds()
    }

    /// The generation the per-frame resolver reports to the compositor —
    /// see [`WindowedSpanCache::generation`] for what moves it.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.spans.generation()
    }

    /// Whether the kernel had a language set at the last [`Self::refresh`]
    /// — the answer the resolver carries as
    /// [`HighlightSource::language_active`].
    #[must_use]
    pub const fn language_active(&self) -> bool {
        self.spans.language_active()
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
            index: self.spans.index(),
            colors,
            generation: self.spans.generation(),
            language_active: self.spans.language_active(),
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
    /// The owning [`HighlightCache`]'s language answer at construction —
    /// see [`HighlightCache::language_active`].
    language_active: bool,
}

impl HighlightSource for FrameHighlights<'_> {
    fn resolve<'a>(&mut self, context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>> {
        let index = self.index?;
        Some(rich_spans(index, context, self.colors))
    }

    fn language_active(&self) -> bool {
        self.language_active
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

    // Snapped here rather than inside the flattening: the drift is this face's
    // (document offsets against fold-collapsed content), and the kernel's rule
    // is about which span owns a byte, not about where a byte begins. A span
    // that snapping empties is discarded by `flatten_spans` along with any the
    // window handed over degenerate.
    let spans: Vec<SpanRun<_>> = index
        .query(start_byte, end_byte)
        .map(|span| SpanRun {
            start: snap_down(visible, span.start.saturating_sub(start_byte)),
            end: snap_down(visible, span.end.saturating_sub(start_byte)),
            payload: span.highlight,
        })
        .collect();

    // Order is left as the index yields it: `flatten_spans` sorts stably and
    // resolves identical ranges in favour of the first, which is the rule
    // `Highlighter::spans_with` already applied when the spans were emitted.
    flatten_spans(spans, visible.len())
        .into_iter()
        .map(|run| {
            let colour = run.payload.map_or(context.foreground, |highlight| {
                highlight_to_color(highlight, colors)
            });
            (&visible[run.start..run.end], colour)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fmt::Write as _;

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

    /// ⭐ A span nested inside another must keep its own colour.
    ///
    /// The grammar captures the whole template literal as a string *and* the
    /// `${...}` inside it as embedded code, both correctly. Before
    /// `flatten_spans` the resolver walked the spans in start order and skipped
    /// any that began inside a range already claimed — so the literal, which
    /// starts first, took all of it and every span within was discarded. The
    /// whole interpolation rendered as string.
    ///
    /// Asserted on the run's exact text rather than on a colour count: the
    /// symptom is that no run for `y` exists at all.
    #[test]
    fn an_interpolation_inside_a_template_literal_keeps_its_own_colours() {
        let source = "const a = `x${y}z`;\n";
        let mut editor = Editor::with_defaults();
        editor.set_content(source);
        editor.set_language(Language::TypeScript);
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);

        let theme = Theme::default();
        let syntax_theme = HashMap::new();
        let viewport = ViewportConfig::default();
        let foreground = theme.editor.foreground;
        let context = context_over(source, &syntax_theme, &viewport, foreground);
        let runs = cache
            .resolver(&theme.syntax)
            .resolve(&context)
            .expect("a parsed TypeScript document resolves spans");

        let rebuilt: String = runs.iter().map(|(text, _)| *text).collect();
        assert_eq!(
            rebuilt, source,
            "the runs must still cover the content exactly"
        );

        let interpolated = runs
            .iter()
            .find(|(text, _)| *text == "y")
            .map(|(_, colour)| *colour);
        assert_eq!(
            interpolated,
            Some(theme.syntax.variable),
            "the interpolated variable was swallowed by the string span around it"
        );
        assert!(
            runs.iter()
                .any(|(text, colour)| text.contains('x') && *colour == theme.syntax.string),
            "and the literal's own text must still paint as a string"
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
            "and the answer really did change: the resolver has no spans"
        );
        assert!(
            !cache.language_active(),
            "the cleared language is reported inactive — the frame renders plain"
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
    fn without_a_language_the_resolver_reports_no_spans_and_no_language() {
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
        let mut resolver = cache.resolver(&theme.syntax);
        assert!(
            resolver.resolve(&context).is_none(),
            "no language means no spans"
        );
        assert!(
            !resolver.language_active(),
            "and no language to bridge to — the compositor renders plain, \
             never the keyword fallback"
        );
        assert_eq!(cache.rebuilds(), 0);
    }

    /// The bridge half of the ruling: a language that is set but whose spans
    /// are unavailable keeps the language active, so the compositor's
    /// keyword fallback still colours.
    #[test]
    fn a_set_language_keeps_the_language_active_for_the_bridge() {
        let editor = rust_editor("fn main() {}\n");
        let mut cache = HighlightCache::new();
        cache.refresh(&editor);
        assert!(cache.language_active(), "a set language is reported active");

        let theme = Theme::default();
        assert!(
            cache.resolver(&theme.syntax).language_active(),
            "and the per-frame resolver carries that answer"
        );
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

    // `snap_down`'s own unit test went with it into the kernel, where the web
    // face's use of it is covered too. What stays here is the face-level
    // guarantee it exists for —
    // `multibyte_content_is_covered_without_splitting_characters`, above, which
    // asserts it through the resolver rather than at the function.

    // =====================================================================
    // The windowed derive (docs/design/PARSER-TAX-MAP.md stage 1)
    // =====================================================================

    /// A Rust document of `total` lines carrying one multi-line string from
    /// line `string_start` through `string_end` inclusive — the span shape
    /// every boundary question below is asked about.
    fn source_with_straddling_string(
        total: usize,
        string_start: usize,
        string_end: usize,
    ) -> String {
        let mut out = String::new();
        for line in 0..total {
            if line == string_start {
                out.push_str("const LONG: &str = \"string start\n");
            } else if line > string_start && line < string_end {
                out.push_str("string interior\n");
            } else if line == string_end {
                out.push_str("string end\";\n");
            } else {
                writeln!(out, "fn f{line}() {{ let value = {line}; }}")
                    .expect("writing to a String cannot fail");
            }
        }
        out
    }

    /// Resolves one whole document line against `cache` and returns its runs.
    fn runs_for_line<'a>(
        cache: &HighlightCache,
        source: &'a str,
        line_start_byte: usize,
        line_end_byte: usize,
        theme: &Theme,
        syntax_theme: &'a HashMap<String, Color>,
        viewport: &'a ViewportConfig,
    ) -> Vec<(&'a str, Color)> {
        let context = HighlightContext {
            content: &source[line_start_byte..line_end_byte],
            content_start_byte: line_start_byte,
            content_width: 800.0,
            line_height: 20.0,
            viewport_config: viewport,
            syntax_theme,
            foreground: theme.editor.foreground,
        };
        cache
            .resolver(&theme.syntax)
            .resolve(&context)
            .expect("a parsed Rust document resolves spans")
    }

    /// The byte range of one document line, without its newline.
    fn line_bytes(editor: &Editor, line: usize) -> (usize, usize) {
        let document = &editor.state().document;
        let start = document
            .line_to_byte_offset(line)
            .expect("the fixture line exists");
        let text = document.line(line).expect("the fixture line exists");
        (start, start + text.len())
    }

    /// Proof obligation: a span straddling the covered window's edge renders
    /// at the window edge exactly as the whole-document derive renders it.
    /// The string here starts inside the viewport and runs more than
    /// [`super::OVERSCAN_LINES`] past it, so its node crosses the covered
    /// window's end; tree-sitter yields it whole and the resolver clamps it.
    #[test]
    fn a_span_straddling_the_covered_window_edge_resolves_like_the_whole_document() {
        let string_start = 85;
        let string_end = 195;
        let source = source_with_straddling_string(400, string_start, string_end);
        let editor = rust_editor(&source);

        let mut whole = HighlightCache::new();
        whole.refresh(&editor);
        let mut windowed = HighlightCache::new();
        // Viewport 40..90: covered 0..190, so the string (85..=195) enters
        // inside the viewport and leaves past the covered end.
        windowed.refresh_windowed(&editor, 40..90);

        let theme = Theme::default();
        let syntax_theme = HashMap::new();
        let viewport = ViewportConfig::default();
        for line in [86, 88] {
            let (start, end) = line_bytes(&editor, line);
            let from_whole = runs_for_line(
                &whole,
                &source,
                start,
                end,
                &theme,
                &syntax_theme,
                &viewport,
            );
            let from_window = runs_for_line(
                &windowed,
                &source,
                start,
                end,
                &theme,
                &syntax_theme,
                &viewport,
            );
            assert_eq!(
                from_window, from_whole,
                "line {line}: the windowed derive must answer exactly as the \
                 whole-document derive at the window edge"
            );
            assert!(
                from_window
                    .iter()
                    .any(|(_, colour)| *colour == theme.syntax.string),
                "line {line}: the straddling string really does paint as a string"
            );
        }
    }

    /// The "window only" observable: content past the covered window has no
    /// spans in the windowed index — which is what proves the derive was
    /// scoped — while the whole-document derive colours it.
    #[test]
    fn content_outside_the_covered_window_carries_no_spans() {
        let source = source_with_straddling_string(400, 85, 195);
        let editor = rust_editor(&source);

        let mut whole = HighlightCache::new();
        whole.refresh(&editor);
        let mut windowed = HighlightCache::new();
        windowed.refresh_windowed(&editor, 40..90);

        let theme = Theme::default();
        let syntax_theme = HashMap::new();
        let viewport = ViewportConfig::default();
        let (start, end) = line_bytes(&editor, 300);
        let from_whole = runs_for_line(
            &whole,
            &source,
            start,
            end,
            &theme,
            &syntax_theme,
            &viewport,
        );
        let from_window = runs_for_line(
            &windowed,
            &source,
            start,
            end,
            &theme,
            &syntax_theme,
            &viewport,
        );
        assert!(
            from_whole
                .iter()
                .any(|(_, colour)| *colour != theme.editor.foreground),
            "the whole-document derive colours line 300"
        );
        assert!(
            from_window
                .iter()
                .all(|(_, colour)| *colour == theme.editor.foreground),
            "the windowed derive never derived spans 110 lines past its cover"
        );
    }

    /// Proof obligation: scrolling within the overscan is a comparison — no
    /// rebuild, no generation move.
    #[test]
    fn scrolling_within_the_overscan_rebuilds_nothing() {
        let source = source_with_straddling_string(600, 85, 195);
        let editor = rust_editor(&source);
        let mut cache = HighlightCache::new();
        cache.refresh_windowed(&editor, 200..250);
        assert_eq!(cache.rebuilds(), 1);
        let generation = cache.generation();

        // Covered window: 100..350. Both requests stay inside it.
        cache.refresh_windowed(&editor, 230..280);
        cache.refresh_windowed(&editor, 120..170);
        assert_eq!(
            cache.rebuilds(),
            1,
            "a scroll within the overscan is a window comparison, not a rebuild"
        );
        assert_eq!(
            cache.generation(),
            generation,
            "and the resolution answer has not changed, so the generation holds"
        );
    }

    /// Proof obligation: scrolling past the cover re-derives, and the new
    /// cover is a new answer — the generation moves like any rebuild.
    #[test]
    fn scrolling_past_the_cover_rebuilds_and_moves_the_generation() {
        let source = source_with_straddling_string(600, 85, 195);
        let editor = rust_editor(&source);
        let mut cache = HighlightCache::new();
        cache.refresh_windowed(&editor, 200..250);
        assert_eq!(cache.rebuilds(), 1);
        let generation = cache.generation();

        // Covered window: 100..350. One line past the cover escapes it.
        cache.refresh_windowed(&editor, 301..351);
        assert_eq!(
            cache.rebuilds(),
            2,
            "a viewport escaping the covered window forces the re-derive"
        );
        assert_ne!(
            cache.generation(),
            generation,
            "content outside the old cover now answers differently"
        );

        // And the new cover holds for the viewport that produced it.
        cache.refresh_windowed(&editor, 301..351);
        assert_eq!(
            cache.rebuilds(),
            2,
            "the new cover answers the new viewport"
        );
    }

    /// Proof obligation: a generation move inside the window rebuilds — the
    /// parse-gate half of the trigger is untouched by the windowing.
    #[test]
    fn an_edit_inside_the_window_rebuilds_the_window() {
        let source = source_with_straddling_string(400, 85, 195);
        let mut editor = rust_editor(&source);
        let mut cache = HighlightCache::new();
        cache.refresh_windowed(&editor, 40..90);
        assert_eq!(cache.rebuilds(), 1);
        let generation = cache.generation();

        editor.paste("// note\n");
        cache.refresh_windowed(&editor, 40..90);
        assert_eq!(
            cache.rebuilds(),
            2,
            "the edit's reparse rebuilds the window once"
        );
        assert_ne!(cache.generation(), generation);
        cache.refresh_windowed(&editor, 40..90);
        assert_eq!(cache.rebuilds(), 2, "and only once");
    }

    /// A short document's viewport estimate may run past its last line; the
    /// clamp keeps such a request answerable, so it must not rebuild forever.
    #[test]
    fn a_viewport_past_the_document_end_is_clamped_not_rebuilt_every_frame() {
        let editor = rust_editor("fn main() {}\n");
        let mut cache = HighlightCache::new();
        cache.refresh_windowed(&editor, 0..500);
        assert_eq!(cache.rebuilds(), 1);
        cache.refresh_windowed(&editor, 0..500);
        cache.refresh_windowed(&editor, 0..500);
        assert_eq!(
            cache.rebuilds(),
            1,
            "an over-long viewport clamps to the document and stays covered"
        );
    }
}
