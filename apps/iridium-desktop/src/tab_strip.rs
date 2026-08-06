//! The tab strip: the docked band along the window's top edge.
//!
//! Everything here is a pure function of the window, the text grid and the
//! open tabs. The painter in [`crate::overlay`] turns the result into
//! instances; [`crate::app`] hit-tests a press against the same result. That
//! split is the whole point of the module.
//!
//! # Why one layout, read twice
//!
//! A strip drawn from one placement and clicked against another agrees
//! wherever the two happen to coincide — which, for a strip laid out left to
//! right, is the first tab and nothing else. The user clicks a tab and a
//! different one comes forward, reliably, but only once enough files are
//! open. [`tab_strip_layout`] is therefore the only thing that decides where
//! a tab is, and both the painter and [`TabStripLayout::hit`] read what it
//! decided rather than recomputing it.
//!
//! # The character grid
//!
//! Horizontal placement is in whole character cells — padding, the gap
//! between tabs, the marker column, all of them. That is not thrift: the
//! strip's text is shaped as **one** buffer and drawn at one origin, so every
//! tab's glyphs land where its rectangle says they do only if the rectangles
//! fall on the same grid the glyphs advance along. A pixel gap between tabs
//! would put the label of tab three half a cell left of its card.
//!
//! Vertical placement is in logical pixels times the window's scale factor,
//! like the rest of the chrome — there is only one row, so nothing has to
//! line up with anything.
//!
//! # Overflow
//!
//! More tabs than fit scroll rather than shrink: the strip is offset by whole
//! cells, just enough to keep the active tab on screen, exactly as
//! [`crate::overlay::scroll_for`] keeps a prompt field's caret on screen. A
//! shrinking strip would make every tab's width depend on how many other
//! files are open, so the tab under the pointer would move when an unrelated
//! file opened.
//!
//! # The shape
//!
//! The band is edge to edge and has no free corners to round, like the prompt
//! strip at the other end of the window; it gets an honest opaque background
//! and a hairline along its lower edge. The **active** tab is a floating card
//! with genuine arcs on all four corners — Tom's standing rule, and the one
//! piece of the strip that has corners at all. Inactive tabs are label and
//! marker on the band itself, so the card is the only thing the eye has to
//! find.

use iridium_editor::theme::Color;

use crate::overlay::{GridMetrics, PanelRect, Span};
use crate::units::{index_to_f32, pixels_to_cells};

/// Vertical padding above and below the strip's row of text, in logical
/// pixels — the prompt strip's [`crate::overlay`] value, so the two docked
/// bands are the same height.
const TAB_PAD_Y: f32 = 6.0;

/// How far the active tab's card is inset from the band's top and bottom
/// edges, in logical pixels, so it reads as a card resting on the band rather
/// than as a second band.
const CARD_INSET_Y: f32 = 3.0;

/// The active card's corner radius in logical pixels — the panels'
/// `PANEL_RADIUS` less a step, since the card is the smaller shape.
const CARD_RADIUS: f32 = 6.0;

/// Cells of padding inside a tab, before its label and after its marker.
const PAD_COLUMNS: usize = 1;

/// Cells of band left between two adjacent tabs.
const GAP_COLUMNS: usize = 1;

/// The longest label a tab shows, in cells. A longer one is cut and ends in
/// an ellipsis, so the cut is visible rather than looking like a file whose
/// name happens to end there.
const MAX_LABEL_COLUMNS: usize = 20;

/// The character standing in for a cut label's missing tail.
const ELLIPSIS: char = '…';

/// The marker on a tab whose document differs from what is on disk.
const DIRTY_MARKER: char = '●';

/// The marker on a saved tab: the close control.
const CLOSE_MARKER: char = '×';

/// How wide the close control's hit box is, in cells, centred on the marker.
///
/// The marker itself is one cell — around eight logical pixels — which is
/// too small a target to ask anyone to hit. Two cells is close to the twelve
/// points macOS gives a window button, and it still fits inside the tab: the
/// marker sits one padding cell in from the right edge, so a box of two cells
/// centred on it reaches from half a cell before the marker to half a cell
/// after the padding, and no further.
const CLOSE_COLUMNS: f32 = 2.0;

