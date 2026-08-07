//! Simple keyword-based syntax highlighter.
//!
//! This module provides basic syntax highlighting that works in WASM environments
//! where tree-sitter is not available. It uses simple pattern matching for common
//! language constructs like keywords, strings, comments, and numbers.

use crate::theme::{Color, Theme};

/// A highlighted span with text and color.
#[derive(Debug, Clone)]
pub struct HighlightSpan {
    /// The text content of this span
    pub text: String,
    /// The color for this span
    pub color: Color,
}

/// Token types for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    /// Plain text (identifiers, operators, etc.)
    Text,
    /// Language keywords (fn, let, if, etc.)
    Keyword,
    /// String literals
    String,
    /// Numeric literals
    Number,
    /// Comments (line and block)
    Comment,
    /// Type names (capitalized identifiers)
    Type,
    /// Function names
    Function,
    /// Punctuation
    Punctuation,
}

/// Colors for different token types.
#[derive(Debug, Clone)]
pub struct SyntaxColors {
    /// Default text color
    pub text: Color,
    /// Keyword color (fn, let, if, etc.)
    pub keyword: Color,
    /// String literal color
    pub string: Color,
    /// Number literal color
    pub number: Color,
    /// Comment color
    pub comment: Color,
    /// Type name color
    pub type_name: Color,
    /// Function name color
    pub function: Color,
    /// Punctuation color
    pub punctuation: Color,
}

impl SyntaxColors {
    /// The palette a highlighter holds before any theme reaches it.
    ///
    /// The one remaining hard-coded preset, and it is a *default* rather than
    /// a choice: [`SimpleHighlighter::new`] needs some palette before
    /// [`Self::from_theme`] has been called, and this matches the compositor's
    /// own default theme so a frame composed before the first `set_theme` is
    /// not a surprise. Its light counterpart is gone — nothing selects a
    /// palette by darkness any more, so a second preset had no way to be
    /// reached and no reason to exist.
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            text: Color::rgb(0.847, 0.871, 0.914),        // #D8DEE9
            keyword: Color::rgb(0.506, 0.631, 0.757),     // #81A1C1 (blue)
            string: Color::rgb(0.639, 0.745, 0.549),      // #A3BE8C (green)
            number: Color::rgb(0.702, 0.561, 0.678),      // #B48EAD (purple)
            comment: Color::rgb(0.396, 0.482, 0.514),     // #657B83 (gray)
            type_name: Color::rgb(0.922, 0.796, 0.545),   // #EBCB8B (yellow)
            function: Color::rgb(0.533, 0.753, 0.816),    // #88C0D0 (cyan)
            punctuation: Color::rgb(0.608, 0.639, 0.690), // #9BA0AB (light gray)
        }
    }

    /// The bridge palette a theme calls for.
    ///
    /// ⚠️ **From the whole [`Theme`], not from
    /// [`theme::SyntaxColors`](crate::theme::SyntaxColors)**, and the reason is
    /// [`Self::text`]: it is the colour for tokens this highlighter could not
    /// classify, and the theme's syntax colours have no such field. It comes
    /// from `editor.foreground`, so the conversion needs both halves. Taking
    /// only the syntax colours would have left unclassified text on a
    /// hard-coded default — the same defect, one field smaller.
    ///
    /// # Why six of the theme's fields are dropped
    ///
    /// The theme states fourteen token colours; this palette has eight. The
    /// missing six — `variable`, `operator`, `property`, `constant`, `tag`,
    /// `attribute` — have no counterpart here because [`TokenType`] has eight
    /// variants and this highlighter classifies by keyword table and character
    /// class. It cannot tell a variable from a property, so there is no token
    /// it could paint with those colours. Dropping them loses no information.
    #[must_use]
    pub const fn from_theme(theme: &Theme) -> Self {
        Self {
            text: theme.editor.foreground,
            keyword: theme.syntax.keyword,
            string: theme.syntax.string,
            number: theme.syntax.number,
            comment: theme.syntax.comment,
            type_name: theme.syntax.type_name,
            function: theme.syntax.function,
            punctuation: theme.syntax.punctuation,
        }
    }

    /// Gets the color for a token type.
    #[must_use]
    pub const fn color_for(&self, token_type: TokenType) -> Color {
        match token_type {
            TokenType::Text => self.text,
            TokenType::Keyword => self.keyword,
            TokenType::String => self.string,
            TokenType::Number => self.number,
            TokenType::Comment => self.comment,
            TokenType::Type => self.type_name,
            TokenType::Function => self.function,
            TokenType::Punctuation => self.punctuation,
        }
    }
}

impl Default for SyntaxColors {
    fn default() -> Self {
        Self::dark()
    }
}

