//! The command palette: a floating panel that runs any command by name.
//!
//! This is the desktop face of the kernel's `palette.open` host command — the
//! UI the kernel names but cannot draw — and it keeps the terminal face's
//! semantics exactly; the terminal's test suite is the specification.
//!
//! # The engine is the kernel's and none of it is repeated here
//!
//! Nothing in this module matches, scores, ranks or remembers. Every
//! keystroke re-runs
//! [`palette::search_text`]
//! against the kernel's own registry — the same matcher, the same recency
//! bonus, the same total order every face uses, so the same query can never
//! put a different command first here than in the terminal. What is here is a
//! text field, a selection, a scroll window and rows for [`crate::overlay`]
//! to paint.
//!
//! Recency lives in a [`CommandMru`] the *host* owns and records into after a
//! command actually runs; the panel only reads it. A panel that recorded on
//! `Enter` would remember commands whose execution then failed.
//!
//! # The panel is modal
//!
//! Every key is consumed while it is open, exactly like a prompt: the palette
//! exists to run any command by name, and a chord falling through to the
//! document while the user is aiming at a command list would edit text they
//! are not looking at. `Escape` closes it; so do the `Ctrl+K` and `⌘K` that
//! open it, because a toggle is what the finger expects.
//!
//! | Key | Action |
//! |---|---|
//! | any printable | insert into the query |
//! | `Left` `Right` `Home` `End` `Backspace` `Delete` | edit the query |
//! | `Down`, `Ctrl+N` / `Up`, `Ctrl+P` | move the selection, clamping |
//! | `PageDown` / `PageUp` | hop by one windowful |
//! | `Enter` | run the selected command |
//! | `Escape`, `Ctrl+K`, `⌘K` | close |
//!
//! The selection **clamps** at both ends rather than wrapping: wrapping
//! overshoots on key repeat, and a list whose ends are walls can be leaned
//! on.
//!
//! # What a row shows
//!
//! The command's title; the key that runs it, right-aligned in the mac glyph
//! spelling ([`KeyLabelStyle::MacGlyphsCommandAsMeta`] — this face's chords really are ⌘
//! chords) and dropped before it would collide with the title; and, when the
//! query matched something other than the title, that matched text as a quiet
//! annotation after it — highlighting match positions inside a string that is
//! not shown would underline nothing. Match positions are recoloured on
//! exactly the characters the kernel reported, mapped through the text's own
//! grapheme clusters so a joined emoji recolours whole, never mid-cluster.

use iridium_editor::commands::palette::{self, CommandMru, MatchField, PaletteEntry};
use iridium_editor::theme::Theme;
use iridium_editor::{CommandId, Editor, KeyCode, KeyEvent, KeyLabelStyle, Modifiers};

use crate::line::{LineBuilder, highlighted_spans, match_color, skip_chars};
use crate::overlay::{
    PANEL_MAX_VISIBLE_ROWS, PanelAnchor, PanelCaret, PanelContent, PanelFit, PanelRow, Span,
    scroll_for,
};
use crate::prompt::Entry;

/// The query prompt, drawn before the field.
const PROMPT: &str = "> ";

/// The fewest characters the title keeps before the key hint is dropped.
const TITLE_FLOOR: usize = 8;

/// The blank characters between the title (or annotation) and the key hint.
const HINT_GAP: usize = 2;

/// The blank characters between the title and a non-title match annotation.
const ANNOTATION_GAP: usize = 2;

/// What became of a key handed to the palette.
///
/// There is no `Ignored`: the palette is modal, and a key it does not bind is
/// swallowed rather than falling through to the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteOutcome {
    /// The palette consumed the key and stays open.
    Handled,
    /// The palette was closed without running anything.
    Closed,
    /// The palette was closed and this command should now run.
    ///
    /// Resolution happened against the same ranked search the panel painted —
    /// the kernel's order is total, so what was highlighted is what runs.
    Run(CommandId),
}

