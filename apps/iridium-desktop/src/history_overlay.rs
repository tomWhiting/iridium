//! The undo-tree panel: every branch of the history, reachable.
//!
//! This is the desktop face of the kernel's `history.togglePanel` host
//! command, and it keeps the terminal face's semantics exactly; the
//! terminal's test suite is the specification.
//!
//! # The tree is the kernel's and none of it is repeated here
//!
//! Everything shown comes from one call — [`Editor::history_snapshot`] — and
//! so does the panel's *shape*: the linearized rows and the selection
//! browsing them are [`tree_view`](iridium_editor::history::tree_view), the
//! same module the terminal face draws from, so the two panels cannot drift.
//! Jumping is [`Editor::jump_to_history_node`], the kernel's own multi-edge
//! replay. What is here is a key map and rows for [`crate::overlay`] to
//! paint.
//!
//! # Reading the panel
//!
//! The root — the state the file was opened in — is the top row; time runs
//! downward. A run with no forks stays flush left; each branch point indents
//! its children, so a straight history reads as a straight list and only
//! genuine forks grow sideways. The row the document currently sits on is
//! marked `*` and reads loudest; every other state is `o`. The **active
//! path** — the states plain undo and redo would travel through — is painted
//! bright; everything off it is dim. Ages are right-aligned.
//!
//! # Keys
//!
//! The panel is modal, like the palette: browsing history while stray keys
//! edited the document would grow the very tree being read.
//!
//! | Key | Action |
//! |---|---|
//! | `Up` / `Down` | move the selection, clamping at the ends |
//! | `PageUp` / `PageDown` | hop by one windowful |
//! | `Home` / `End` | the root / the newest row |
//! | `Enter` | jump the document to the selected state — the panel stays open |
//! | `Escape`, `Ctrl+Alt+H`, `⌘⌥H` | close |
//!
//! `Enter` deliberately keeps the panel open: hopping between two states and
//! watching the document change underneath is what a tree is *for*, and the
//! recomposed panel shows the `*` moving. `Escape` is how you put it away.

use iridium_editor::history::UndoNodeId;
use iridium_editor::history::tree_view::{TreeViewRow, TreeViewSelection, linearize};
use iridium_editor::theme::Theme;
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

use crate::line::LineBuilder;
use crate::overlay::{PANEL_MAX_VISIBLE_ROWS, PanelAnchor, PanelContent, PanelFit, PanelRow};

/// The characters one level of fork indentation moves a row.
const INDENT_CHARS: usize = 2;

/// The blank characters between a row's text and its age.
const AGE_GAP: usize = 2;

/// What became of a key handed to the panel.
///
/// There is no `Ignored`: the panel is modal, and a key it does not bind is
/// swallowed rather than falling through to the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryOutcome {
    /// The panel consumed the key and stays open.
    Handled,
    /// The panel was closed.
    Closed,
    /// The document should jump to this history node. The panel stays open,
    /// so the host recomposes it with the current marker moved.
    Jump(UndoNodeId),
}

/// The undo-tree panel.
///
/// The host owns one of these, toggles it when the kernel reports the
/// `history.togglePanel` host command, hands it every key while it is open,
/// and paints it over the composed frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryPanel {
    /// The kernel's selection model: which node is selected, and which slice
    /// of the rows the window shows.
    selection: TreeViewSelection,
}

