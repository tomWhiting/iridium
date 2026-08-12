//! Which of a theme's fourteen syntax colours a highlight category paints from.
//!
//! # ⚠️ Why this is in `theme` and not beside the highlighter
//!
//! It was in `crate::syntax`, which is **feature-gated on `syntax`**, and the
//! theme module is not. That was invisible for as long as only the renderer
//! asked the question — the renderer is in the same gated half. Importing a
//! VS Code theme asks it from the ungated half, and the parser-free kernel
//! build stopped compiling.
//!
//! ⭐ The placement is right on its own terms, not merely expedient: "which
//! field is this category painted from" is a fact about
//! [`SyntaxColors`](super::SyntaxColors), which has fourteen fields, rather
//! than about the parser, which has none. `highlight_to_color` reads it
//! forwards and the VS Code import reads it backwards, and both get one map.
//!
//! ⚠️ [`HighlightType`](crate::HighlightType) is itself two types — the real
//! one from `iridium_syntax` and the stub in `crate::syntax_stubs` — depending
//! on the feature. The `match` below is exhaustive, so a variant either build
//! is missing fails to compile here. That is the only structural guard the two
//! enums have; see `emphasis`'s parity test for the half a `match` cannot
//! reach.

use crate::HighlightType;

use super::SyntaxSlot;

/// Which of [`SyntaxColors`](super::SyntaxColors)' fourteen fields a category
/// is painted from.
///
/// ⭐ **The one forty-onto-fourteen map, and the reason it is a `SyntaxSlot`
/// rather than a `Color`.** `crate::syntax::highlight_to_color` is this read forwards.
/// Importing a VS Code theme reads it *backwards* — a `TextMate` scope names a
/// category and the imported colour has to land in whatever field that category
/// paints from — and a second copy of this map written for that direction would
/// drift from this one the first time a category was added. There is one map,
/// read two ways.
#[must_use]
#[allow(clippy::match_same_arms)] // Different semantic types intentionally map to same slot
pub const fn slot_of(highlight: HighlightType) -> SyntaxSlot {
    use SyntaxSlot as Slot;

    match highlight {
        // Keywords
        HighlightType::Keyword | HighlightType::KeywordControl => Slot::Keyword,

        // Strings
        HighlightType::String | HighlightType::StringEscape => Slot::String,

        // Numbers and booleans
        HighlightType::Number | HighlightType::Boolean => Slot::Number,

        // Comments
        HighlightType::Comment | HighlightType::CommentDoc => Slot::Comment,

        // Functions
        HighlightType::Function
        | HighlightType::FunctionDefinition
        | HighlightType::FunctionMethod
        | HighlightType::FunctionSpecial => Slot::Function,

        // Variables
        HighlightType::Variable
        | HighlightType::VariableParameter
        | HighlightType::VariableSpecial => Slot::Variable,

        // Types
        HighlightType::Type | HighlightType::TypeBuiltin | HighlightType::TypeInterface => {
            Slot::TypeName
        },

        // Operators
        HighlightType::Operator => Slot::Operator,

        // Punctuation
        HighlightType::PunctuationBracket
        | HighlightType::PunctuationDelimiter
        | HighlightType::PunctuationSpecial => Slot::Punctuation,

        // Properties
        HighlightType::Property => Slot::Property,

        // Constants
        HighlightType::Constant => Slot::Constant,

        // Rust-specific
        HighlightType::Lifetime => Slot::TypeName, // Use type color for lifetimes

        // Attributes/decorators
        HighlightType::Attribute => Slot::Attribute,

        // Tags (HTML/XML)
        HighlightType::Tag => Slot::Tag,

        // Embedded content - use default foreground
        HighlightType::Embedded => Slot::Variable,

        // ----- Markup -----
        //
        // ⭐ **The six that already reached the screen keep the exact colour
        // they had before markup was split off the code slots.** Giving markup
        // its own categories is what lets a theme style a heading without
        // touching every `fn` in every language; it is emphatically not a
        // change to what markdown looks like today, and
        // `the_markup_slots_render_exactly_as_they_did_before_they_had_slots`
        // is what stops one arriving by accident.
        HighlightType::MarkupHeading => Slot::Keyword,
        HighlightType::MarkupLink => Slot::Function,
        HighlightType::MarkupUrl => Slot::String,
        HighlightType::MarkupList
        | HighlightType::MarkupPunctuation
        | HighlightType::MarkupFence => Slot::Punctuation,

        // The four inline categories had no mapping at all before the split —
        // `emphasis.markup` and its siblings live in `markdown-inline`, which
        // has no grammar linked, so nothing has ever asked for their colour.
        // They are ruled here rather than left to fall through, because a
        // category with no colour is the same silent-unstyled defect #70
        // cleared and there is no reason to reintroduce it and wait.
        //
        // Emphasis and strong emphasis take `operator`. Neither wants a colour
        // of its own — emphasised prose is body text wearing weight or slant,
        // which is what `render::RunStyle` carries — and `operator` is the
        // slot that is plain page ink in both shipped presets. That agreement
        // is a fact about today's themes rather than about the categories, so
        // `markup_emphasis_borrows_a_slot_that_is_still_plain_ink` fails the
        // moment a theme parts the two and markup needs its own field.
        //
        // Strikethrough takes `comment`: withdrawn text is recessive, and
        // `comment` is the recessive slot every theme already has. An inline
        // code span takes `string` — it is a literal, and reads as one.
        HighlightType::MarkupEmphasis | HighlightType::MarkupStrong => Slot::Operator,
        HighlightType::MarkupStrikethrough => Slot::Comment,
        HighlightType::MarkupCode => Slot::String,

        // ----- Diff status -----
        //
        // ⚠️ **No `SyntaxColors` field means "added" or "removed", so all four
        // of these are borrows and none of them is a colour ruling.** Said
        // plainly because the obvious reading of "added is green" is a fact
        // about a theme, not about a category, and both shipped presets
        // disagree with it: `string` is `ce9178` here, an orange-tan.
        //
        // Added takes `string` and removed takes `keyword` because those are
        // the two fallbacks **the vendored `diff` query names in its own
        // text** — `;; TODO: This should eventually be @diff.plus with a
        // fallback of @string`. Borrowing the upstream author's stated choice
        // is a weaker claim than inventing one, and a weaker claim is the right
        // kind here: nothing renders these today.
        //
        // Modified and moved take `attribute`, the slot for a token that
        // annotates the entity beside it — which is exactly what "modified:"
        // does to the path that follows it in a commit buffer. They share a
        // colour and keep separate categories, which is what a many-to-one
        // colour map is for.
        //
        // ⛔ **Deliberately not reaching `EditorColors`.** `change_added` and
        // `change_modified` exist there and are tempting, and they belong to
        // the change gutter — uncommitted edits measured against the file on
        // disk — which is a different feature answering a different question.
        // Widening this function's signature to reach them would let every
        // syntax category address the editor palette, and would change the TUI
        // palette's call site too, to serve four names nothing draws yet.
        // Diff's own theme fields belong with the theme work (#87), exactly as
        // markup's do.
        HighlightType::DiffAdded => Slot::String,
        HighlightType::DiffRemoved => Slot::Keyword,
        HighlightType::DiffModified | HighlightType::DiffMoved => Slot::Attribute,

        // Errors
        HighlightType::Error => Slot::Error,
    }
}
