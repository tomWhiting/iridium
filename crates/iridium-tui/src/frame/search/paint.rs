//! Drawing the search overlay's panel.
//!
//! The panel is one or two rows immediately above the statusline:
//!
//! ```text
//!  Find    foo                       [Aa]  \b   .*    3 of 17
//!  Replace bar                                Replaced 4 matches
//! ```
//!
//! # Every part of it degrades rather than colliding
//!
//! A terminal can be any width, and a panel that let two segments overwrite
//! each other would show a match count with a query spliced through it — which
//! tells the reader neither. So the field keeps a floor of [`MIN_FIELD_WIDTH`]
//! cells and the right-hand segments are dropped, widest group first, until
//! what is left fits. The one exception is the feedback message, which is
//! *clipped* rather than dropped: the beginning of a regex error is worth more
//! than nothing at all, and it is the only segment whose length the user
//! controls.
//!
//! When only one row is available the panel shows whichever field has focus,
//! with the toggles and the count beside it. A field that cannot be seen must
//! not be the one keystrokes are going into.
//!
//! # A toggle says what it is in text, not only in colour
//!
//! An active toggle is bracketed — `[Aa]` — and an inactive one is not. The
//! style differs too, but a terminal degraded to sixteen colours, or a user who
//! cannot distinguish them, would otherwise have no way to tell whether
//! case-sensitivity was on.

use iridium_editor::Editor;
use iridium_editor::search::SearchOptions;

use super::{Feedback, Focus, SearchOverlay};
use crate::cell::{CellBuffer, Style};
use crate::frame::CellPosition;
use crate::frame::field::scroll_for;
use crate::frame::line::LineLayout;
use crate::frame::palette::Palette;
use crate::frame::text::{self, TextArea};

/// The width of the label column, which both labels are padded to.
const LABEL_WIDTH: usize = 9;

/// The label of the query field, padded to [`LABEL_WIDTH`].
const FIND_LABEL: &str = " Find    ";

/// The label of the replacement field, padded to [`LABEL_WIDTH`].
const REPLACE_LABEL: &str = " Replace ";

/// The narrowest field worth showing. Below this the right-hand segments go.
const MIN_FIELD_WIDTH: usize = 8;

/// The cells one toggle occupies: two of label between two of bracket.
const TOGGLE_WIDTH: usize = 4;

/// The cells all three toggles occupy, with one blank between each pair.
const TOGGLES_WIDTH: usize = TOGGLE_WIDTH * 3 + 2;

/// The blank cells between the toggles and the match count.
const COUNT_GAP: usize = 2;

/// The blank cell kept at the right edge, so nothing touches the frame.
const RIGHT_MARGIN: usize = 1;

/// The blank cell kept between the field and whatever is to its right.
const FIELD_GAP: usize = 1;

/// What the right-hand end of a panel row carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RightSide {
    /// The option toggles and the match count.
    Counters,
    /// The feedback message from the last action.
    Feedback,
}

/// Paints the panel into `rows` and returns where the caret belongs.
///
/// The rows are the ones [`SearchOverlay::rows`](super::SearchOverlay::rows)
/// sized. `None` comes back when there is no room to show the focused field at
/// all, and then the caret belongs nowhere: a cursor parked on a field the user
/// cannot see would say typing goes somewhere it does not.
pub(super) fn paint(
    buffer: &mut CellBuffer,
    first_row: usize,
    row_count: usize,
    panel: Panel<'_>,
) -> Option<CellPosition> {
    if row_count == 0 || buffer.width() == 0 {
        return None;
    }

    if row_count == 1 {
        let focused = panel.overlay.focus;
        return paint_row(buffer, first_row, focused, RightSide::Counters, panel);
    }

    let find = paint_row(buffer, first_row, Focus::Find, RightSide::Counters, panel);
    let replace = paint_row(
        buffer,
        first_row + 1,
        Focus::Replace,
        RightSide::Feedback,
        panel,
    );
    match panel.overlay.focus {
        Focus::Find => find,
        Focus::Replace => replace,
    }
}