impl HistoryPanel {
    /// A closed panel.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            selection: TreeViewSelection::with_window(PANEL_MAX_VISIBLE_ROWS),
        }
    }

    /// Resets the panel for opening: the selection follows the current node.
    pub const fn open(&mut self) {
        self.selection.reset();
    }

    /// Handles one key press. Every key is consumed; see the module docs.
    pub fn handle_key(&mut self, event: &KeyEvent, editor: &Editor) -> HistoryOutcome {
        let snapshot = editor.history_snapshot();
        let rows = linearize(&snapshot);
        let page = isize_of(self.selection.page());
        match (chord(event.modifiers), event.key) {
            (Chord::Plain, KeyCode::Escape)
            | (Chord::CtrlAlt | Chord::MetaAlt, KeyCode::Char('h' | 'H')) => HistoryOutcome::Closed,
            (Chord::Plain, KeyCode::Enter) => self
                .selection
                .selected_node(&rows)
                .map_or(HistoryOutcome::Handled, HistoryOutcome::Jump),
            (Chord::Plain, KeyCode::Up) => self.move_selection(&rows, -1),
            (Chord::Plain, KeyCode::Down) => self.move_selection(&rows, 1),
            (Chord::Plain, KeyCode::PageUp) => self.move_selection(&rows, -page),
            (Chord::Plain, KeyCode::PageDown) => self.move_selection(&rows, page),
            (Chord::Plain, KeyCode::Home) => self.move_selection(&rows, isize::MIN),
            (Chord::Plain, KeyCode::End) => self.move_selection(&rows, isize::MAX),
            // Modal: everything else is swallowed, not passed to the document.
            _ => HistoryOutcome::Handled,
        }
    }

    /// Composes the panel for painting.
    ///
    /// `&mut self` because composition is where the scroll window follows the
    /// selection and the page size is learned, exactly as in the palette.
    pub fn content(&mut self, editor: &Editor, theme: &Theme, fit: PanelFit) -> PanelContent {
        let snapshot = editor.history_snapshot();
        let tree = linearize(&snapshot);
        let visible = tree
            .len()
            .clamp(1, PANEL_MAX_VISIBLE_ROWS)
            .min(fit.max_interior_rows);
        self.selection.follow(&tree, visible.min(tree.len()));
        let selected_row = self.selection.selected_row(&tree);
        let scroll = self.selection.scroll();

        let mut rows = Vec::with_capacity(visible);
        for index in 0..visible {
            match tree.get(scroll + index) {
                Some(entry) => {
                    let selected = selected_row == Some(scroll + index);
                    rows.push(tree_row(entry, selected, theme, fit.content_columns));
                },
                None => rows.push(PanelRow::new(Vec::new())),
            }
        }
        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: fit.content_columns,
            rows,
            caret: None,
        }
    }

    /// Moves the selection by `delta` rows, clamping at both ends.
    fn move_selection(&mut self, rows: &[TreeViewRow<'_>], delta: isize) -> HistoryOutcome {
        self.selection.move_by(rows, delta);
        HistoryOutcome::Handled
    }
}

/// Composes one tree row: indent, marker, label, age.
fn tree_row(entry: &TreeViewRow<'_>, selected: bool, theme: &Theme, width: usize) -> PanelRow {
    // The current node reads loudest, the active path normally, parked
    // branches dimmed — so the eye finds "where am I" before anything else.
    let voice = if entry.node.is_current {
        theme.editor.line_number_active
    } else if entry.on_active_path {
        theme.editor.foreground
    } else {
        theme.editor.line_number
    };
    let quiet = theme.editor.line_number;

    let age = age_text(entry.node.elapsed_ms);
    let age_chars = age.chars().count();
    let text_width = width.saturating_sub(age_chars + AGE_GAP);

    // The indent is capped so a pathological fork ladder cannot push every
    // label off the panel's right edge.
    let indent = (entry.indent * INDENT_CHARS).min(width / 2);
    let marker = if entry.node.is_current { "* " } else { "o " };

    let mut line = LineBuilder::new(text_width);
    line.pad_to(indent, voice);
    line.push(marker, voice);
    line.push(&label_text(entry.node), voice);
    let used = line.used();
    let mut spans = line.finish();
    if width > age_chars {
        spans.push(crate::overlay::Span::new(
            " ".repeat(width - age_chars - used),
            quiet,
        ));
        spans.push(crate::overlay::Span::new(age, quiet));
    }
    if selected {
        PanelRow::selected(spans)
    } else {
        PanelRow::new(spans)
    }
}

/// The label of one node: what it is, and whether it forks.
fn label_text(node: &iridium_editor::history::UndoNodeInfo) -> String {
    let name = match node.description.as_deref() {
        Some(description) => description,
        None if node.parent_id.is_none() => "start",
        None => "edit",
    };
    if node.child_ids.len() > 1 {
        format!("{name} ({} branches)", node.child_ids.len())
    } else {
        name.to_owned()
    }
}