/// Simple syntax highlighter for WASM environments.
///
/// This provides basic syntax highlighting using pattern matching rather than
/// tree-sitter, making it compatible with WASM builds.
#[derive(Debug, Default)]
pub struct SimpleHighlighter {
    /// Colors for different token types
    colors: SyntaxColors,
}

impl SimpleHighlighter {
    /// Creates a new highlighter with default dark theme colors.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            colors: SyntaxColors::dark(),
        }
    }

    /// Creates a highlighter with the specified colors.
    #[must_use]
    pub const fn with_colors(colors: SyntaxColors) -> Self {
        Self { colors }
    }

    /// Sets the syntax colors.
    ///
    /// The only way to change this palette, deliberately. It replaced a pair
    /// of `set_dark_theme` / `set_light_theme` methods that installed the two
    /// hard-coded presets — the mechanism by which the bridge's colours could
    /// diverge from the theme's, since neither method took a theme and neither
    /// caller could pass one. Callers now derive the palette with
    /// [`SyntaxColors::from_theme`], so the only palette reachable at runtime
    /// is the one the theme states.
    pub const fn set_colors(&mut self, colors: SyntaxColors) {
        self.colors = colors;
    }

    /// Highlights a line of text, returning colored spans.
    ///
    /// # Arguments
    ///
    /// * `line` - The line of text to highlight
    ///
    /// # Returns
    ///
    /// A vector of `HighlightSpan` with text and colors
    #[must_use]
    pub fn highlight_line(&self, line: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let mut chars = line.char_indices().peekable();
        let mut current_text = String::new();
        let mut current_type = TokenType::Text;

        while let Some((idx, ch)) = chars.next() {
            // Check for line comments
            if ch == '/' {
                if let Some(&(_, '/')) = chars.peek() {
                    // Flush current span
                    if !current_text.is_empty() {
                        spans.push(HighlightSpan {
                            text: std::mem::take(&mut current_text),
                            color: self.colors.color_for(current_type),
                        });
                    }
                    // Rest of line is a comment
                    let comment = &line[idx..];
                    spans.push(HighlightSpan {
                        text: comment.to_string(),
                        color: self.colors.comment,
                    });
                    return spans;
                }
            }

            // Check for strings
            if ch == '"' || ch == '\'' || ch == '`' {
                // Flush current span
                if !current_text.is_empty() {
                    spans.push(HighlightSpan {
                        text: std::mem::take(&mut current_text),
                        color: self.colors.color_for(current_type),
                    });
                }

                let quote = ch;
                let mut string_content = String::from(ch);
                let mut escaped = false;

                for (_, c) in chars.by_ref() {
                    string_content.push(c);
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == quote {
                        break;
                    }
                }

                spans.push(HighlightSpan {
                    text: string_content,
                    color: self.colors.string,
                });
                current_type = TokenType::Text;
                continue;
            }

            // Check for numbers
            if ch.is_ascii_digit()
                && (current_text.is_empty()
                    || !current_text.chars().last().unwrap_or(' ').is_alphanumeric())
            {
                // Flush current span
                if !current_text.is_empty() {
                    spans.push(HighlightSpan {
                        text: std::mem::take(&mut current_text),
                        color: self.colors.color_for(current_type),
                    });
                }

                let mut number = String::from(ch);
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_ascii_digit()
                        || c == '.'
                        || c == 'x'
                        || c == 'b'
                        || c == 'o'
                        || c == '_'
                        || c.is_ascii_hexdigit()
                    {
                        number.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }

                spans.push(HighlightSpan {
                    text: number,
                    color: self.colors.number,
                });
                current_type = TokenType::Text;
                continue;
            }

            // Check for identifiers and keywords
            if ch.is_alphabetic() || ch == '_' {
                // Flush current span
                if !current_text.is_empty() {
                    spans.push(HighlightSpan {
                        text: std::mem::take(&mut current_text),
                        color: self.colors.color_for(current_type),
                    });
                }

                let mut word = String::from(ch);
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        word.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }

                // Check if it's followed by ( to detect function calls
                let is_function = chars.peek().is_some_and(|(_, c)| *c == '(');

                let token_type = if is_keyword(&word) {
                    TokenType::Keyword
                } else if is_type(&word) {
                    TokenType::Type
                } else if is_function {
                    TokenType::Function
                } else {
                    TokenType::Text
                };

                spans.push(HighlightSpan {
                    text: word,
                    color: self.colors.color_for(token_type),
                });
                current_type = TokenType::Text;
                continue;
            }

            // Punctuation
            if is_punctuation(ch) {
                // Flush current span
                if !current_text.is_empty() {
                    spans.push(HighlightSpan {
                        text: std::mem::take(&mut current_text),
                        color: self.colors.color_for(current_type),
                    });
                }

                spans.push(HighlightSpan {
                    text: ch.to_string(),
                    color: self.colors.punctuation,
                });
                continue;
            }

            // Default: add to current text
            current_text.push(ch);
        }

        // Flush remaining text
        if !current_text.is_empty() {
            spans.push(HighlightSpan {
                text: current_text,
                color: self.colors.color_for(current_type),
            });
        }

        spans
    }

    /// Highlights multiple lines of text.
    ///
    /// # Arguments
    ///
    /// * `text` - The full text to highlight
    ///
    /// # Returns
    ///
    /// A vector of lines, each containing colored spans
    #[must_use]
    pub fn highlight_text(&self, text: &str) -> Vec<Vec<HighlightSpan>> {
        text.lines().map(|line| self.highlight_line(line)).collect()
    }

    /// Highlights text and returns a flat list of spans with newlines preserved.
    ///
    /// This is suitable for use with `TextRenderer::set_rich_text`.
    #[must_use]
    pub fn highlight_flat(&self, text: &str) -> Vec<HighlightSpan> {
        let mut result = Vec::new();
        let lines: Vec<&str> = text.split('\n').collect();

        for (i, line) in lines.iter().enumerate() {
            result.extend(self.highlight_line(line));

            // Add newline span (except for last line)
            if i < lines.len() - 1 {
                result.push(HighlightSpan {
                    text: "\n".to_string(),
                    color: self.colors.text,
                });
            }
        }

        result
    }
}

