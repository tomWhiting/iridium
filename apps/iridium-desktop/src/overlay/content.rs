//! The vocabulary a panel is composed in: rows of coloured runs, a caret,
//! an anchor, and the fit a placement affords.
//!
//! ⭐ **Nothing here knows about pixels or a GPU.** These are the types the
//! builders in [`crate::search`], [`crate::command_palette`],
//! [`crate::history_overlay`] and [`crate::file_tree`] produce and
//! [`super::paint`] consumes, which is what keeps those builders windowless
//! and testable — and what will let the explorer's composition leave this
//! face for a terminal one without the painter following it.

use iridium_editor::theme::Color;

use super::metrics::EXPLORER_MAX_VISIBLE_ROWS;

/// One line of strip content, ready to paint.
#[derive(Debug, Clone)]
pub struct StripContent {
    /// The line's text, label and field together.
    pub text: String,
    /// The caret's column in `char`s, or `None` for a strip with nothing to
    /// type into.
    pub caret_column: Option<usize>,
    /// Whether the line reports a failure, which colors it accordingly.
    pub is_error: bool,
}

/// One run of characters in one colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// The characters.
    pub text: String,
    /// Their colour.
    pub color: Color,
}

impl Span {
    /// A run of characters in one colour.
    #[must_use]
    pub fn new(text: impl Into<String>, color: Color) -> Self {
        Self {
            text: text.into(),
            color,
        }
    }
}

/// One interior row of a panel.
///
/// The spans read left to right; the painter truncates anything past the
/// panel's content width and clips the shaped text at the content area, so a
/// builder that measured correctly is drawn exactly and a builder mistake
/// costs alignment, never a panic or an overrun panel edge.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelRow {
    /// The row's text runs, left to right.
    pub spans: Vec<Span>,
    /// Whether the row is the selection, which tints its background.
    pub selected: bool,
    /// Whether the row is a group separator, drawn as a hairline rule across
    /// the panel instead of text.
    pub separator: bool,
}

impl PanelRow {
    /// A row from its runs.
    #[must_use]
    pub const fn new(spans: Vec<Span>) -> Self {
        Self {
            spans,
            selected: false,
            separator: false,
        }
    }

    /// A selection row from its runs.
    #[must_use]
    pub const fn selected(spans: Vec<Span>) -> Self {
        Self {
            spans,
            selected: true,
            separator: false,
        }
    }

    /// A separator row: a hairline rule between two groups of rows.
    #[must_use]
    pub const fn separator() -> Self {
        Self {
            spans: Vec::new(),
            selected: false,
            separator: true,
        }
    }

    /// The row's text with the colours dropped, for tests and diagnostics.
    #[must_use]
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// Where a panel's caret sits, in interior coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelCaret {
    /// The interior row, counted from the panel's first content row.
    pub row: usize,
    /// The column in `char`s, counted from the content area's left edge.
    pub column: usize,
}

/// Which window edge a panel hangs from, or which point it hangs at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelAnchor {
    /// Twelve percent down from the top, horizontally centred — the palette
    /// and the undo tree, at the web demo's anchor.
    Top,
    /// Above the bottom edge (and above the strip when one is up),
    /// horizontally centred — the search panel, which belongs near the
    /// statusline row it occupies in the terminal face.
    Bottom,
    /// Down the window's left edge, below whatever chrome the face reserved
    /// above it — the file explorer placed as a sidebar.
    ///
    /// ⭐ **The one anchor whose height is not its content's.** Every other
    /// panel is exactly as tall as the rows it composed; this one fills the
    /// band the face has reserved beside the document, because the reserve is
    /// what pushes the text and a short panel over a full-height reserve would
    /// leave the document indented past nothing. So the rows are drawn from
    /// the top and the remainder of the band is background.
    Left {
        /// The band's top edge in physical pixels — the tab strip's height.
        ///
        /// Passed rather than derived: the panel could be bottom-aligned to
        /// clear the strip without being told where it is, but then every
        /// pixel of slack would collect *above* the sidebar and it would sit
        /// flush against the window's bottom edge. The slack belongs at the
        /// bottom, which means the top has to be stated.
        top: f32,
        /// How many interior rows the band holds — `max_interior_rows` of the
        /// fit the panel was composed against, which is more than the rows it
        /// composed whenever the tree is shorter than the window.
        interior_rows: usize,
    },
    /// At an arbitrary point in the window, in physical pixels — the context
    /// menu, which hangs from the click that opened it. The point is the
    /// panel's top-left corner where the window allows it; see
    /// [`anchored_origin`] for the clamping and the flip.
    Point {
        /// The anchor's distance from the window's left edge.
        x: f32,
        /// The anchor's distance from the window's top edge.
        y: f32,
    },
}

