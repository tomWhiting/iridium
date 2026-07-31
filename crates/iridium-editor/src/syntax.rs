//! Syntax highlighting integration.
//!
//! This module bridges `iridium_syntax` with the editor's theme system,
//! providing syntax highlighting for the document.

use crate::theme::{Color, SyntaxColors};

// Re-export core syntax types
pub use iridium_syntax::{
    HighlightSpan, HighlightType, Highlighter, Language, SyntaxError, SyntaxTree,
};

/// Converts a `HighlightType` to a `Color` using the theme's syntax colors.
///
/// This function maps semantic highlight types to concrete colors from the theme.
#[must_use]
#[allow(clippy::match_same_arms)] // Different semantic types intentionally map to same color
pub const fn highlight_to_color(highlight: HighlightType, syntax: &SyntaxColors) -> Color {
    match highlight {
        // Keywords
        HighlightType::Keyword | HighlightType::KeywordControl => syntax.keyword,

        // Strings
        HighlightType::String | HighlightType::StringEscape => syntax.string,

        // Numbers and booleans
        HighlightType::Number | HighlightType::Boolean => syntax.number,

        // Comments
        HighlightType::Comment | HighlightType::CommentDoc => syntax.comment,

        // Functions
        HighlightType::Function
        | HighlightType::FunctionDefinition
        | HighlightType::FunctionMethod
        | HighlightType::FunctionSpecial => syntax.function,

        // Variables
        HighlightType::Variable
        | HighlightType::VariableParameter
        | HighlightType::VariableSpecial => syntax.variable,

        // Types
        HighlightType::Type | HighlightType::TypeBuiltin | HighlightType::TypeInterface => {
            syntax.type_name
        },

        // Operators
        HighlightType::Operator => syntax.operator,

        // Punctuation
        HighlightType::PunctuationBracket
        | HighlightType::PunctuationDelimiter
        | HighlightType::PunctuationSpecial => syntax.punctuation,

        // Properties
        HighlightType::Property => syntax.property,

        // Constants
        HighlightType::Constant => syntax.constant,

        // Rust-specific
        HighlightType::Lifetime => syntax.type_name, // Use type color for lifetimes

        // Attributes/decorators
        HighlightType::Attribute => syntax.attribute,

        // Tags (HTML/XML)
        HighlightType::Tag => syntax.tag,

        // Embedded content - use default foreground
        HighlightType::Embedded => syntax.variable,

        // Errors
        HighlightType::Error => syntax.error,
    }
}

/// A syntax highlighter wrapper that caches highlights for the current document.
///
/// This struct manages the underlying `Highlighter` and provides methods
/// to get colored spans for rendering.
#[derive(Debug)]
pub struct DocumentHighlighter {
    /// The underlying highlighter (if language is supported)
    highlighter: Option<Highlighter>,
    /// The parse tree the spans are read from.
    ///
    /// Owned here only until the editor owns one tree for the whole document.
    /// The highlighter itself holds no tree, so this is the only copy on this
    /// path and there is nothing for it to disagree with.
    tree: Option<SyntaxTree>,
    /// The detected language
    language: Option<Language>,
    /// Cached highlight spans
    cached_spans: Vec<HighlightSpan>,
    /// Whether the cache is valid
    cache_valid: bool,
}

impl Default for DocumentHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentHighlighter {
    /// Creates a new document highlighter with no language set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            highlighter: None,
            tree: None,
            language: None,
            cached_spans: Vec::new(),
            cache_valid: false,
        }
    }

    /// Sets the language for highlighting.
    ///
    /// # Errors
    ///
    /// Returns an error if the language is not supported or the query file
    /// is invalid for the grammar version.
    pub fn set_language(&mut self, language: Language) -> Result<(), SyntaxError> {
        self.language = Some(language);
        self.highlighter = Some(Highlighter::new(language)?);
        self.tree = Some(SyntaxTree::new(language)?);
        self.cache_valid = false;
        Ok(())
    }

    /// Detects and sets the language from a file extension.
    ///
    /// Returns `None` if the extension is not recognized or the highlighter
    /// fails to initialize.
    pub fn set_language_from_extension(&mut self, ext: &str) -> Option<Language> {
        let lang = Language::from_extension(ext)?;

        if self.set_language(lang).is_ok() {
            return Some(lang);
        }

        None
    }

    /// Clears the language, disabling highlighting.
    pub fn clear_language(&mut self) {
        self.highlighter = None;
        self.tree = None;
        self.language = None;
        self.cached_spans.clear();
        self.cache_valid = false;
    }

    /// Returns the current language, if any.
    #[must_use]
    pub const fn language(&self) -> Option<Language> {
        self.language
    }

    /// Returns true if highlighting is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.highlighter.is_some()
    }

    /// Highlights the given source code, caching the result.
    ///
    /// If no language is set, returns an empty slice.
    pub fn highlight(&mut self, source: &str) -> &[HighlightSpan] {
        if !self.cache_valid
            && let (Some(highlighter), Some(tree)) = (&self.highlighter, &mut self.tree)
            && let Some(parsed) = tree.parse(source)
        {
            self.cached_spans = highlighter.spans_in(parsed, source);
            self.cache_valid = true;
        }
        &self.cached_spans
    }

    /// Updates the highlighter for an incremental edit.
    ///
    /// This is more efficient than re-highlighting the entire document.
    ///
    /// # Arguments
    ///
    /// * `source` - The new source code after the edit
    /// * `start_byte` - The byte offset where the edit started
    /// * `old_end_byte` - The byte offset where the edit ended in the old source
    /// * `new_end_byte` - The byte offset where the edit ended in the new source
    pub fn update(
        &mut self,
        source: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) {
        if let (Some(highlighter), Some(tree)) = (&self.highlighter, &mut self.tree)
            && let Some(parsed) = tree.edit_bytes(source, start_byte, old_end_byte, new_end_byte)
        {
            self.cached_spans = highlighter.spans_in(parsed, source);
            self.cache_valid = true;
        }
    }

    /// Invalidates the cache, forcing a re-highlight on next call.
    #[inline]
    pub const fn invalidate(&mut self) {
        self.cache_valid = false;
    }

    /// Returns the cached highlight spans.
    ///
    /// This does not re-highlight; use [`highlight`] to get fresh results.
    #[must_use]
    pub fn cached_spans(&self) -> &[HighlightSpan] {
        &self.cached_spans
    }

    /// Generates colored text spans from highlights.
    ///
    /// Returns an iterator of `(text_slice, color)` pairs suitable for
    /// `TextRenderer::set_rich_text`.
    pub fn colored_spans<'a>(
        &'a self,
        source: &'a str,
        syntax_colors: &'a SyntaxColors,
        default_color: Color,
    ) -> impl Iterator<Item = (&'a str, Color)> + 'a {
        ColoredSpanIterator::new(source, &self.cached_spans, syntax_colors, default_color)
    }
}

