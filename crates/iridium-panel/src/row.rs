//! The vocabulary a panel is composed in: rows of coloured runs, a caret,
//! and the fit a placement affords.
//!
//! ⭐ **No pixels, no cells, no device.** A GPU face measures these in
//! character widths and a terminal face in grid cells, and neither number
//! belongs here — what belongs here is the count of characters a row holds
//! and the colour each run is drawn in, which both faces agree on.

use iridium_editor::theme::Color;

/// The most content rows a panel's list shows at once, matching the terminal
/// face's `MAX_VISIBLE_ROWS`.
///
/// This is the cap for a panel that *answers a question* — the command palette
/// and the undo tree, where the row wanted is nearly always in the first few
/// and a longer list is further to read rather than more to see. A panel that
/// is **browsed** wants a different number; see
/// [`EXPLORER_MAX_VISIBLE_ROWS`].
pub const PANEL_MAX_VISIBLE_ROWS: usize = 12;

/// The most content rows the file explorer shows at once.
///
/// ⭐ **Separate from [`PANEL_MAX_VISIBLE_ROWS`] because the two panels are
/// used differently, not because one number was wrong.** The palette is
/// queried: you type three characters and take the top row, and a thirty-row
/// palette is thirty rows to skim past. The explorer is *browsed* — Tom's
/// words on 12 Aug 2026 were that it "probably also needs to be taller as
/// well" — and a browser that shows a dozen entries of a directory holding
/// sixty is a browser that hides most of what you opened it to look at.
///
/// **Twelve was leaving most of the window unused, and that was measured
/// rather than assumed.** [`fit_for`] gives a panel `1 - TOP_ANCHOR_FRACTION`
/// of the window height less its padding, and a row costs
/// `font_size × line_height` = `14 × 1.4` logical pixels. A 1440×900 laptop
/// therefore affords **39** interior rows, and the panel was drawing twelve of
/// them.
///
/// Thirty rather than "as many as fit", deliberately. `fit.max_interior_rows`
/// already clamps this on any window too short for it — that is what protects
/// a small window, and it means this constant is only ever an upper *taste*
/// bound. What it buys is that the panel stays a popover with a window around
/// it on a large display instead of silently becoming a full-height column.
/// The full-height column is a real thing Tom asked for, and it is the
/// **sidebar**, which is a placement rather than a bigger number.
pub const EXPLORER_MAX_VISIBLE_ROWS: usize = 30;

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

/// A panel composed down to what every face agrees on: its rows, the width
/// they were laid out against, and where the caret went.
///
/// ⭐ **This is the whole of the seam between a shared panel and the face that
/// paints it.** The desktop's `PanelContent` is this plus an *anchor* — which
/// edge of a window the panel hangs from — and a *hovered* row, and neither is
/// a statement a shared builder can make: an anchor needs a window, a hover
/// needs a pointer, and the terminal face has a different answer to the first
/// and may have none to the second.
///
/// So a builder that has to exist in both faces composes one of these, and the
/// face wraps it. The face-specific builders — the palette, the search bar,
/// the undo tree, the context menu — keep building the face's own type
/// directly, because they have no second face to agree with. The asymmetry is
/// the point rather than an oversight: what crosses is what has to.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelBody {
    /// The characters available to each interior row — the
    /// [`PanelFit::content_columns`] the rows were composed against, carried
    /// out so the face never has to remember which fit it passed in.
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