/// Checks if a word is a keyword.
fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        // Rust keywords
        "fn" | "let" | "mut" | "const" | "static" | "if" | "else" | "match" | "while" | "for"
        | "loop" | "break" | "continue" | "return" | "struct" | "enum" | "impl" | "trait"
        | "pub" | "mod" | "use" | "crate" | "self" | "Self" | "super" | "where" | "async"
        | "await" | "move" | "ref" | "type" | "dyn" | "unsafe" | "extern" | "as" | "in"
        // JavaScript/TypeScript keywords
        | "function" | "var" | "class" | "extends" | "new" | "this" | "import" | "export"
        | "default" | "from" | "try" | "catch" | "finally" | "throw" | "typeof" | "instanceof"
        | "switch" | "case" | "yield" | "delete" | "void" | "with" | "debugger"
        // Python keywords
        | "def" | "lambda" | "pass" | "raise" | "assert" | "global" | "nonlocal" | "del"
        | "is" | "not" | "and" | "or" | "None" | "True" | "False" | "except" | "elif"
        // Go keywords
        | "package" | "go" | "defer" | "chan" | "select" | "fallthrough" | "range" | "map"
        | "interface"
        // Common keywords
        | "true" | "false" | "null" | "undefined" | "nil"
    )
}

/// Checks if a word looks like a type (starts with uppercase).
fn is_type(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
        && word.chars().skip(1).any(char::is_lowercase)
}

/// Checks if a character is punctuation.
const fn is_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
            | ','
            | '.'
            | ':'
            | ';'
            | '='
            | '+'
            | '-'
            | '*'
            | '/'
            | '%'
            | '&'
            | '|'
            | '^'
            | '!'
            | '?'
            | '#'
            | '@'
            | '$'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_keywords() {
        let highlighter = SimpleHighlighter::new();
        let spans = highlighter.highlight_line("fn main() {");

        assert!(spans.len() >= 3);
        assert_eq!(spans[0].text, "fn");
    }

    #[test]
    fn highlight_strings() {
        let highlighter = SimpleHighlighter::new();
        let spans = highlighter.highlight_line("let s = \"hello world\";");

        // Find the string span
        let string_span = spans.iter().find(|s| s.text.contains("hello")).unwrap();
        assert!(string_span.text.starts_with('"'));
        assert!(string_span.text.ends_with('"'));
    }

    #[test]
    fn highlight_comments() {
        let highlighter = SimpleHighlighter::new();
        let spans = highlighter.highlight_line("let x = 5; // comment");

        // Last span should be the comment
        let comment_span = spans.last().unwrap();
        assert!(comment_span.text.starts_with("//"));
    }

    #[test]
    fn highlight_numbers() {
        let highlighter = SimpleHighlighter::new();
        let spans = highlighter.highlight_line("let x = 42;");

        // Find the number span
        let number_span = spans.iter().find(|s| s.text == "42").unwrap();
        assert_eq!(number_span.text, "42");
    }

    #[test]
    fn highlight_types() {
        let highlighter = SimpleHighlighter::new();
        let spans = highlighter.highlight_line("let x: String = s;");

        // Find the type span
        let type_span = spans.iter().find(|s| s.text == "String").unwrap();
        assert_eq!(type_span.text, "String");
    }

    #[test]
    fn highlight_multiline() {
        let highlighter = SimpleHighlighter::new();
        let text = "fn main() {\n    println!(\"Hello\");\n}";
        let lines = highlighter.highlight_text(text);

        assert_eq!(lines.len(), 3);
    }
}