/// Everything one panel is drawn from.
///
/// The overlay, the kernel and the theme travel together because every row
/// needs all three, and passing them as three positional references is how a
/// caller ends up drawing one frame's panel from another frame's editor.
#[derive(Clone, Copy)]
pub(super) struct Panel<'a> {
    /// The overlay's own state: the fields, the focus, the options.
    pub(super) overlay: &'a SearchOverlay,
    /// The kernel, read for the match count and the current match.
    pub(super) editor: &'a Editor,
    /// The resolved theme.
    pub(super) palette: &'a Palette,
}

/// Paints one panel row and returns where its field's caret landed.
fn paint_row(
    buffer: &mut CellBuffer,
    row: usize,
    field: Focus,
    right: RightSide,
    panel: Panel<'_>,
) -> Option<CellPosition> {
    let width = buffer.width();
    if row >= buffer.height() {
        return None;
    }
    let base = panel.palette.overlay();
    blank(buffer, row, 0..width, base);

    let label = match field {
        Focus::Find => FIND_LABEL,
        Focus::Replace => REPLACE_LABEL,
    };
    buffer.set_str(0, row, label, panel.palette.overlay_quiet());

    let field_width = paint_right_side(buffer, row, right, panel);
    let text = match field {
        Focus::Find => &panel.overlay.query,
        Focus::Replace => &panel.overlay.replacement,
    };

    if field_width == 0 || LABEL_WIDTH >= width {
        return None;
    }
    let caret = text.caret_cell();
    let area = TextArea {
        origin: LABEL_WIDTH,
        width: field_width,
        scroll: scroll_for(caret, field_width),
    };
    let style = panel.palette.overlay_field(field == panel.overlay.focus);
    text.paint(buffer, row, area, style);

    Some(CellPosition {
        column: area.origin + (caret - area.scroll),
        row,
    })
}

/// Paints the right-hand segments and returns the cells left for the field.
fn paint_right_side(
    buffer: &mut CellBuffer,
    row: usize,
    right: RightSide,
    panel: Panel<'_>,
) -> usize {
    let width = buffer.width();
    let field_start = LABEL_WIDTH;
    let budget = width.saturating_sub(field_start + MIN_FIELD_WIDTH + FIELD_GAP + RIGHT_MARGIN);

    let used = match right {
        RightSide::Counters => paint_counters(buffer, row, budget, panel),
        RightSide::Feedback => paint_feedback(buffer, row, budget, panel),
    };

    let reserved = if used == 0 {
        RIGHT_MARGIN
    } else {
        used + FIELD_GAP + RIGHT_MARGIN
    };
    width.saturating_sub(field_start + reserved)
}

/// Paints the toggles and the match count, and returns the cells they took.
fn paint_counters(buffer: &mut CellBuffer, row: usize, budget: usize, panel: Panel<'_>) -> usize {
    let (count, is_error) = count_segment(panel);
    let count_cells = display_width(&count);
    let width = buffer.width();

    // Widest group first, then the count alone, then the toggles alone. The
    // count outranks the toggles because it is the only segment that answers
    // the question the user is asking; the toggles merely say how it was asked.
    let (with_toggles, with_count, used) =
        if count_cells > 0 && budget >= TOGGLES_WIDTH + COUNT_GAP + count_cells {
            (true, true, TOGGLES_WIDTH + COUNT_GAP + count_cells)
        } else if count_cells > 0 && budget >= count_cells {
            (false, true, count_cells)
        } else if budget >= TOGGLES_WIDTH {
            (true, false, TOGGLES_WIDTH)
        } else {
            return 0;
        };

    if with_toggles {
        let start = width.saturating_sub(RIGHT_MARGIN + used);
        paint_toggles(buffer, row, start, &panel.overlay.options, panel.palette);
    }
    if with_count {
        let style = if is_error {
            panel.palette.overlay_error()
        } else {
            panel.palette.overlay()
        };
        let column = width.saturating_sub(RIGHT_MARGIN + count_cells);
        buffer.set_str(column, row, &count, style);
    }
    used
}