/// The command palette panel.
///
/// The host owns one of these, opens it when the kernel reports the
/// `palette.open` host command, hands it every key while it is open, and
/// paints it over the composed frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandPalette {
    /// The query field.
    query: Entry,
    /// The selected result, as an index into the full ranked result list.
    ///
    /// Clamped against the result count wherever it is read, because the
    /// count changes under it with every edit to the query.
    selected: usize,
    /// The first visible result: the scroll position of the list window.
    scroll: usize,
    /// How many result rows the last composition showed — the page the page
    /// keys hop.
    ///
    /// Before the first composition it is a reasonable page rather than zero,
    /// so `PageDown` on a panel that has not reached the screen yet still
    /// moves.
    window: usize,
    /// The selection the window last followed.
    ///
    /// ⭐ **What separates "the selection moved" from "a frame happened".**
    /// The window has to chase the selection when a key moves it and stay put
    /// when the wheel moves the window instead, and those two are the same
    /// composition from the inside — the only difference is whether the
    /// selection is where it was last time this ran.
    followed: Option<usize>,
}

impl CommandPalette {
    /// A closed palette with an empty query.
    #[must_use]
    pub fn new() -> Self {
        Self {
            window: PANEL_MAX_VISIBLE_ROWS,
            ..Self::default()
        }
    }

    /// Resets the panel for opening: empty query, selection at the top.
    ///
    /// The query is cleared rather than kept — the opposite of the search
    /// panel's choice, deliberately. A search query is a place in the
    /// document the user may want back; a palette query was an *aim*, already
    /// hit or abandoned, and an empty query shows every command ordered by
    /// recency, which is the most useful first screen a palette has.
    pub fn open(&mut self) {
        self.query = Entry::new();
        self.selected = 0;
        self.scroll = 0;
        self.followed = None;
    }

    /// The query field's text.
    #[must_use]
    pub fn query(&self) -> &str {
        self.query.text()
    }

    /// Inserts pasted text into the query, one character at a time.
    ///
    /// The field refuses control characters, so a multi-line paste
    /// contributes its printable characters only — the newlines that would
    /// silently change what the query matched never enter it.
    pub fn paste(&mut self, text: &str) {
        let mut changed = false;
        for character in text.chars() {
            changed |= self.query.insert(character);
        }
        if changed {
            self.selected = 0;
            self.scroll = 0;
            self.followed = None;
        }
    }

    /// Handles one key press. Every key is consumed; see the module docs.
    pub fn handle_key(
        &mut self,
        event: &KeyEvent,
        editor: &Editor,
        mru: &CommandMru,
    ) -> PaletteOutcome {
        let page = isize_of(self.window.max(1));
        match (chord(event.modifiers), event.key) {
            (Chord::Plain, KeyCode::Escape)
            | (Chord::Ctrl | Chord::Meta, KeyCode::Char('k' | 'K')) => PaletteOutcome::Closed,
            (Chord::Plain, KeyCode::Enter) => self.accept(editor, mru),
            (Chord::Plain, KeyCode::Up) | (Chord::Ctrl, KeyCode::Char('p' | 'P')) => {
                self.move_selection(editor, mru, -1)
            },
            (Chord::Plain, KeyCode::Down) | (Chord::Ctrl, KeyCode::Char('n' | 'N')) => {
                self.move_selection(editor, mru, 1)
            },
            (Chord::Plain, KeyCode::PageUp) => self.move_selection(editor, mru, -page),
            (Chord::Plain, KeyCode::PageDown) => self.move_selection(editor, mru, page),
            (Chord::Plain, KeyCode::Left) => self.edit(Entry::move_left, false),
            (Chord::Plain, KeyCode::Right) => self.edit(Entry::move_right, false),
            (Chord::Plain, KeyCode::Home) => self.edit(Entry::move_home, false),
            (Chord::Plain, KeyCode::End) => self.edit(Entry::move_end, false),
            (Chord::Plain, KeyCode::Backspace) => self.edit(Entry::backspace, true),
            (Chord::Plain, KeyCode::Delete) => self.edit(Entry::delete, true),
            (Chord::Plain, KeyCode::Char(character)) => {
                self.edit(|field| field.insert(character), true)
            },
            // Modal: everything else is swallowed, not passed to the document.
            _ => PaletteOutcome::Handled,
        }
    }