/// The cells a tab occupies besides its label: a padding cell each side, the
/// separating space, and the marker.
const TAB_FIXED_COLUMNS: usize = PAD_COLUMNS * 2 + 2;

/// One tab as the strip will draw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabItem {
    /// The tab's label, usually a file name.
    pub label: String,
    /// Whether this is the tab in front.
    pub is_active: bool,
    /// Whether its document differs from what is on disk.
    pub is_dirty: bool,
}

/// Every open tab, in the order the strip draws them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TabStripContent {
    /// The tabs, left to right.
    pub tabs: Vec<TabItem>,
}

/// The four colours the strip's text is drawn in.
///
/// Derived from the theme by the painter and passed in, so this module stays
/// a placement statement and the theme stays the painter's business.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabColors {
    /// The active tab's label.
    pub active: Color,
    /// Every other tab's label.
    pub inactive: Color,
    /// A saved tab's close marker.
    pub close: Color,
    /// An unsaved tab's dot.
    pub dirty: Color,
}

/// What a press on the strip means.
///
/// The index is into [`TabStripContent::tabs`], which the face built from the
/// workspace's own display order, so it maps straight back to a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabHit {
    /// Bring this tab to the front.
    Activate(usize),
    /// Close it — through the face's close path, so unsaved work is still
    /// asked about.
    Close(usize),
}

/// Where one tab landed, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabPlacement {
    /// The tab's whole extent, which is what a press is resolved against.
    /// Square by construction: it is a hit region, never drawn.
    pub bounds: PanelRect,
    /// The card drawn behind the tab when it is the active one — the bounds
    /// inset vertically and given genuine arcs.
    pub card: PanelRect,
    /// The close control's hit box, tested before [`Self::bounds`].
    pub close: PanelRect,
}

/// Where the whole strip landed, in physical pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct TabStripLayout {
    /// The band: full width, flush to the top edge, no corners to round.
    pub band: PanelRect,
    /// The tabs, in content order. A tab scrolled off the edge keeps its
    /// entry — with an origin outside the window — so the indices never shift
    /// under the caller.
    pub tabs: Vec<TabPlacement>,
    /// Where cell zero of the strip's single shaped line is drawn. Negative
    /// once the strip has scrolled; the painter clips at the window edge.
    pub text_x: f32,
    /// The text's baseline origin — the band's top edge plus its padding.
    pub text_y: f32,
    /// Whole cells the strip is scrolled by, kept for the tests that pin the
    /// active tab's visibility.
    pub scroll_columns: usize,
}

impl TabStripLayout {
    /// What a press at a physical pixel means, or `None` for a pixel off the
    /// strip or on the band between tabs.
    ///
    /// The close control is tested first because it lies inside its tab: a
    /// bounds-first test would swallow every close.
    #[must_use]
    pub fn hit(&self, x: f32, y: f32) -> Option<TabHit> {
        if !self.band.contains(x, y) {
            return None;
        }
        for (index, tab) in self.tabs.iter().enumerate() {
            if tab.close.contains(x, y) {
                return Some(TabHit::Close(index));
            }
            if tab.bounds.contains(x, y) {
                return Some(TabHit::Activate(index));
            }
        }
        None
    }

    /// Whether a physical pixel is anywhere on the strip.
    ///
    /// Distinct from [`Self::hit`] returning `Some`: a press on the band
    /// between two tabs means nothing, but it is still a press on chrome and
    /// must not reach the document underneath.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.band.contains(x, y)
    }
}

/// The label a tab actually shows: its own, or its first cells and an
/// ellipsis.
///
/// Counted and cut in `char`s, never bytes, so a name with an accent in it is
/// cut on a character boundary. The cut keeps [`MAX_LABEL_COLUMNS`] cells
/// including the ellipsis, so every over-long tab is exactly the same width.
fn shaped_label(label: &str) -> String {
    let count = label.chars().count();
    if count <= MAX_LABEL_COLUMNS {
        return label.to_owned();
    }
    let mut shaped: String = label.chars().take(MAX_LABEL_COLUMNS - 1).collect();
    shaped.push(ELLIPSIS);
    shaped
}