/// Paints the three option toggles from `column`.
fn paint_toggles(
    buffer: &mut CellBuffer,
    row: usize,
    column: usize,
    options: &SearchOptions,
    palette: &Palette,
) {
    for (index, (label, is_active)) in [
        ("Aa", options.case_sensitive),
        ("\\b", options.whole_word),
        (".*", options.regex),
    ]
    .into_iter()
    .enumerate()
    {
        let text = if is_active {
            format!("[{label}]")
        } else {
            format!(" {label} ")
        };
        buffer.set_str(
            column + index * (TOGGLE_WIDTH + 1),
            row,
            &text,
            palette.overlay_toggle(is_active),
        );
    }
}

/// Paints the feedback message, clipped to `budget`, and returns its cells.
fn paint_feedback(buffer: &mut CellBuffer, row: usize, budget: usize, panel: Panel<'_>) -> usize {
    let Some((message, is_error)) = feedback_segment(panel.overlay) else {
        return 0;
    };
    let used = display_width(&message).min(budget);
    if used == 0 {
        return 0;
    }
    let start = buffer.width().saturating_sub(RIGHT_MARGIN + used);
    let style = if is_error {
        panel.palette.overlay_error()
    } else {
        panel.palette.overlay()
    };
    let area = TextArea {
        origin: start,
        width: used,
        scroll: 0,
    };
    text::paint_text(buffer, row, 0, &message, style, area);
    used
}

/// The match-count segment, and whether it reports a failure.
///
/// The counts are the kernel's — [`Editor::search_match_count`] and
/// [`Editor::current_match_index`] — so the panel cannot disagree with what is
/// highlighted. An empty query says nothing at all rather than `0 of 0`: the
/// user has not asked a question yet, and answering one they did not ask reads
/// as a failure.
fn count_segment(panel: Panel<'_>) -> (String, bool) {
    if panel.overlay.query.is_empty() {
        return (String::new(), false);
    }
    if panel.overlay.has_error() {
        let text = if panel.overlay.options.regex {
            "Invalid regex"
        } else {
            "Invalid query"
        };
        return (text.to_owned(), true);
    }
    let total = panel.editor.search_match_count();
    if total == 0 {
        return ("No matches".to_owned(), false);
    }
    // The `None` arm is unreachable while the kernel selects a match whenever
    // it finds one, and is still answerable: the count is the fact, the
    // position within it is not.
    panel.editor.current_match_index().map_or_else(
        || (format!("{total} matches"), false),
        |index| (format!("{} of {total}", index + 1), false),
    )
}

/// The message row's text, and whether it reports a failure.
///
/// An unusable query outranks everything else. The count segment only has room
/// to say *that* the pattern is broken; this is where the engine says how, and
/// a stale `Replaced 4 matches` sitting in its place would leave the user with
/// no way to find out.
fn feedback_segment(overlay: &SearchOverlay) -> Option<(String, bool)> {
    if let Some(message) = overlay.error() {
        return Some((message.to_owned(), true));
    }
    Some(match overlay.feedback {
        Feedback::None => return None,
        Feedback::Replaced(1) => ("Replaced 1 match".to_owned(), false),
        Feedback::Replaced(count) => (format!("Replaced {count} matches"), false),
        Feedback::NoMatch => ("No match to replace".to_owned(), true),
        Feedback::ReadOnly => ("Document is read-only".to_owned(), true),
    })
}

/// The number of cells a string occupies.
fn display_width(text: &str) -> usize {
    LineLayout::new(text, 1).width()
}

/// Fills a run of a row with blanks in `style`.
fn blank(buffer: &mut CellBuffer, row: usize, columns: core::ops::Range<usize>, style: Style) {
    for column in columns {
        buffer.set_str(column, row, " ", style);
    }
}
