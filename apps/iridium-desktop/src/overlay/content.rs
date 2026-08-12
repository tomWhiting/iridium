//! The vocabulary a panel is composed in: rows of coloured runs, a caret,
//! an anchor, and the fit a placement affords.
//!
//! ⭐ **Nothing here knows about pixels or a GPU.** These are the types the
//! builders in [`crate::search`], [`crate::command_palette`],
//! [`crate::history_overlay`] and [`crate::file_tree`] produce and
//! [`super::paint`] consumes, which is what keeps those builders windowless
//! and testable — and what will let the explorer's composition leave this
//! face for a terminal one without the painter following it.

use iridium_panel::{PanelCaret, PanelRow};

/// Which of this face's panels a composed [`PanelContent`] is.
///
/// ⭐ **A press has to resolve to a panel, not to a boolean.** The painter
/// records where each panel of the last frame landed, and until this existed
/// the only thing that record could answer was *whether* the pointer was on
/// some panel — which is enough to stop a click reaching the document and not
/// enough to give it to anything. The kind travels with the content into
/// [`OverlayPainter::paint`](super::OverlayPainter::paint) and comes back out
/// in the frame's record, so what the pointer resolves to and what was drawn
/// cannot be two different answers.
///
/// Not in `iridium-panel`: this is the list of panels *this face* has, and the
/// terminal face's list is its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    /// The find-and-replace panel — two fields, no list.
    Search,
    /// The undo tree.
    History,
    /// The file explorer, in either placement.
    Explorer,
    /// The command palette.
    Palette,
    /// The right-click context menu.
    Menu,
}

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
    /// The interior row the pointer is over, drawn as a fainter band than the
    /// selection.
    ///
    /// ⚠️ **Deliberately not [`PanelRow::selected`], and the two must not be
    /// collapsed.** The selection is what `Enter` acts on; the hover is where
    /// the mouse happens to be. Merging them would mean dragging the pointer
    /// across a sidebar silently changed what the next keystroke opened —
    /// which is the behaviour of a menu, where hover *is* the selection,
    /// and wrong everywhere else.
    ///
    /// Set by the face after the panel composed itself, exactly as the
    /// sidebar's anchor is: a builder has no idea where a pointer is, and
    /// giving one an argument for it would put a mouse in every panel's
    /// signature.
    pub hovered: Option<usize>,
}
