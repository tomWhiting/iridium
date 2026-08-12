//! Composing one explorer row: indent, disclosure, name, and what it has to
//! say for itself.
//!
//! Two builders, because the two modes read their names from different places.
//! [`entry_row`] takes a node and asks the tree what it is called;
//! [`edited_row`] takes a row of the buffer and draws what it now *says*,
//! which is the only thing that could be right while somebody is typing into
//! it. A row the user typed has no node at all, so the first builder could not
//! draw it under any argument.

use iridium_editor::theme::{Color, Theme};
use iridium_explorer::{EntryKind, FileTree, NodeId};

use super::plan::EditedRow;
use crate::line::{highlighted_spans, match_color};
use crate::row::{PanelRow, Span};

/// One indent level, in characters.
const INDENT: usize = 2;

/// The column an edited name starts at, which is where its caret is measured
/// from.
///
/// The indent plus the two-character marker every row pays for whether or not
/// it carries one — the same rule [`entry_row`] follows with the disclosure,
/// and for the same reason: names that stepped in and out as rows were struck
/// through would be names nobody could read down the panel.
pub const fn edited_name_column(depth: usize) -> usize {
    depth.saturating_mul(INDENT).saturating_add(2)
}

/// How deep each buffer row is drawn, from the parent chain it carries.
///
/// The buffer stores a parent index rather than a depth, because the parent is
/// what every path is derived from and a depth beside it would be a second
/// answer that could disagree. Drawing needs the other one, so it is computed
/// here — in one forward pass, which is enough because a parent always comes
/// before its children and the buffer maintains that on every edit.
///
/// A parent index that somehow pointed forwards would read a depth that has
/// not been written yet; the `get` answers `0` for it rather than panicking,
/// and [`super::plan`] refuses such a row before anything can be done to it.
#[must_use]
pub fn depths(rows: &[EditedRow]) -> Vec<usize> {
    let mut depths = Vec::with_capacity(rows.len());
    for row in rows {
        let depth = row.parent.map_or(0, |parent| {
            depths.get(parent).map_or(0, |above: &usize| above + 1)
        });
        depths.push(depth);
    }
    depths
}

/// One row of the edited buffer, at `width` characters.
///
/// # There is no strike-through, and this is what stands in for one
///
/// ⚠️ The overlay's [`Span`] carries text and a colour and nothing else, and
/// the painter lays glyphs on a fixed `char × char_width` grid — so combining
/// marks would not advance-match it and there is no line-through attribute to
/// set. A row marked for deletion is therefore drawn in
/// `change_deleted` with a `✗` in the marker column, and a row the user typed
/// in `change_added` with a `+`. Those are the same two colours
/// [`super::confirm`] spends on a delete and a create, so the buffer and the
/// confirmation cannot disagree about which rows are which.
///
/// The marker is not decoration: colour alone fails anyone who cannot tell
/// these two apart, and a row that will destroy a directory tree is the last
/// place to rely on it.
pub fn edited_row(
    row: &EditedRow,
    depth: usize,
    selected: bool,
    theme: &Theme,
    width: usize,
) -> PanelRow {
    let mut prefix = " ".repeat(depth.saturating_mul(INDENT));
    let (marker, marker_color) = marker(row, theme);
    prefix.push_str(marker);

    let mut name = row.name.clone();
    if row.directory && !name.is_empty() {
        name.push('/');
    }

    let prefix_width = prefix.chars().count();
    let mut spans = vec![Span::new(truncate(&prefix, width), marker_color)];
    let shown = truncate(&name, width.saturating_sub(prefix_width));
    if !shown.is_empty() {
        spans.push(Span::new(shown, name_color(row, theme)));
    }

    if selected {
        PanelRow::selected(spans)
    } else {
        PanelRow::new(spans)
    }
}