    /// Composes the panel for painting: the query row, then the visible slice
    /// of the ranked results.
    ///
    /// `&mut self` because composition is where the panel learns its
    /// geometry: the scroll window follows the selection here, and the page
    /// size the page keys hop by is whatever this composition actually
    /// showed.
    pub fn content(
        &mut self,
        editor: &Editor,
        mru: &CommandMru,
        theme: &Theme,
        fit: PanelFit,
    ) -> PanelContent {
        let results = palette::search_text(editor.commands(), mru, self.query.text(), None);
        // An empty list still gets one row, to say so.
        let list_rows = results.len().max(1);
        let visible = list_rows
            .min(PANEL_MAX_VISIBLE_ROWS)
            .min(fit.max_interior_rows.saturating_sub(1));
        self.follow_selection(results.len(), visible.min(results.len()));

        let mut rows = Vec::with_capacity(visible + 1);
        let (input, caret_column) = self.input_row(theme, fit.content_columns);
        rows.push(input);
        for index in 0..visible {
            match results.get(self.scroll + index) {
                Some(entry) => {
                    let selected = self.scroll + index == self.clamped_selection(results.len());
                    rows.push(result_row(
                        entry,
                        selected,
                        editor,
                        theme,
                        fit.content_columns,
                    ));
                },
                None => rows.push(PanelRow::new(vec![Span::new(
                    "No matching commands",
                    theme.editor.line_number,
                )])),
            }
        }
        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: fit.content_columns,
            rows,
            caret: caret_column.map(|column| PanelCaret { row: 0, column }),
            hovered: None,
        }
    }

    /// Composes the query row and reports where its caret landed.
    fn input_row(&self, theme: &Theme, width: usize) -> (PanelRow, Option<usize>) {
        let mut line = LineBuilder::new(width);
        line.push(PROMPT, theme.editor.line_number);
        let prompt_chars = PROMPT.chars().count();
        if width <= prompt_chars {
            return (PanelRow::new(line.finish()), None);
        }
        let field_width = width - prompt_chars;
        let caret = self.query.caret_column();
        let scroll = scroll_for(caret, field_width);
        let shown: String = skip_chars(self.query.text(), scroll)
            .chars()
            .take(field_width)
            .collect();
        line.push(&shown, theme.editor.foreground);
        (
            PanelRow::new(line.finish()),
            Some(prompt_chars + caret - scroll),
        )
    }

    /// What a press on composed row `row` does: the row becomes the selection
    /// and runs, which is exactly what `Enter` does to it.
    ///
    /// ⭐ **Through [`accept`](Self::accept), not beside it.** A click that
    /// resolved its own command would be a second definition of what a palette
    /// row *is*, and the two would drift the first time the ranking changed.
    ///
    /// Row zero is the query field, and a press on it selects nothing: there
    /// is one field and it already has focus, so the honest answer is that
    /// nothing happened.
    pub fn click_row(&mut self, row: usize, editor: &Editor, mru: &CommandMru) -> PaletteOutcome {
        let Some(offset) = row.checked_sub(1) else {
            return PaletteOutcome::Handled;
        };
        let index = self.scroll.saturating_add(offset);
        let count = palette::search_text(editor.commands(), mru, self.query.text(), None).len();
        if index >= count {
            return PaletteOutcome::Handled;
        }
        self.selected = index;
        self.accept(editor, mru)
    }

    /// Moves the window `delta` rows without touching the selection.
    ///
    /// Clamped at the top only; the bottom is clamped by
    /// [`follow_selection`](Self::follow_selection) on the next composition,
    /// which is the one place that knows both how many results there are and
    /// how many rows the window is showing.
    pub const fn scroll_rows(&mut self, delta: isize) {
        self.scroll = if delta < 0 {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll.saturating_add(delta.unsigned_abs())
        };
    }

    /// Resolves the selected entry and closes, or stays open with nothing to
    /// run.
    fn accept(&self, editor: &Editor, mru: &CommandMru) -> PaletteOutcome {
        let results = palette::search_text(editor.commands(), mru, self.query.text(), None);
        let Some(entry) = results.get(self.selected.min(results.len().saturating_sub(1))) else {
            return PaletteOutcome::Handled;
        };
        PaletteOutcome::Run(entry.id().clone())
    }

    /// Moves the selection by `delta`, clamping at both ends of the list.
    fn move_selection(
        &mut self,
        editor: &Editor,
        mru: &CommandMru,
        delta: isize,
    ) -> PaletteOutcome {
        let count = palette::search_text(editor.commands(), mru, self.query.text(), None).len();
        let Some(last) = count.checked_sub(1) else {
            self.selected = 0;
            return PaletteOutcome::Handled;
        };
        let current = self.selected.min(last);
        self.selected = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta.unsigned_abs()).min(last)
        };
        PaletteOutcome::Handled
    }

    /// Applies an edit to the query field.
    ///
    /// `rewrites` says whether the edit can change the text. When it did, the
    /// selection returns to the top: the old index pointed into a list that
    /// no longer exists, and the best match for the new query is the thing
    /// the user is narrowing towards.
    fn edit(&mut self, edit: impl FnOnce(&mut Entry) -> bool, rewrites: bool) -> PaletteOutcome {
        let changed = edit(&mut self.query);
        if changed && rewrites {
            self.selected = 0;
            self.scroll = 0;
            // The list the window was placed against no longer exists, so a
            // window "already following the selection" would be following an
            // index into it.
            self.followed = None;
        }
        PaletteOutcome::Handled
    }

    /// The selection, clamped against a list of `count` results.
    fn clamped_selection(&self, count: usize) -> usize {
        self.selected.min(count.saturating_sub(1))
    }

    /// Slides the scroll window so the selection is inside `visible` rows,
    /// and remembers `visible` as the page size.
    ///
    /// ⭐ **The window follows the selection when the selection *moves*, and
    /// not otherwise.** A wheel moves the window and leaves the selection
    /// where it was; a rule that re-centred on the selection every composition
    /// would undo that scroll on the very next frame, so the list would appear
    /// to spring back under the pointer. Following unconditionally was correct
    /// while the keyboard was the only thing that could move either.
    fn follow_selection(&mut self, count: usize, visible: usize) {
        self.window = visible.max(1);
        if visible == 0 || count == 0 {
            self.scroll = 0;
            self.followed = None;
            return;
        }
        let selected = self.clamped_selection(count);
        if self.followed != Some(selected) {
            self.followed = Some(selected);
            if selected < self.scroll {
                self.scroll = selected;
            } else if selected >= self.scroll + visible {
                self.scroll = selected + 1 - visible;
            }
        }
        self.scroll = self.scroll.min(count.saturating_sub(visible));
    }
}