/// The cells a tab occupies, label included.
fn tab_columns(label: &str) -> usize {
    shaped_label(label).chars().count() + TAB_FIXED_COLUMNS
}

/// Where each tab starts, and how wide it is, in cells — the layout before
/// any scrolling, which is what the scroll is then chosen against.
fn tab_extents(content: &TabStripContent) -> Vec<(usize, usize)> {
    let mut extents = Vec::with_capacity(content.tabs.len());
    let mut cursor: usize = 0;
    for tab in &content.tabs {
        let width = tab_columns(&tab.label);
        extents.push((cursor, width));
        cursor = cursor.saturating_add(width).saturating_add(GAP_COLUMNS);
    }
    extents
}

/// The first cell of the strip that is shown, so the active tab is on screen.
///
/// Zero while everything fits. Otherwise the smallest scroll that brings the
/// active tab's right edge inside the window, then pulled back if that pushed
/// its left edge off — which is what happens when one tab is wider than the
/// whole window, and showing its start is the useful half.
fn scroll_columns(extents: &[(usize, usize)], active: Option<usize>, visible: usize) -> usize {
    let Some((start, width)) = active.and_then(|index| extents.get(index).copied()) else {
        return 0;
    };
    let end = start.saturating_add(width);
    let mut scroll = end.saturating_sub(visible);
    if start < scroll {
        scroll = start;
    }
    scroll
}

/// The pixels of window the strip occupies, or zero on a window or a grid
/// that cannot honestly hold one.
///
/// The face reserves exactly this much above the document, through
/// [`FrameCompositor::set_top_inset`](iridium_editor::render::FrameCompositor::set_top_inset),
/// so it has to be the number [`tab_strip_layout`] itself uses. Written once
/// and called twice rather than stated twice, and for two reasons: a reserve
/// that disagreed with the band by a few pixels would leave the first line of
/// the document permanently half-covered, which reads as a font bug — and a
/// reserve that survived a window the band declined to draw on would push the
/// document down under nothing at all.
#[must_use]
pub fn tab_strip_height(window_height: f32, metrics: GridMetrics) -> f32 {
    if metrics.is_degenerate() {
        return 0.0;
    }
    let height = 2.0_f32.mul_add(TAB_PAD_Y * metrics.scale, metrics.line_height);
    if height > window_height { 0.0 } else { height }
}

/// Places the strip on a `window_width × window_height` window, or `None`
/// when there is nothing to place or no honest room for it.
///
/// All arguments and results are physical pixels; the metrics' scale converts
/// the chrome's logical measurements.
#[must_use]
pub fn tab_strip_layout(
    window_width: f32,
    window_height: f32,
    metrics: GridMetrics,
    content: &TabStripContent,
) -> Option<TabStripLayout> {
    if content.tabs.is_empty() || metrics.is_degenerate() {
        return None;
    }
    let pad_y = TAB_PAD_Y * metrics.scale;
    let height = tab_strip_height(window_height, metrics);
    if height <= 0.0 {
        return None;
    }
    let visible = pixels_to_cells(window_width, metrics.char_width);
    if visible == 0 {
        return None;
    }

    let extents = tab_extents(content);
    let active = content.tabs.iter().position(|tab| tab.is_active);
    let scroll = scroll_columns(&extents, active, visible);
    let origin = -(index_to_f32(scroll) * metrics.char_width);

    let inset = (CARD_INSET_Y * metrics.scale).min(height / 2.0);
    let card_height = 2.0_f32.mul_add(-inset, height);
    let tabs = extents
        .iter()
        .map(|&(start, width)| {
            let x = index_to_f32(start).mul_add(metrics.char_width, origin);
            let tab_width = index_to_f32(width) * metrics.char_width;
            // The marker sits one padding cell in from the tab's right edge;
            // the close box is centred on it.
            let marker_columns = index_to_f32(width.saturating_sub(PAD_COLUMNS + 1));
            let close_width = CLOSE_COLUMNS * metrics.char_width;
            let close_x = marker_columns.mul_add(metrics.char_width, x)
                - (close_width - metrics.char_width) / 2.0;
            let radius = (CARD_RADIUS * metrics.scale)
                .min(tab_width / 2.0)
                .min(card_height / 2.0);
            TabPlacement {
                bounds: PanelRect {
                    x,
                    y: 0.0,
                    width: tab_width,
                    height,
                    radius: 0.0,
                },
                card: PanelRect {
                    x,
                    y: inset,
                    width: tab_width,
                    height: card_height,
                    radius,
                },
                close: PanelRect {
                    x: close_x,
                    y: inset,
                    width: close_width,
                    height: card_height,
                    radius: 0.0,
                },
            }
        })
        .collect();

    Some(TabStripLayout {
        band: PanelRect {
            x: 0.0,
            y: 0.0,
            width: window_width,
            height,
            radius: 0.0,
        },
        tabs,
        text_x: origin,
        text_y: pad_y,
        scroll_columns: scroll,
    })
}