/// The two characters before an edited name, and their colour.
const fn marker(row: &EditedRow, theme: &Theme) -> (&'static str, Color) {
    if row.deleted {
        return ("✗ ", theme.editor.change_deleted);
    }
    if row.origin.is_none() {
        return ("+ ", theme.editor.change_added);
    }
    ("  ", theme.editor.line_number)
}

/// The colour an edited name is drawn in.
///
/// Struck first, because what is about to happen to a row matters more than
/// what kind of thing it is — and a deleted directory that read like every
/// other directory would be the one row in this panel worth telling apart.
const fn name_color(row: &EditedRow, theme: &Theme) -> Color {
    if row.deleted {
        theme.editor.change_deleted
    } else if row.origin.is_none() {
        theme.editor.change_added
    } else if row.directory {
        theme.editor.foreground
    } else {
        theme.editor.line_number
    }
}

/// Everything about a row that is not read out of the tree.
///
/// A struct rather than six positional arguments: they are all small, all the
/// same shape, and swapping two `bool`s at a call site is the kind of mistake
/// nothing would catch.
#[derive(Debug, Clone, Default)]
pub struct RowShape {
    /// How far the row is indented, counted from the root at zero.
    pub depth: usize,
    /// Whether the node claims children, and therefore gets a disclosure.
    pub has_children: bool,
    /// Whether the disclosure points down.
    pub expanded: bool,
    /// Whether the selection is on this row.
    pub selected: bool,
    /// Character positions **into the name** to underline, from a filter.
    pub positions: Vec<u32>,
}

/// One row of the tree, at `width` characters.
///
/// Returns a row saying so when `id` is not a node this tree issued — a
/// selection can outlive a refresh, and a panel that panicked on a stale id
/// would take the window with it.
pub fn entry_row(
    files: &FileTree,
    id: NodeId,
    shape: &RowShape,
    theme: &Theme,
    width: usize,
) -> PanelRow {
    let Some(info) = files.info(id) else {
        return PanelRow::new(vec![Span::new("?", theme.editor.line_number)]);
    };

    // Two characters per level, and a disclosure column every row pays for
    // whether or not it has an arrow — so names line up down the panel instead
    // of stepping in and out with the arrows.
    let mut prefix = " ".repeat(shape.depth.saturating_mul(INDENT));
    prefix.push_str(match (shape.has_children, shape.expanded) {
        (true, true) => "▾ ",
        (true, false) => "▸ ",
        (false, _) => "  ",
    });

    let mut name = info.name.to_owned();
    if info.kind == EntryKind::Directory {
        name.push('/');
    }
    if info.is_loading {
        name.push('…');
    }

    let prefix_width = prefix.chars().count();
    // Files sit a shade back from directories, so the shape of the tree reads
    // before its contents do.
    let base = if info.kind.is_expandable() {
        theme.editor.foreground
    } else {
        theme.editor.line_number
    };

    let mut spans = vec![Span::new(
        truncate(&prefix, width),
        theme.editor.line_number,
    )];
    let room = width.saturating_sub(prefix_width);
    let shown = truncate(&name, room);
    if !shown.is_empty() {
        // The positions index the untruncated name, and `highlighted_spans`
        // drops the ones that fall past the end — which is exactly what a cut
        // name needs, since the characters they underlined are gone.
        spans.extend(highlighted_spans(
            &shown,
            &shape.positions,
            base,
            match_color(theme),
        ));
    }

    if let Some(error) = info.error {
        let used = prefix_width + shown.chars().count();
        let room = width.saturating_sub(used + 2);
        if room > 0 {
            spans.push(Span::new(
                format!("  {}", truncate(error, room)),
                theme.editor.line_number,
            ));
        }
    }

    if shape.selected {
        PanelRow::selected(spans)
    } else {
        PanelRow::new(spans)
    }
}

/// Cuts `text` to `width` characters, by characters and not by bytes.
///
/// An ellipsis rather than a hard cut, so a truncated name is visibly
/// truncated — a path silently missing its last three characters is how
/// someone opens the wrong file.
pub fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}