/// Composes one result row: title, match highlighting, annotation, key hint.
fn result_row(
    entry: &PaletteEntry<'_>,
    selected: bool,
    editor: &Editor,
    theme: &Theme,
    width: usize,
) -> PanelRow {
    let base = theme.editor.foreground;
    let quiet = theme.editor.line_number;
    let matched = match_color(theme);

    let hint: Option<&str> = editor
        .key_hints()
        .primary_hint(entry.id().as_str())
        .map(|hint| hint.label(KeyLabelStyle::MacGlyphsCommandAsMeta));
    let hint_chars = hint.map_or(0, |label| label.chars().count());
    let hint_fits = hint_chars > 0 && width >= TITLE_FLOOR + HINT_GAP + hint_chars;
    let text_width = if hint_fits {
        width - HINT_GAP - hint_chars
    } else {
        width
    };

    let mut line = LineBuilder::new(text_width);
    match entry.matched_field() {
        Some(MatchField::Title) => {
            for span in highlighted_spans(entry.title(), entry.matches(), base, matched) {
                line.push(&span.text, span.color);
            }
        },
        Some(_) => {
            // The match landed in text the title does not show; show that
            // text so the highlight has something true to sit on.
            line.push(entry.title(), base);
            line.pad_to(line.used() + ANNOTATION_GAP, base);
            for span in highlighted_spans(entry.matched_text(), entry.matches(), quiet, matched) {
                line.push(&span.text, span.color);
            }
        },
        None => line.push(entry.title(), base),
    }

    let mut spans = if hint_fits {
        let used = line.used();
        let mut spans = line.finish();
        if let Some(label) = hint {
            let pad = width - hint_chars - used;
            spans.push(Span::new(" ".repeat(pad), quiet));
            spans.push(Span::new(label, quiet));
        }
        spans
    } else {
        line.finish()
    };
    if spans.is_empty() {
        spans.push(Span::new("", base));
    }
    if selected {
        PanelRow::selected(spans)
    } else {
        PanelRow::new(spans)
    }
}

