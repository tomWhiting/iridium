//! The statusline: what document this is, what mode the keymap is in, and
//! where the cursor is.
//!
//! The kernel has no notion of a file, so the document's name and whether it
//! has unsaved changes are the host's to supply — opening and saving belong to
//! `apps/iridium`. Everything else on the line is read from the kernel: the
//! active mode comes from the keymap resolver, and the position from the
//! primary cursor.
//!
//! The mode is shown whenever the resolver reports one. A modal keymap that
//! does not say which mode it is in is unusable, and "no mode" is itself
//! meaningful — a non-modal keymap simply has none, and then the segment is
//! absent rather than blank.

use iridium_editor::Editor;
use iridium_editor::commands::structural_keymap::{STRUCTURAL, STRUCTURAL_CHANGE, STRUCTURAL_VERB};

use super::line::LineLayout;
use super::palette::Palette;
use crate::cell::{CellBuffer, Style};

/// What the host knows about the document that the kernel does not.
#[derive(Debug, Clone, Copy, Default)]
pub struct Status<'a> {
    /// The document's display name, if it has one.
    ///
    /// `None` renders as `[No Name]`, matching what an unsaved buffer is.
    pub name: Option<&'a str>,
    /// Whether the document has changes that have not been written out.
    ///
    /// The kernel cannot answer this: it tracks revisions, not files, and a
    /// document undone back to its saved state is a question only the host's
    /// save bookkeeping can settle.
    pub dirty: bool,
}

/// Paints the statusline across `row`.
pub fn paint(
    buffer: &mut CellBuffer,
    row: usize,
    editor: &Editor,
    status: Status<'_>,
    palette: &Palette,
) {
    let width = buffer.width();
    if width == 0 || row >= buffer.height() {
        return;
    }
    let style = palette.status();
    blank(buffer, row, width, style);

    let left = left_segment(editor, status);
    let right = right_segment(editor);

    buffer.set_str(0, row, &left, style);

    let right_width = display_width(&right);
    if right_width < width {
        let start = width - right_width;
        // Only when the two would not collide: a narrow terminal keeps the
        // document's identity and drops the position, because a name half
        // overwritten by a column number tells the reader neither.
        if start > display_width(&left) {
            buffer.set_str(start, row, &right, style);
        }
    }
}

/// The document's identity and the active mode.
fn left_segment(editor: &Editor, status: Status<'_>) -> String {
    let mut segment = String::new();
    segment.push(' ');
    segment.push_str(status.name.unwrap_or("[No Name]"));
    if status.dirty {
        segment.push_str(" [+]");
    }
    if let Some(count) = editor.pending_key_count() {
        segment.push_str("  count:");
        segment.push_str(&count.to_string());
    }
    for key in editor.pending_key_sequence() {
        segment.push(' ');
        segment.push_str(
            &iridium_editor::commands::StrokePattern::new(
                key.key,
                iridium_editor::commands::ModifierPattern::exact(key.modifiers),
            )
            .to_string(),
        );
        segment.push('…');
    }
    if let Some(mode) = editor.mode() {
        segment.push_str("  ");
        segment.push_str(mode.as_str());
        if mode == &STRUCTURAL_CHANGE {
            segment.push_str("  Esc: finish change");
        }
        if mode == &STRUCTURAL || mode == &STRUCTURAL_VERB {
            if editor.state().syntax.tree().is_none() {
                segment.push_str(" [text] i: type");
            } else {
                if let Some(kind) = editor
                    .state()
                    .syntax
                    .navigation_label(&editor.state().document, &editor.state().cursor)
                    .or_else(|| editor.selected_syntax_kind())
                {
                    segment.push_str(" [");
                    segment.push_str(kind);
                    segment.push(']');
                }
                if !editor.pending_key_sequence().is_empty() || editor.pending_key_count().is_some()
                {
                    segment.push_str("  Esc: cancel");
                } else if editor
                    .keymap()
                    .layers()
                    .iter()
                    .any(|layer| layer.name() == "terminal-modal")
                {
                    segment.push_str("  arrows: move , smaller . larger i: type");
                } else {
                    segment.push_str("  ↑↓ move ← coarser → finer i: type");
                }
            }
        }
    }
    if editor.mode() != Some(&STRUCTURAL)
        && editor.mode() != Some(&STRUCTURAL_VERB)
        && editor.mode() != Some(&STRUCTURAL_CHANGE)
        && editor
            .keymap()
            .layers()
            .iter()
            .any(|layer| layer.name() == "structural-navigation")
    {
        segment.push_str("  Ctrl+K: structural mode");
    }
    segment
}

/// The cursor position, and how many cursors there are when there is more than
/// one.
fn right_segment(editor: &Editor) -> String {
    let cursor = editor.cursor();
    let count = editor.state().cursor.cursor_count();
    let scope = editor
        .selected_syntax_kind()
        .map_or_else(String::new, |kind| format!("{}  ", kind.replace('_', " ")));
    let position = format!("{scope}Ln {}, Col {} ", cursor.line + 1, cursor.column + 1);
    if count > 1 {
        return format!("{count} cursors  {position}");
    }
    position
}

