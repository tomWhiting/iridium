//! The named-region verbs, as byte ranges.
//!
//! The sibling of [`super::walk`], and deliberately a second path rather than a
//! wider version of the first. A tree walk is a pure function of the root node
//! and a range — [`iridium_syntax::navigate`] needs nothing else, so
//! [`super::walk::Walk`] carries nothing else. A vendored text-object query
//! needs three more things: the [`Language`] whose `textobjects.scm` is being
//! run, the source text the query matches captures against, and, for a jump, a
//! direction. Widening `Walk` to carry all three would put three parameters on
//! ten walks that have no use for any of them, and would make every one of them
//! fallible besides.
//!
//! So the shape here is `(source, range, caret) -> Option<Range>`, and the
//! caller applies it with [`super::expand`]'s `map_regions` exactly as it
//! applies a `Walk` with `map_selections`. `None` means the same thing in both:
//! this cursor stays where it is.

use std::ops::Range;

use iridium_syntax::query::textobject::{self, Direction, TextObject, Variant};
use iridium_syntax::{Language, Tree};

use crate::input::keyboard::AstRequest;

/// Everything a vendored query needs that a tree walk does not carry.
///
/// Grouped into one borrow rather than passed as three parameters so the
/// mapping function stays the same width as the one beside it.
pub(super) struct Source<'a> {
    /// The language whose `textobjects.scm` is run.
    pub(super) language: Language,
    /// The tree the query is matched against.
    pub(super) tree: &'a Tree,
    /// The text that tree was parsed from.
    ///
    /// Not merely a convenience: tree-sitter resolves a query's predicates —
    /// `#match?`, `#eq?` — against the source bytes, so a query cannot be run
    /// without them.
    pub(super) text: &'a str,
}

/// One named-region step, in bytes.
#[derive(Clone, Copy)]
pub(super) enum Region {
    /// The smallest region of this object and variant covering the selection.
    Covering {
        /// Which kind of named region to look for.
        object: TextObject,
        /// Whether to take the body alone or the whole construct.
        variant: Variant,
    },
    /// The nearest region of this object past the caret, travelling one way.
    Nearest {
        /// Which kind of named region to look for.
        object: TextObject,
        /// Which way to travel from the caret.
        direction: Direction,
    },
}

impl Region {
    /// The region a request asks for, or `None` if it is not query-driven.
    ///
    /// Spelled out arm by arm rather than with a wildcard: a tenth named-region
    /// verb added to [`AstRequest`] must fail to compile here, not silently fall
    /// through to "this request wants nothing".
    pub(super) const fn of(request: AstRequest) -> Option<Self> {
        Some(match request {
            AstRequest::SelectFunctionInside => Self::Covering {
                object: TextObject::Function,
                variant: Variant::Inside,
            },
            AstRequest::SelectFunctionAround => Self::Covering {
                object: TextObject::Function,
                variant: Variant::Around,
            },
            AstRequest::SelectClassInside => Self::Covering {
                object: TextObject::Class,
                variant: Variant::Inside,
            },
            AstRequest::SelectClassAround => Self::Covering {
                object: TextObject::Class,
                variant: Variant::Around,
            },
            AstRequest::SelectCommentAround => Self::Covering {
                object: TextObject::Comment,
                variant: Variant::Around,
            },
            AstRequest::NextFunction => Self::Nearest {
                object: TextObject::Function,
                direction: Direction::Forward,
            },
            AstRequest::PreviousFunction => Self::Nearest {
                object: TextObject::Function,
                direction: Direction::Backward,
            },
            AstRequest::NextClass => Self::Nearest {
                object: TextObject::Class,
                direction: Direction::Forward,
            },
            AstRequest::PreviousClass => Self::Nearest {
                object: TextObject::Class,
                direction: Direction::Backward,
            },

            AstRequest::SelectNode
            | AstRequest::ExpandSelection
            | AstRequest::ShrinkSelection
            | AstRequest::SelectNextSibling
            | AstRequest::SelectPreviousSibling
            | AstRequest::SelectFirstChild
            | AstRequest::SelectLastChild
            | AstRequest::ExtendNextSibling
            | AstRequest::ExtendPreviousSibling
            | AstRequest::CursorNodeStart
            | AstRequest::CursorNodeEnd
            | AstRequest::CursorOnEverySibling
            | AstRequest::CursorOnEveryChild => return None,
        })
    }

    /// Locates this region, or `None` when there is nowhere to go.
    ///
    /// `range` is the selection being moved and `caret` is its head. The two are
    /// used by different halves: a covering region is asked for around the whole
    /// selection, because "the function this selection is in" has to account for
    /// a selection spanning several statements; a jump travels from the caret,
    /// because a jump is a caret motion and the caret is the thing that moves.
    ///
    /// # Why an error becomes `None`
    ///
    /// [`textobject::find`] and [`textobject::jump`] are fallible, and the only
    /// failure either produces is a vendored query that will not compile against
    /// its grammar. That cannot be reached from a build whose tests pass —
    /// `every_embedded_query_compiles_against_its_grammar` in `iridium-syntax`
    /// asserts exactly this for every language and every query kind — so
    /// reaching it means a grammar dependency moved underneath the vendored
    /// `.scm` files, which is a fact about the build and not about the document.
    ///
    /// A person pressing a key can do nothing whatever about that, and putting
    /// an error path on every structural keystroke to report it would trade a
    /// quiet key for a loud one that says something unactionable. So the error
    /// folds into the same `None` that "this language has no functions" already
    /// produces: the verb does nothing.
    pub(super) fn locate(
        self,
        source: &Source<'_>,
        range: &Range<usize>,
        caret: usize,
    ) -> Option<Range<usize>> {
        let located = match self {
            Self::Covering { object, variant } => textobject::find(
                source.language,
                source.tree,
                source.text,
                range,
                object,
                variant,
            ),
            Self::Nearest { object, direction } => textobject::jump(
                source.language,
                source.tree,
                source.text,
                caret,
                object,
                direction,
            ),
        }
        .ok()
        .flatten()?;

        Some(match self {
            // `jump` answers with the whole `around` extent, and this is where
            // that becomes a caret rather than a selection. Both are defensible;
            // this set already has `ast.selectFunctionAround` for "select the
            // whole thing", so if the jump selected too there would be no verb
            // that simply *goes* somewhere, and the two would be
            // indistinguishable after a single press.
            //
            // The start, not the end, and the same for backward as for forward:
            // it puts the caret on the signature, which is where a person
            // arriving at a function wants to read from, and it makes the two
            // directions exact inverses — from the start of function N a
            // backward jump finds N-1, and a forward jump from there finds N
            // again.
            Self::Nearest { .. } => located.start..located.start,
            Self::Covering { .. } => located,
        })
    }
}

#[cfg(test)]
mod tests;
