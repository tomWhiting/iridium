//! Drawing the rows while they are edited: the editor's lines with a gutter
//! saying which changed, and its carets, selections and search matches as
//! marks a face paints behind the text.

use iridium_editor::commands::builtin::{EXPLORER_EDIT_ASK_APPLY, EXPLORER_EDIT_LEAVE};
use iridium_editor::theme::Theme;
use iridium_editor::{Editor, Position};

use super::panel::FileExplorer;
use crate::line::LineBuilder;
use crate::outline::LineState;
use crate::row::{MarkKind, PanelBody, PanelCaret, PanelFit, PanelMark, PanelRow};

/// The columns before each line's text: a marker and a space.
pub(super) const GUTTER: usize = 2;

impl FileExplorer {
    /// The text being edited, with the caret in it.
    pub(super) fn edit_body(&mut self, theme: &Theme, fit: PanelFit) -> PanelBody {
        let total = self.row_count();
        let visible = total.clamp(1, fit.max_browse_rows);
        self.follow_selection(total, visible);
        let width = fit.content_columns;
        let text_width = width.saturating_sub(GUTTER).max(1);
        let hints = self.hints_text(&[
            (EXPLORER_EDIT_ASK_APPLY, "review"),
            (EXPLORER_EDIT_LEAVE, "browse"),
        ]);

        let Some(editing) = self.mode.editing_mut() else {
            return PanelBody {
                content_columns: width,
                rows: Vec::new(),
                caret: None,
            };
        };
        editing.set_page_rows(visible);

        let head = editing.editor.state().cursor.primary.head;
        if head.column < self.column_scroll {
            self.column_scroll = head.column;
        } else if head.column >= self.column_scroll + text_width {
            self.column_scroll = head.column + 1 - text_width;
        }
        let view = View {
            first_line: self.scroll,
            first_column: self.column_scroll,
            width,
        };

        let states = editing.listing.line_states();
        let removed = editing.listing.removed_count();
        let editor = &editing.editor;
        let document = &editor.state().document;

        let mut rows = Vec::with_capacity(visible + 1);
        rows.push(editing_label(theme, width, removed, &hints));
        for offset in 0..visible {
            let line = view.first_line + offset;
            let Some(text) = document.line(line) else {
                rows.push(PanelRow::new(Vec::new()));
                continue;
            };
            let state = states.get(line).copied().unwrap_or(LineState::Blank);
            let marks = marks_on(editor, line, text.chars().count(), view);
            rows.push(line_row(&text, state, view, theme).with_marks(marks));
        }

        PanelBody {
            content_columns: width,
            rows,
            caret: view.caret(head, visible),
        }
    }
}

/// Which slice of the text the screen shows.
#[derive(Debug, Clone, Copy)]
struct View {
    /// The text line on the first row under the label.
    first_line: usize,
    /// The text column at the left edge of the text area.
    first_column: usize,
    /// The characters a row holds, gutter included.
    width: usize,
}

impl View {
    /// Where text column `column` lands in a row, if it is on screen.
    fn column(self, column: usize) -> Option<usize> {
        let shown = GUTTER + column.checked_sub(self.first_column)?;
        (shown < self.width).then_some(shown)
    }

    /// The mark covering text columns `from..to`, clipped to the screen.
    fn mark(self, from: usize, to: usize, kind: MarkKind) -> Option<PanelMark> {
        let start = GUTTER + from.max(self.first_column).checked_sub(self.first_column)?;
        let end = (GUTTER + to.checked_sub(self.first_column)?).min(self.width);
        (start < end).then_some(PanelMark { start, end, kind })
    }

    /// The primary caret, against the composed rows (the label is row zero).
    fn caret(self, head: Position, visible: usize) -> Option<PanelCaret> {
        let row = head.line.checked_sub(self.first_line)?;
        if row >= visible {
            return None;
        }
        Some(PanelCaret {
            row: row + 1,
            column: self.column(head.column)?,
        })
    }
}

/// The label that stands where the query field does while browsing, with
/// how many entries no line holds — the deletions the text cannot show.
///
/// The keys are named from the live keymap, in the face's own modifier
/// spelling, so a rebound apply or leave is advertised as what it now is.
fn editing_label(theme: &Theme, width: usize, removed: usize, hints: &str) -> PanelRow {
    let mut line = LineBuilder::new(width);
    line.push("edit", theme.editor.change_modified);
    if removed > 0 {
        line.push("  ", theme.editor.line_number);
        line.push(&format!("{removed} to delete"), theme.editor.change_deleted);
    }
    line.push("    ", theme.editor.line_number);
    line.push(hints, theme.editor.line_number);
    PanelRow::new(line.finish())
}

/// One line of the text, after the gutter marker its state earns.
///
/// The marker is not decoration: colour alone fails anyone who cannot tell
/// the change colours apart.
fn line_row(text: &str, state: LineState, view: View, theme: &Theme) -> PanelRow {
    let colors = &theme.editor;
    let (marker, marker_color, text_color) = match state {
        LineState::Typed => ("+ ", colors.change_added, colors.change_added),
        LineState::Changed => ("~ ", colors.change_modified, colors.change_modified),
        LineState::Loaded | LineState::Blank => {
            let folder = text.ends_with(crate::outline::BRANCH_MARK);
            let color = if folder {
                colors.foreground
            } else {
                colors.line_number
            };
            ("  ", colors.line_number, color)
        },
    };
    let mut line = LineBuilder::new(view.width);
    line.push(marker, marker_color);
    // A tab is one column to the editor, so it is one cell here too.
    let shown: String = text
        .chars()
        .skip(view.first_column)
        .map(|character| if character == '\t' { ' ' } else { character })
        .collect();
    line.push(&shown, text_color);
    PanelRow::new(line.finish())
}

/// The marks on text line `line`, which is `length` characters long: search
/// matches, then selections, then the carets other than the primary one.
fn marks_on(editor: &Editor, line: usize, length: usize, view: View) -> Vec<PanelMark> {
    let state = editor.state();
    let mut marks = Vec::new();
    // A selected line break is drawn as one cell past the end, as editors do.
    let span_on = |start: Position, end: Position| -> Option<(usize, usize)> {
        if line < start.line || line > end.line {
            return None;
        }
        let from = if line == start.line { start.column } else { 0 };
        let to = if line == end.line {
            end.column
        } else {
            length + 1
        };
        (from < to).then_some((from, to))
    };

    if editor.is_searching() {
        let current = editor.current_match_index();
        for (index, range) in editor.search_state().all_matches().iter().enumerate() {
            if range.start.line > line {
                break;
            }
            if let Some((from, to)) = span_on(range.start, range.end) {
                let kind = if current == Some(index) {
                    MarkKind::CurrentMatch
                } else {
                    MarkKind::Match
                };
                marks.extend(view.mark(from, to, kind));
            }
        }
    }
    for selection in state.cursor.all_selections() {
        if !selection.is_collapsed()
            && let Some((from, to)) = span_on(selection.start(), selection.end())
        {
            marks.extend(view.mark(from, to, MarkKind::Selection));
        }
    }
    for selection in &state.cursor.secondary {
        if selection.head.line == line {
            let column = selection.head.column;
            marks.extend(view.mark(column, column + 1, MarkKind::Caret));
        }
    }
    marks
}
