//! What one frame resolves to: colours, coverage, and the gates that decide
//! whether the spans behind it were rebuilt.

use std::collections::HashMap;

use iridium_editor::render::{HighlightSource, ViewportConfig};
use iridium_editor::theme::Theme;
use iridium_editor::{Editor, Language};

use super::super::HighlightCache;
use super::{context_over, rust_editor};

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
        .resolver(&theme)
        .resolve(&context)
        .expect("a parsed Rust document resolves spans");

    let rebuilt: String = runs.iter().map(|(text, _)| *text).collect();
    assert_eq!(rebuilt, source, "the runs cover the content exactly");

    let colour_of = |needle: &str| {
        runs.iter()
            .find(|(text, _)| text.contains(needle))
            .map(|(_, style)| style.color)
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
        runs.iter().any(|(_, style)| style.color == foreground),
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
        .resolver(&theme)
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
        .map(|(_, style)| style.color);
    assert_eq!(
        interpolated,
        Some(theme.syntax.variable),
        "the interpolated variable was swallowed by the string span around it"
    );
    assert!(
        runs.iter()
            .any(|(text, style)| text.contains('x') && style.color == theme.syntax.string),
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
        cache.resolver(&theme).generation(),
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
        cache.resolver(&theme).resolve(&context).is_none(),
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
    let mut resolver = cache.resolver(&theme);
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
        cache.resolver(&theme).language_active(),
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
        .resolver(&theme)
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
        .resolver(&theme)
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