/// The strip's single line of text, as coloured runs.
///
/// Every cell the layout accounted for is emitted — the padding cells and the
/// gaps as spaces — so the run list and [`tab_strip_layout`] describe the same
/// row. A run list that skipped the padding would draw every label one cell
/// left of its card.
#[must_use]
pub fn tab_strip_spans(content: &TabStripContent, colors: TabColors) -> Vec<Span> {
    let pad: String = " ".repeat(PAD_COLUMNS);
    let gap: String = " ".repeat(GAP_COLUMNS);
    let mut spans = Vec::with_capacity(content.tabs.len() * 4);
    for (index, tab) in content.tabs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::new(gap.clone(), colors.inactive));
        }
        let label = if tab.is_active {
            colors.active
        } else {
            colors.inactive
        };
        let (marker, marker_color) = if tab.is_dirty {
            (DIRTY_MARKER, colors.dirty)
        } else {
            (CLOSE_MARKER, colors.close)
        };
        spans.push(Span::new(pad.clone(), label));
        spans.push(Span::new(shaped_label(&tab.label), label));
        spans.push(Span::new(" ", label));
        spans.push(Span::new(marker.to_string(), marker_color));
        spans.push(Span::new(pad.clone(), label));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::{
        CLOSE_MARKER, DIRTY_MARKER, ELLIPSIS, GridMetrics, MAX_LABEL_COLUMNS, TabColors, TabHit,
        TabItem, TabStripContent, TabStripLayout, tab_columns, tab_strip_layout, tab_strip_spans,
    };
    use iridium_editor::theme::Color;

    /// The working grid of a 14 px face on a 2× display, as the overlay tests
    /// use.
    const CHAR_WIDTH: f32 = 16.8;
    const LINE_HEIGHT: f32 = 39.2;
    const SCALE: f32 = 2.0;
    const WINDOW: (f32, f32) = (3024.0, 1964.0);

    const METRICS: GridMetrics = GridMetrics {
        char_width: CHAR_WIDTH,
        line_height: LINE_HEIGHT,
        scale: SCALE,
    };

    const COLORS: TabColors = TabColors {
        active: Color::new(1.0, 1.0, 1.0, 1.0),
        inactive: Color::new(0.6, 0.6, 0.6, 1.0),
        close: Color::new(0.5, 0.5, 0.5, 1.0),
        dirty: Color::new(0.9, 0.7, 0.2, 1.0),
    };

    /// Tabs from labels, with `active` in front and every other one saved.
    fn content(labels: &[&str], active: usize) -> TabStripContent {
        TabStripContent {
            tabs: labels
                .iter()
                .enumerate()
                .map(|(index, label)| TabItem {
                    label: (*label).to_owned(),
                    is_active: index == active,
                    is_dirty: false,
                })
                .collect(),
        }
    }

    fn placed(content: &TabStripContent) -> TabStripLayout {
        tab_strip_layout(WINDOW.0, WINDOW.1, METRICS, content)
            .expect("the standard window holds a strip")
    }

    /// **The round trip.** Every tab the layout placed answers to a press in
    /// its own label, and to a press on its own close control — which is the
    /// assertion a painter-and-hit-test pair drifting apart cannot pass, and
    /// the reason both read one layout.
    #[test]
    fn a_press_on_a_tab_resolves_to_that_tab() {
        let tabs = content(&["main.rs", "Cargo.toml", "a-longer-name.json", "x"], 2);
        let layout = placed(&tabs);
        let y = layout.band.height / 2.0;

        for index in 0..tabs.tabs.len() {
            let tab = layout.tabs[index];
            // The middle of the label's first cell: inside the tab, and never
            // inside the close control however short the label is.
            let label_x = 1.5_f32.mul_add(CHAR_WIDTH, tab.bounds.x);
            assert_eq!(
                layout.hit(label_x, y),
                Some(TabHit::Activate(index)),
                "a press on tab {index}'s label"
            );

            let close_x = tab.close.x + tab.close.width / 2.0;
            assert_eq!(
                layout.hit(close_x, y),
                Some(TabHit::Close(index)),
                "a press on tab {index}'s close control"
            );
        }
    }

    /// The close control lies inside its tab and clear of its neighbours, so
    /// closing one tab can never be a press on the next.
    #[test]
    fn every_close_control_stays_inside_its_own_tab() {
        let tabs = content(&["a", "bb", "ccc.rs", "a-much-longer-name.json"], 0);
        let layout = placed(&tabs);
        for (index, tab) in layout.tabs.iter().enumerate() {
            assert!(
                tab.close.x >= tab.bounds.x - 0.001,
                "tab {index}'s close control started before its tab"
            );
            assert!(
                tab.close.x + tab.close.width <= tab.bounds.x + tab.bounds.width + 0.001,
                "tab {index}'s close control ran past its tab"
            );
        }
    }

    #[test]
    fn tabs_are_laid_out_left_to_right_without_overlapping() {
        let layout = placed(&content(&["one", "two", "three", "four"], 0));
        for pair in layout.tabs.windows(2) {
            let [left, right] = pair else {
                continue;
            };
            assert!(
                left.bounds.x + left.bounds.width <= right.bounds.x + 0.001,
                "two tabs overlapped"
            );
        }
    }

    /// A press on the band between two tabs is on the strip but on no tab: it
    /// must not activate one, and it must not reach the document either.
    #[test]
    fn the_gap_between_tabs_belongs_to_neither() {
        let layout = placed(&content(&["one", "two"], 0));
        let first = layout.tabs[0];
        let gap_x = first.bounds.x + first.bounds.width + CHAR_WIDTH / 2.0;
        let y = layout.band.height / 2.0;

        assert_eq!(layout.hit(gap_x, y), None);
        assert!(layout.contains(gap_x, y), "the gap is still chrome");
    }

    #[test]
    fn a_pixel_below_the_band_is_not_on_the_strip() {
        let layout = placed(&content(&["one", "two"], 0));
        let below = layout.band.height + 1.0;
        assert!(!layout.contains(layout.tabs[0].bounds.x + 1.0, below));
        assert_eq!(layout.hit(layout.tabs[0].bounds.x + 1.0, below), None);
    }

    /// The overflow rule: however many files are open, the tab in front is on
    /// screen — otherwise walking the strip with `⌘⇧]` silently leaves the
    /// window behind.
    #[test]
    fn the_active_tab_is_visible_however_many_are_open() {
        let labels: Vec<String> = (0..80).map(|index| format!("file-{index}.rs")).collect();
        let borrowed: Vec<&str> = labels.iter().map(String::as_str).collect();

        for active in [0_usize, 1, 17, 40, 78, 79] {
            let tabs = content(&borrowed, active);
            let layout = placed(&tabs);
            let tab = layout.tabs[active];
            // Half a pixel of slack: the eightieth tab's origin is a product
            // of order 2×10⁴, where an `f32` is worth a couple of
            // thousandths, and "on screen" is not a claim about float bits.
            assert!(
                tab.bounds.x >= -0.5,
                "the active tab started off the left edge with {active} in front"
            );
            assert!(
                tab.bounds.x + tab.bounds.width <= WINDOW.0 + 0.5,
                "the active tab ran off the right edge with {active} in front"
            );
            assert_eq!(
                layout.hit(
                    1.5_f32.mul_add(CHAR_WIDTH, tab.bounds.x),
                    layout.band.height / 2.0
                ),
                Some(TabHit::Activate(active)),
                "the visible active tab did not answer a press"
            );
        }
    }

    /// A window that fits everything never scrolls, so the first tab stays at
    /// the left edge rather than drifting.
    #[test]
    fn a_strip_that_fits_does_not_scroll() {
        let layout = placed(&content(&["one", "two", "three"], 2));
        assert_eq!(layout.scroll_columns, 0);
        assert!((layout.text_x - 0.0).abs() < f32::EPSILON);
        assert!(layout.tabs[0].bounds.x.abs() < f32::EPSILON);
    }

    /// One tab wider than the window shows its start — the end of a name is
    /// no use without the beginning.
    #[test]
    fn a_tab_wider_than_the_window_shows_its_start() {
        let tabs = content(&["a-name-that-is-long.rs"], 0);
        let narrow = 6.0 * CHAR_WIDTH;
        let layout = tab_strip_layout(narrow, WINDOW.1, METRICS, &tabs)
            .expect("a narrow window still holds a strip");
        assert_eq!(layout.scroll_columns, 0);
        assert!(layout.tabs[0].bounds.x.abs() < f32::EPSILON);
    }

    /// Tom's standing rule, pinned: the one shape on the strip that has
    /// corners keeps genuine arcs at every scale a display reports.
    #[test]
    fn the_active_card_keeps_a_genuine_arc() {
        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            let metrics = GridMetrics {
                char_width: CHAR_WIDTH / SCALE * scale,
                line_height: LINE_HEIGHT / SCALE * scale,
                scale,
            };
            let layout = tab_strip_layout(WINDOW.0, WINDOW.1, metrics, &content(&["main.rs"], 0))
                .expect("every scale holds a strip");
            assert!(
                layout.tabs[0].card.radius > 0.0,
                "a tab corner went sharp at scale {scale}"
            );
            assert!(layout.tabs[0].card.height > 0.0);
        }
    }

    #[test]
    fn an_empty_workspace_places_no_strip() {
        assert!(
            tab_strip_layout(
                WINDOW.0,
                WINDOW.1,
                METRICS,
                &TabStripContent { tabs: Vec::new() }
            )
            .is_none()
        );
    }

    #[test]
    fn an_unmeasured_font_places_no_strip() {
        let unmeasured = GridMetrics {
            char_width: 0.0,
            ..METRICS
        };
        assert!(tab_strip_layout(WINDOW.0, WINDOW.1, unmeasured, &content(&["a"], 0)).is_none());
        assert!(tab_strip_layout(WINDOW.0, 10.0, METRICS, &content(&["a"], 0)).is_none());
    }

    /// **The reserve and the band decline together.**
    ///
    /// The face pushes the document down by
    /// [`tab_strip_height`] and the painter draws the band from
    /// [`tab_strip_layout`]. If the two disagreed about when a window is too
    /// short, a shallow window would push its document down by a band that
    /// was never drawn — a stripe of empty page above the first line, and no
    /// chrome to explain it.
    #[test]
    fn a_window_that_gets_no_band_is_charged_nothing_for_one() {
        let tabs = content(&["a.rs"], 0);
        for window_height in [0.0_f32, 10.0, 40.0, 62.0, 64.0, 200.0, WINDOW.1] {
            let reserved = super::tab_strip_height(window_height, METRICS);
            let drawn = tab_strip_layout(WINDOW.0, window_height, METRICS, &tabs);
            assert_eq!(
                reserved > 0.0,
                drawn.is_some(),
                "at {window_height} px the reserve and the band disagree"
            );
            if let Some(layout) = drawn {
                assert!(
                    (layout.band.height - reserved).abs() < f32::EPSILON,
                    "at {window_height} px the band is not the height reserved for it"
                );
            }
        }

        let unmeasured = GridMetrics {
            line_height: 0.0,
            ..METRICS
        };
        assert!((super::tab_strip_height(WINDOW.1, unmeasured) - 0.0).abs() < f32::EPSILON);
    }

    /// **The other half of the round trip.** The shaped row and the placed
    /// rectangles describe the same cells.
    ///
    /// The strip is drawn as one buffer at one origin, so a run list that
    /// emitted one cell fewer than the layout charged would slide every label
    /// after it left of its own card — which looks like a font problem and is
    /// an arithmetic one. Cell indices are read back by rounding rather than
    /// flooring: at cell 1200 an `f32` product is worth a couple of
    /// thousandths of a pixel either way, and a floor would occasionally
    /// answer one cell early.
    #[test]
    fn the_run_list_fills_exactly_the_cells_the_layout_charged() {
        let labels = ["main.rs", "Cargo.toml", "x", "a-really-long-name.json"];
        let tabs = content(&labels, 1);
        let layout = placed(&tabs);
        let text: String = tab_strip_spans(&tabs, COLORS)
            .iter()
            .map(|span| span.text.as_str())
            .collect();
        let cells: Vec<char> = text.chars().collect();

        let charged: usize = labels.iter().map(|label| tab_columns(label)).sum::<usize>()
            + super::GAP_COLUMNS * (labels.len() - 1);
        assert_eq!(
            cells.len(),
            charged,
            "the shaped row and the placed tabs disagree about the strip's width"
        );

        for (index, tab) in layout.tabs.iter().enumerate() {
            // Half a cell of nudge before the floor, which is a round.
            let start = super::pixels_to_cells(CHAR_WIDTH.mul_add(0.5, tab.bounds.x), CHAR_WIDTH);
            let label_cell = super::PAD_COLUMNS + start;
            let expected = super::shaped_label(&tabs.tabs[index].label)
                .chars()
                .next()
                .unwrap_or(' ');
            assert_eq!(
                cells[label_cell], expected,
                "tab {index}'s label does not start where its rectangle does"
            );
        }
    }

    #[test]
    fn an_unsaved_tab_shows_a_dot_and_a_saved_one_shows_a_close() {
        let tabs = TabStripContent {
            tabs: vec![
                TabItem {
                    label: "clean.rs".to_owned(),
                    is_active: true,
                    is_dirty: false,
                },
                TabItem {
                    label: "dirty.rs".to_owned(),
                    is_active: false,
                    is_dirty: true,
                },
            ],
        };
        let text: String = tab_strip_spans(&tabs, COLORS)
            .iter()
            .map(|span| span.text.as_str())
            .collect();

        assert!(text.contains(CLOSE_MARKER));
        assert!(text.contains(DIRTY_MARKER));
    }

    /// An over-long name is cut to exactly the budget, ellipsis included, so
    /// every over-long tab is the same width and the cut is visible.
    #[test]
    fn an_overlong_label_is_cut_to_the_budget_with_a_visible_ellipsis() {
        let long = "a-really-quite-long-file-name-indeed.json";
        assert!(long.chars().count() > MAX_LABEL_COLUMNS);

        let shaped = super::shaped_label(long);
        assert_eq!(shaped.chars().count(), MAX_LABEL_COLUMNS);
        assert_eq!(shaped.chars().last(), Some(ELLIPSIS));
        assert_eq!(
            tab_columns(long),
            tab_columns(&"x".repeat(MAX_LABEL_COLUMNS)),
            "two over-long tabs came out different widths"
        );
    }

    /// Cutting counts characters, never bytes: a name of accented characters
    /// must not panic or split one.
    #[test]
    fn a_label_of_multibyte_characters_is_cut_on_a_character_boundary() {
        let long = "é".repeat(60);
        let shaped = super::shaped_label(&long);
        assert_eq!(shaped.chars().count(), MAX_LABEL_COLUMNS);
        assert!(shaped.starts_with('é'));
    }
}