/// The number of cells a string occupies.
fn display_width(text: &str) -> usize {
    LineLayout::new(text, 1).width()
}

/// Fills a row with blanks in `style`.
fn blank(buffer: &mut CellBuffer, row: usize, width: usize, style: Style) {
    for column in 0..width {
        buffer.set_str(column, row, " ", style);
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::commands::ModeName;

    use super::*;
    use crate::cell::CellContent;

    /// The text of a row.
    fn row_text(buffer: &CellBuffer, row: usize) -> String {
        let mut out = String::new();
        let Some(cells) = buffer.row(row) else {
            return out;
        };
        for cell in cells {
            match cell.content() {
                CellContent::Grapheme(grapheme) => grapheme.push_to(&mut out),
                CellContent::Continuation => {},
            }
        }
        out
    }

    /// A statusline painted into a one-row buffer of `width` cells.
    fn painted(editor: &Editor, status: Status<'_>, width: usize) -> String {
        let mut buffer = CellBuffer::new(width, 1);
        let palette = Palette::from_theme(editor.get_theme());
        paint(&mut buffer, 0, editor, status, &palette);
        row_text(&buffer, 0)
    }

    #[test]
    fn an_unnamed_document_says_so() {
        let editor = Editor::with_defaults();
        let line = painted(&editor, Status::default(), 40);
        assert!(line.contains("[No Name]"), "{line:?}");
    }

    #[test]
    fn a_dirty_document_is_marked() {
        let editor = Editor::with_defaults();
        let clean = painted(
            &editor,
            Status {
                name: Some("main.rs"),
                dirty: false,
            },
            40,
        );
        let dirty = painted(
            &editor,
            Status {
                name: Some("main.rs"),
                dirty: true,
            },
            40,
        );
        assert!(clean.contains("main.rs"), "{clean:?}");
        assert!(!clean.contains("[+]"), "{clean:?}");
        assert!(dirty.contains("[+]"), "{dirty:?}");
    }

    #[test]
    fn the_cursor_position_is_one_based() {
        let mut editor = Editor::with_defaults();
        editor.set_content("hello\nworld");
        editor.set_cursor(iridium_editor::Position::new(1, 3));
        let line = painted(&editor, Status::default(), 40);
        assert!(line.contains("Ln 2, Col 4"), "{line:?}");
    }

    #[test]
    fn the_active_mode_is_shown_when_there_is_one() {
        let mut editor = Editor::with_defaults();
        let without = painted(&editor, Status::default(), 40);
        assert!(!without.contains("normal"), "{without:?}");

        editor.set_mode(Some(ModeName::from_static("normal")));
        let with = painted(&editor, Status::default(), 40);
        assert!(with.contains("normal"), "{with:?}");
    }

    #[test]
    fn extra_cursors_are_counted() {
        let mut editor = Editor::with_defaults();
        editor.set_content("hello\nworld");
        let one = painted(&editor, Status::default(), 40);
        assert!(!one.contains("cursors"), "{one:?}");

        editor
            .state_mut()
            .cursor
            .add_cursor(iridium_editor::Selection::collapsed(
                iridium_editor::Position::new(1, 0),
            ));
        let two = painted(&editor, Status::default(), 40);
        assert!(two.contains("2 cursors"), "{two:?}");
    }

    #[test]
    fn the_line_fills_the_whole_width() {
        let editor = Editor::with_defaults();
        assert_eq!(painted(&editor, Status::default(), 24).chars().count(), 24);
    }

    #[test]
    fn a_narrow_line_keeps_the_name_and_drops_the_position() {
        let editor = Editor::with_defaults();
        let line = painted(
            &editor,
            Status {
                name: Some("a-rather-long-document-name.rs"),
                dirty: true,
            },
            20,
        );
        assert!(line.starts_with(" a-rather-long-docu"), "{line:?}");
        assert!(!line.contains("Ln"), "{line:?}");
    }

    #[test]
    fn a_zero_width_statusline_is_a_no_op() {
        let editor = Editor::with_defaults();
        let mut buffer = CellBuffer::new(0, 0);
        let palette = Palette::from_theme(editor.get_theme());
        paint(&mut buffer, 0, &editor, Status::default(), &palette);
        assert_eq!(buffer.width(), 0);
    }
    #[test]
    fn status_names_the_exact_primary_syntax_selection() {
        let mut editor = Editor::with_defaults();
        editor.set_content("[11,22]");
        editor.set_language(iridium_editor::Language::Json);
        editor.set_selection(
            iridium_editor::Position::new(0, 1),
            iridium_editor::Position::new(0, 3),
        );
        let line = painted(&editor, Status::default(), 80);
        assert!(line.contains("number  Ln 1, Col 4"), "{line}");
        editor.set_cursor(iridium_editor::Position::new(0, 1));
        assert!(!painted(&editor, Status::default(), 80).contains("number"));
    }
}