/// An age in the shortest honest unit.
fn age_text(elapsed_ms: u64) -> String {
    const SECOND: u64 = 1000;
    const MINUTE: u64 = 60 * SECOND;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    if elapsed_ms < SECOND {
        return "now".to_owned();
    }
    if elapsed_ms < MINUTE {
        return format!("{}s", elapsed_ms / SECOND);
    }
    if elapsed_ms < HOUR {
        return format!("{}m", elapsed_ms / MINUTE);
    }
    if elapsed_ms < DAY {
        return format!("{}h", elapsed_ms / HOUR);
    }
    format!("{}d", elapsed_ms / DAY)
}

/// `value` as an `isize`, saturating on a page size no window can reach.
fn isize_of(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}

/// The modifier combinations the panel distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chord {
    /// No modifier that changes the meaning of the key.
    Plain,
    /// Control and Alt together — the toggle chord that closes the panel.
    CtrlAlt,
    /// Meta and Alt together — the mac spelling of the same toggle.
    MetaAlt,
    /// Anything else, which the modal panel swallows.
    Other,
}

/// The chord one modifier set names.
const fn chord(modifiers: Modifiers) -> Chord {
    match (modifiers.ctrl, modifiers.alt, modifiers.meta) {
        (false, false, false) => Chord::Plain,
        (true, true, false) => Chord::CtrlAlt,
        (false, true, true) => Chord::MetaAlt,
        _ => Chord::Other,
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::theme::Theme;
    use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

    use super::{HistoryOutcome, HistoryPanel, age_text};
    use crate::overlay::PanelFit;

    /// A key press with no modifiers.
    fn press(key: KeyCode) -> KeyEvent {
        KeyEvent {
            key,
            modifiers: Modifiers::none(),
            is_repeat: false,
        }
    }

    /// A chord under the given modifiers.
    fn chord(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
        KeyEvent {
            key,
            modifiers,
            is_repeat: false,
        }
    }

    /// Undoes one step through the kernel's own binding.
    fn undo(editor: &mut Editor) {
        let _ = editor.handle_key(&chord(KeyCode::Char('z'), Modifiers::ctrl()));
    }

    /// A kernel whose edits never coalesce, so each paste is one history node.
    fn ungrouped_editor() -> Editor {
        let mut editor = Editor::with_defaults();
        editor.state_mut().history.set_group_timeout_ms(0);
        editor
    }

    /// A kernel whose history is a straight line: root → "a" → "ab".
    fn linear_history() -> Editor {
        let mut editor = ungrouped_editor();
        editor.paste("a");
        editor.paste("b");
        editor
    }

    /// A kernel whose history forks at the root: root → "a" abandoned,
    /// root → "c" current.
    fn forked_history() -> Editor {
        let mut editor = ungrouped_editor();
        editor.paste("a");
        undo(&mut editor);
        editor.paste("c");
        editor
    }

    /// An open panel.
    fn open_panel() -> HistoryPanel {
        let mut panel = HistoryPanel::new();
        panel.open();
        panel
    }

    /// The composed rows as plain text at a generous size.
    fn rows_text(panel: &mut HistoryPanel, editor: &Editor) -> Vec<String> {
        let content = panel.content(
            editor,
            &Theme::dark(),
            PanelFit {
                content_columns: 60,
                max_interior_rows: 12,
            },
        );
        content
            .rows
            .iter()
            .map(crate::overlay::PanelRow::text)
            .collect()
    }

    #[test]
    fn escape_closes_and_both_toggle_spellings_close() {
        let editor = linear_history();
        let mut panel = open_panel();
        assert_eq!(
            panel.handle_key(&press(KeyCode::Escape), &editor),
            HistoryOutcome::Closed
        );
        let ctrl_alt = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        };
        assert_eq!(
            panel.handle_key(&chord(KeyCode::Char('h'), ctrl_alt), &editor),
            HistoryOutcome::Closed,
            "the chord that opened the panel closes it"
        );
        let meta_alt = Modifiers {
            meta: true,
            alt: true,
            ..Modifiers::none()
        };
        assert_eq!(
            panel.handle_key(&chord(KeyCode::Char('h'), meta_alt), &editor),
            HistoryOutcome::Closed,
            "the mac spelling closes it too"
        );
    }

    #[test]
    fn the_panel_is_modal_and_swallows_what_it_does_not_bind() {
        let editor = linear_history();
        let mut panel = open_panel();
        assert_eq!(
            panel.handle_key(&chord(KeyCode::Char('s'), Modifiers::ctrl()), &editor),
            HistoryOutcome::Handled
        );
        assert_eq!(
            panel.handle_key(&press(KeyCode::Char('x')), &editor),
            HistoryOutcome::Handled,
            "typing must not reach the document while history is open"
        );
    }

    #[test]
    fn moving_up_and_jumping_reaches_the_previous_state() {
        let mut editor = linear_history();
        assert_eq!(editor.content(), "ab");
        let mut panel = open_panel();

        // The selection starts on the current node — the newest row. One step
        // up is the state before the last edit.
        assert_eq!(
            panel.handle_key(&press(KeyCode::Up), &editor),
            HistoryOutcome::Handled
        );
        let HistoryOutcome::Jump(id) = panel.handle_key(&press(KeyCode::Enter), &editor) else {
            panic!("a selected row must resolve to a jump");
        };
        assert!(editor.jump_to_history_node(id), "the kernel accepts the id");
        assert_eq!(editor.content(), "a", "one row up is one edit back");
    }

    #[test]
    fn a_parked_branch_is_reachable_by_rows_alone() {
        let mut editor = forked_history();
        assert_eq!(editor.content(), "c");
        let mut panel = open_panel();

        // Root at the top, then the abandoned "a" branch, then the current
        // "c": one step up from the current row is the parked branch.
        panel.handle_key(&press(KeyCode::Up), &editor);
        let HistoryOutcome::Jump(id) = panel.handle_key(&press(KeyCode::Enter), &editor) else {
            panic!("the parked branch resolves to a jump");
        };
        assert!(editor.jump_to_history_node(id));
        assert_eq!(
            editor.content(),
            "a",
            "the branch abandoned by undo-then-edit is reachable"
        );
    }

    #[test]
    fn the_selection_clamps_at_both_ends() {
        let editor = linear_history();
        let mut panel = open_panel();
        // The selection starts at the newest row; `Down` must stay there.
        let at_end = panel.handle_key(&press(KeyCode::Enter), &editor);
        panel.handle_key(&press(KeyCode::Down), &editor);
        assert_eq!(panel.handle_key(&press(KeyCode::Enter), &editor), at_end);

        // `Home` is the root; `Up` from it must stay there.
        panel.handle_key(&press(KeyCode::Home), &editor);
        let at_root = panel.handle_key(&press(KeyCode::Enter), &editor);
        panel.handle_key(&press(KeyCode::Up), &editor);
        assert_eq!(panel.handle_key(&press(KeyCode::Enter), &editor), at_root);
        assert_ne!(at_root, at_end);
    }

    #[test]
    fn the_rows_mark_the_current_state_the_root_and_the_ages() {
        let editor = linear_history();
        let mut panel = open_panel();
        let rows = rows_text(&mut panel, &editor);
        assert!(
            rows.iter().any(|row| row.contains("* edit")),
            "the current state is marked with an asterisk: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("o start")),
            "the root row is labelled start: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("now")),
            "fresh edits show their age: {rows:?}"
        );
    }

    #[test]
    fn a_fork_names_its_branch_count() {
        let editor = forked_history();
        let mut panel = open_panel();
        let rows = rows_text(&mut panel, &editor);
        assert!(
            rows.iter().any(|row| row.contains("(2 branches)")),
            "a fork must say it forks: {rows:?}"
        );
    }

    #[test]
    fn the_selected_row_is_flagged_for_the_painter() {
        let editor = linear_history();
        let mut panel = open_panel();
        let content = panel.content(
            &editor,
            &Theme::dark(),
            PanelFit {
                content_columns: 60,
                max_interior_rows: 12,
            },
        );
        let selected: Vec<usize> = content
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.selected.then_some(index))
            .collect();
        assert_eq!(
            selected,
            vec![2],
            "the selection starts on the current, newest row"
        );
    }

    #[test]
    fn ages_use_the_shortest_honest_unit() {
        assert_eq!(age_text(0), "now");
        assert_eq!(age_text(999), "now");
        assert_eq!(age_text(1_000), "1s");
        assert_eq!(age_text(59_999), "59s");
        assert_eq!(age_text(60_000), "1m");
        assert_eq!(age_text(3_599_999), "59m");
        assert_eq!(age_text(3_600_000), "1h");
        assert_eq!(age_text(86_400_000), "1d");
    }
}
