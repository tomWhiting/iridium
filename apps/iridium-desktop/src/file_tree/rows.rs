//! Composing one explorer row: indent, disclosure, name, and what it has to
//! say for itself.

use iridium_editor::theme::Theme;
use iridium_explorer::{EntryKind, FileTree, NodeId};

use crate::line::{highlighted_spans, match_color};
use crate::overlay::{PanelRow, Span};

/// One indent level, in characters.
const INDENT: usize = 2;

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