/// `value` as an `isize`, saturating on a page size no window can reach.
fn isize_of(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}

/// The modifier combinations the palette distinguishes.
///
/// Shift is not part of it: it decides which character a printable key
/// produced, and the key translation has already resolved that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chord {
    /// No modifier that changes the meaning of the key.
    Plain,
    /// Control alone.
    Ctrl,
    /// Meta alone — the ⌘ spelling of the toggle that closes the panel.
    Meta,
    /// Anything else, which the modal panel swallows.
    Other,
}

/// The chord one modifier set names.
const fn chord(modifiers: Modifiers) -> Chord {
    match (modifiers.ctrl, modifiers.alt, modifiers.meta) {
        (false, false, false) => Chord::Plain,
        (true, false, false) => Chord::Ctrl,
        (false, false, true) => Chord::Meta,
        _ => Chord::Other,
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::commands::palette::CommandMru;
    use iridium_editor::theme::Theme;
    use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

    use super::{CommandPalette, PaletteOutcome};
    use crate::overlay::PanelFit;

    /// A key press with no modifiers.
    fn press(key: KeyCode) -> KeyEvent {
        KeyEvent {
            key,
            modifiers: Modifiers::none(),
            is_repeat: false,
        }
    }

    /// A bare `Ctrl` chord.
    fn ctrl(character: char) -> KeyEvent {
        KeyEvent {
            key: KeyCode::Char(character),
            modifiers: Modifiers::ctrl(),
            is_repeat: false,
        }
    }

    /// A bare ⌘ chord.
    fn meta(character: char) -> KeyEvent {
        KeyEvent {
            key: KeyCode::Char(character),
            modifiers: Modifiers {
                meta: true,
                ..Modifiers::none()
            },
            is_repeat: false,
        }
    }

    /// An open palette over a kernel carrying this face's commands and keys.
    fn open_palette() -> (CommandPalette, Editor, CommandMru) {
        let mut panel = CommandPalette::new();
        panel.open();
        let mut editor = Editor::with_defaults();
        for meta in crate::commands::command_metas() {
            editor
                .register_command(meta)
                .expect("the kernel accepted the command");
        }
        editor
            .push_keymap(crate::commands::keymap())
            .expect("the keymap validates");
        (panel, editor, CommandMru::default())
    }

    /// Types text into the panel, one key per character.
    fn type_text(panel: &mut CommandPalette, editor: &Editor, mru: &CommandMru, text: &str) {
        for character in text.chars() {
            assert_eq!(
                panel.handle_key(&press(KeyCode::Char(character)), editor, mru),
                PaletteOutcome::Handled
            );
        }
    }

    /// The composed rows as plain text at a generous size.
    fn rows_text(panel: &mut CommandPalette, editor: &Editor, mru: &CommandMru) -> Vec<String> {
        let content = panel.content(editor, mru, &Theme::dark(), PanelFit::popover(60, 13));
        content
            .rows
            .iter()
            .map(crate::overlay::PanelRow::text)
            .collect()
    }

    #[test]
    fn escape_closes_and_both_toggle_spellings_close() {
        let (mut panel, editor, mru) = open_palette();
        assert_eq!(
            panel.handle_key(&press(KeyCode::Escape), &editor, &mru),
            PaletteOutcome::Closed
        );
        assert_eq!(
            panel.handle_key(&ctrl('k'), &editor, &mru),
            PaletteOutcome::Closed,
            "the key that opened the palette closes it"
        );
        assert_eq!(
            panel.handle_key(&meta('k'), &editor, &mru),
            PaletteOutcome::Closed,
            "the mac spelling closes it too"
        );
    }

    #[test]
    fn the_panel_is_modal_and_swallows_what_it_does_not_bind() {
        let (mut panel, editor, mru) = open_palette();
        // `Ctrl+S` is save in the host's keymap; while the palette is open it
        // must reach neither the document nor the host.
        assert_eq!(
            panel.handle_key(&ctrl('s'), &editor, &mru),
            PaletteOutcome::Handled
        );
        assert_eq!(
            panel.handle_key(&meta('s'), &editor, &mru),
            PaletteOutcome::Handled,
            "the ⌘ save spelling is swallowed the same way"
        );
    }

    #[test]
    fn enter_runs_the_best_match_for_the_query() {
        let (mut panel, editor, mru) = open_palette();
        type_text(&mut panel, &editor, &mru, "show all commands");
        let outcome = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);
        let PaletteOutcome::Run(command) = outcome else {
            panic!("a query with matches must resolve to a command, got {outcome:?}");
        };
        assert_eq!(command.as_str(), "palette.open");
    }

    #[test]
    fn enter_with_no_matches_stays_open_and_runs_nothing() {
        let (mut panel, editor, mru) = open_palette();
        type_text(&mut panel, &editor, &mru, "qqzzxxjjqq");
        assert_eq!(
            panel.handle_key(&press(KeyCode::Enter), &editor, &mru),
            PaletteOutcome::Handled
        );
    }

    #[test]
    fn the_selection_clamps_at_both_ends_rather_than_wrapping() {
        let (mut panel, editor, mru) = open_palette();
        // At the top already: `Up` must stay there, not wrap to the end.
        assert_eq!(
            panel.handle_key(&press(KeyCode::Up), &editor, &mru),
            PaletteOutcome::Handled
        );
        let top = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);

        let mut fresh = CommandPalette::new();
        fresh.open();
        assert_eq!(fresh.handle_key(&press(KeyCode::Enter), &editor, &mru), top);

        // Far past the end: the selection must sit on the last entry, and one
        // more `Down` must change nothing.
        for _ in 0..12 {
            panel.handle_key(&press(KeyCode::PageDown), &editor, &mru);
        }
        let at_end = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);
        panel.handle_key(&press(KeyCode::Down), &editor, &mru);
        assert_eq!(
            panel.handle_key(&press(KeyCode::Enter), &editor, &mru),
            at_end,
            "`Down` at the last entry must not move"
        );
        assert_ne!(at_end, top, "the page keys must actually have moved");
    }

    #[test]
    fn editing_the_query_resets_the_selection_to_the_top() {
        let (mut panel, editor, mru) = open_palette();
        panel.handle_key(&press(KeyCode::Down), &editor, &mru);
        panel.handle_key(&press(KeyCode::Down), &editor, &mru);
        type_text(&mut panel, &editor, &mru, "show all commands");
        let PaletteOutcome::Run(command) = panel.handle_key(&press(KeyCode::Enter), &editor, &mru)
        else {
            panic!("the query has a match");
        };
        assert_eq!(
            command.as_str(),
            "palette.open",
            "the selection must be back on the best match, not two rows down"
        );
    }

    #[test]
    fn a_motion_in_the_query_field_keeps_the_selection() {
        let (mut panel, editor, mru) = open_palette();
        panel.handle_key(&press(KeyCode::Down), &editor, &mru);
        let selected = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);
        panel.handle_key(&press(KeyCode::Home), &editor, &mru);
        panel.handle_key(&press(KeyCode::Left), &editor, &mru);
        assert_eq!(
            panel.handle_key(&press(KeyCode::Enter), &editor, &mru),
            selected,
            "a caret motion changes no text and must not reset the selection"
        );
    }

    #[test]
    fn the_panel_lists_commands_and_their_mac_key_hints() {
        let (mut panel, editor, mru) = open_palette();
        type_text(&mut panel, &editor, &mru, "select all");
        let rows = rows_text(&mut panel, &editor, &mru);
        let hit = rows
            .iter()
            .find(|row| row.contains("Select All"))
            .expect("the Select All command is listed");
        // The kernel decides which binding is primary; what this face decides
        // is the spelling — mac glyphs, so `Ctrl+A` reads `⌃A` and the ⌘
        // layer's rows read `⌘…`, never the portable `Ctrl+` text.
        assert!(
            hit.contains("⌃A") || hit.contains("⌘A"),
            "the row must carry the key in mac glyphs: {hit:?}"
        );
        assert!(
            !hit.contains("Ctrl"),
            "the portable spelling must not appear on this face: {hit:?}"
        );
    }

    #[test]
    fn a_query_with_no_matches_says_so_in_the_panel() {
        let (mut panel, editor, mru) = open_palette();
        type_text(&mut panel, &editor, &mru, "qqzzxxjjqq");
        let rows = rows_text(&mut panel, &editor, &mru);
        assert!(
            rows.iter().any(|row| row.contains("No matching commands")),
            "an empty result list must be stated, not blank"
        );
    }

    #[test]
    fn the_query_is_shown_in_the_input_row_with_the_caret_after_it() {
        let (mut panel, editor, mru) = open_palette();
        type_text(&mut panel, &editor, &mru, "fold");
        let content = panel.content(&editor, &mru, &Theme::dark(), PanelFit::popover(60, 13));
        assert!(
            content.rows[0].text().starts_with("> fold"),
            "the query is visible: {:?}",
            content.rows[0].text()
        );
        let caret = content.caret.expect("the query field has a caret");
        assert_eq!(caret.row, 0, "the caret sits in the input row");
        assert_eq!(
            caret.column,
            2 + "fold".len(),
            "the caret sits after the query text"
        );
    }

    #[test]
    fn one_result_row_is_marked_selected() {
        let (mut panel, editor, mru) = open_palette();
        let content = panel.content(&editor, &mru, &Theme::dark(), PanelFit::popover(60, 13));
        let selected: Vec<usize> = content
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.selected.then_some(index))
            .collect();
        assert_eq!(selected, vec![1], "the first result is the selection");
    }

    #[test]
    fn a_match_outside_the_title_is_annotated_beside_it() {
        let (mut panel, editor, mru) = open_palette();
        // "w!" is an alias of this face's Save Anyway, not part of its title.
        type_text(&mut panel, &editor, &mru, "w!");
        let rows = rows_text(&mut panel, &editor, &mru);
        let hit = rows
            .iter()
            .find(|row| row.contains("Save Anyway"))
            .expect("the alias match lists the command");
        assert!(
            hit.contains("w!"),
            "the matched alias must be shown so the highlight sits on truth: {hit:?}"
        );
    }

    #[test]
    fn opening_again_clears_the_query() {
        let (mut panel, editor, mru) = open_palette();
        type_text(&mut panel, &editor, &mru, "fold");
        assert_eq!(panel.query(), "fold");
        panel.open();
        assert_eq!(panel.query(), "", "a palette opens aimed at nothing");
    }

    #[test]
    fn a_paste_lands_in_the_query_with_control_characters_dropped() {
        let (mut panel, editor, mru) = open_palette();
        panel.handle_key(&press(KeyCode::Down), &editor, &mru);
        panel.paste("se\nlect");
        assert_eq!(panel.query(), "select");
        // The paste changed the text, so the selection is back at the top.
        let outcome = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);
        let mut fresh = CommandPalette::new();
        fresh.open();
        fresh.paste("select");
        assert_eq!(
            fresh.handle_key(&press(KeyCode::Enter), &editor, &mru),
            outcome
        );
    }

    #[test]
    fn matched_characters_are_recoloured_on_the_clusters_they_name() {
        let spans = super::highlighted_spans(
            "Save Anyway",
            &[0, 5],
            iridium_editor::theme::Color::new(1.0, 1.0, 1.0, 1.0),
            iridium_editor::theme::Color::new(1.0, 0.0, 0.0, 1.0),
        );
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(text, "Save Anyway", "recolouring loses no text");
        assert_eq!(spans[0].text, "S", "the first matched character");
        assert_eq!(spans[1].text, "ave ");
        assert_eq!(spans[2].text, "A", "the second matched character");
    }
}