/// Iterator that produces colored text spans from highlight data.
struct ColoredSpanIterator<'a> {
    source: &'a str,
    spans: &'a [HighlightSpan],
    syntax_colors: &'a SyntaxColors,
    default_color: Color,
    current_byte: usize,
    span_index: usize,
}

impl<'a> ColoredSpanIterator<'a> {
    const fn new(
        source: &'a str,
        spans: &'a [HighlightSpan],
        syntax_colors: &'a SyntaxColors,
        default_color: Color,
    ) -> Self {
        Self {
            source,
            spans,
            syntax_colors,
            default_color,
            current_byte: 0,
            span_index: 0,
        }
    }
}

impl<'a> Iterator for ColoredSpanIterator<'a> {
    type Item = (&'a str, Color);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_byte >= self.source.len() {
            return None;
        }

        // Find the next span that starts at or after current position
        while self.span_index < self.spans.len()
            && self.spans[self.span_index].end <= self.current_byte
        {
            self.span_index += 1;
        }

        if self.span_index < self.spans.len() {
            let span = &self.spans[self.span_index];

            if span.start > self.current_byte {
                // There's unhighlighted text before this span
                let end = span.start.min(self.source.len());
                let text = &self.source[self.current_byte..end];
                self.current_byte = end;
                Some((text, self.default_color))
            } else {
                // We're inside a highlighted span
                let end = span.end.min(self.source.len());
                let text = &self.source[self.current_byte..end];
                let color = highlight_to_color(span.highlight, self.syntax_colors);
                self.current_byte = end;
                self.span_index += 1;
                Some((text, color))
            }
        } else {
            // No more spans, return remaining text with default color
            let text = &self.source[self.current_byte..];
            self.current_byte = self.source.len();
            Some((text, self.default_color))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_to_color_keywords() {
        let colors = SyntaxColors::dark();
        let color = highlight_to_color(HighlightType::Keyword, &colors);
        assert_eq!(color, colors.keyword);
    }

    #[test]
    fn document_highlighter_default() {
        let highlighter = DocumentHighlighter::new();
        assert!(!highlighter.is_enabled());
        assert!(highlighter.language().is_none());
    }

    #[test]
    fn document_highlighter_set_language() {
        let mut highlighter = DocumentHighlighter::new();
        highlighter.set_language(Language::Rust).unwrap();
        assert!(highlighter.is_enabled());
        assert_eq!(highlighter.language(), Some(Language::Rust));
    }

    #[test]
    fn document_highlighter_from_extension() {
        let mut highlighter = DocumentHighlighter::new();
        let lang = highlighter.set_language_from_extension("rs");
        assert_eq!(lang, Some(Language::Rust));
        assert!(highlighter.is_enabled());
    }

    #[test]
    fn document_highlighter_highlight() {
        let mut highlighter = DocumentHighlighter::new();
        highlighter.set_language(Language::Rust).unwrap();

        let source = "fn main() {}";
        let spans = highlighter.highlight(source);

        // Should have some spans
        assert!(!spans.is_empty());
    }

    #[test]
    fn colored_spans_iterator() {
        let mut highlighter = DocumentHighlighter::new();
        highlighter.set_language(Language::Rust).unwrap();

        let source = "fn main() {}";
        highlighter.highlight(source);

        let colors = SyntaxColors::dark();
        let default = Color::rgb(1.0, 1.0, 1.0);

        let spans: Vec<_> = highlighter
            .colored_spans(source, &colors, default)
            .collect();

        // Should produce spans covering the entire source
        let total_len: usize = spans.iter().map(|(s, _)| s.len()).sum();
        assert_eq!(total_len, source.len());
    }
}