/// A panel composed and ready to paint: rows of coloured runs on the
/// character grid, plus where its caret goes.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelContent {
    /// Which edge the panel hangs from.
    pub anchor: PanelAnchor,
    /// The characters available to each row inside the padding.
    pub content_columns: usize,
    /// The interior rows, top to bottom.
    pub rows: Vec<PanelRow>,
    /// The caret, if a field in the panel has focus.
    pub caret: Option<PanelCaret>,
}

/// What a window can honestly show of a panel, in grid units.
///
/// Builders lay their rows out against this; the painter places the result.
///
/// ⭐ **A fit describes a *placement*, not just a window.** The same window
/// yields [`PanelFit::popover`] for a floating panel and [`PanelFit::sidebar`]
/// for a full-height column down the left edge, and the two differ in every
/// field. That is why the browse ceiling is carried here rather than read from
/// a module constant by whoever composes the rows: a panel that took its width
/// from one placement and its row ceiling from another would be too tall for
/// the box it is drawn in, and nothing downstream could tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelFit {
    /// The characters available to each interior row.
    pub content_columns: usize,
    /// The most interior rows a panel may hold on this window.
    pub max_interior_rows: usize,
    /// The most rows a **browsed** list may fill at this placement, with one
    /// interior row already given up to the panel's own header.
    ///
    /// ⚠️ **Queried panels do not read this.** The command palette and the
    /// undo tree cap themselves at [`PANEL_MAX_VISIBLE_ROWS`], which is a
    /// bound on *how much of a search result is worth skimming* — a property
    /// of the question, not of the window, and so the same twelve wherever the
    /// panel is drawn. This field is the other kind of ceiling: what a list
    /// you are *reading down* is allowed to fill, which is exactly what
    /// changes when a popover becomes a sidebar. See
    /// [`EXPLORER_MAX_VISIBLE_ROWS`] for the distinction in full.
    pub max_browse_rows: usize,
}

impl PanelFit {
    /// The fit of a floating panel: the browse ceiling is
    /// [`EXPLORER_MAX_VISIBLE_ROWS`], clamped by what the window affords.
    ///
    /// The clamp is the half that protects a small window; the constant is the
    /// half that keeps a large one a popover with a window around it.
    #[must_use]
    pub const fn popover(content_columns: usize, max_interior_rows: usize) -> Self {
        let rows = Self::list_rows(max_interior_rows);
        Self {
            content_columns,
            max_interior_rows,
            max_browse_rows: if rows > EXPLORER_MAX_VISIBLE_ROWS {
                EXPLORER_MAX_VISIBLE_ROWS
            } else {
                rows
            },
        }
    }

    /// The fit of a full-height column: the browse ceiling is **everything the
    /// band holds**, with no taste bound above it.
    ///
    /// ⭐ That absence is the whole point of the placement. A sidebar that
    /// inherited the popover's thirty would stop drawing two thirds of the way
    /// down a tall window and leave the rest of its own reserved band empty —
    /// which is the defect the taller popover fixed, reintroduced one layer up.
    #[must_use]
    pub const fn sidebar(content_columns: usize, max_interior_rows: usize) -> Self {
        Self {
            content_columns,
            max_interior_rows,
            max_browse_rows: Self::list_rows(max_interior_rows),
        }
    }

    /// The interior rows left for a list once the panel's header has taken
    /// one, and never zero: a panel with nothing to list still says so on a
    /// row, and a ceiling of zero would compose an empty box instead.
    const fn list_rows(max_interior_rows: usize) -> usize {
        match max_interior_rows.saturating_sub(1) {
            0 => 1,
            rows => rows,
        }
    }
}
