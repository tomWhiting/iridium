//! The palette's state: the query, the selection, and what reaches the screen.
//!
//! [`super`] carries the argument for what the palette *is* and why its engine
//! is the kernel's; this file is the state that argument is about, and the
//! composition that turns it into rows. The keys that change it live in
//! [`super::keys`], and the table they resolve against in [`super::keymap`].

use iridium_editor::commands::palette::{self, CommandMru, MatchField, PaletteEntry};
use iridium_editor::theme::Theme;
use iridium_editor::{CommandId, Editor, KeyLabelStyle, KeyPress, KeymapStack};

use super::keymap::default_keymap;
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The keymap the panel resolves its keys against.
    ///
    /// [`super::keymap::default_keymap`] at the base; a face pushes the user's
    /// `[keys]` bindings on top through
    /// [`set_user_keymap`](Self::set_user_keymap), which filters them to this
    /// panel's verbs.
    pub(super) keys: KeymapStack,
    /// The strokes of a multi-stroke sequence typed so far.
    ///
    /// Almost always empty: the default table is all single strokes, and only a
    /// sequence a *user* bound can leave anything here between presses.
    pub(super) pending: Vec<KeyPress>,
}

impl Default for CommandPalette {
    /// The same panel [`new`](Self::new) builds.
    ///
    /// ⚠️ **Written out rather than derived.** A derived `Default` would give
    /// an empty [`KeymapStack`], so a palette built that way would answer no
    /// keys at all — a panel that looks constructed and is deaf.
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    /// A closed palette with an empty query and the default bindings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            query: Entry::new(),
            selected: 0,
            scroll: 0,
            window: PANEL_MAX_VISIBLE_ROWS,
            followed: None,
            keys: KeymapStack::with_base(default_keymap()),
            pending: Vec::new(),
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
        // A leader stroke pressed before the panel last closed means nothing
        // now, and leaving it would read the user's next key as its
        // continuation — the same reason the editor's resolver resets on blur.
        self.pending.clear();
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

    /// The page the page keys hop by: whatever the last composition showed.
    ///
    /// Never zero, so a panel that has not reached the screen yet still moves.
    pub(super) const fn page(&self) -> usize {
        // `max` rather than a clamp on the field, because a window of zero is
        // a truthful answer to "how many rows did the last composition show"
        // and it is only *this* question that needs a floor.
        if self.window == 0 { 1 } else { self.window }
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
    pub(super) fn accept(&self, editor: &Editor, mru: &CommandMru) -> PaletteOutcome {
        let results = palette::search_text(editor.commands(), mru, self.query.text(), None);
        let Some(entry) = results.get(self.selected.min(results.len().saturating_sub(1))) else {
            return PaletteOutcome::Handled;
        };
        PaletteOutcome::Run(entry.id().clone())
    }

    /// Moves the selection by `delta`, clamping at both ends of the list.
    pub(super) fn move_selection(
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
    pub(super) fn edit(
        &mut self,
        edit: impl FnOnce(&mut Entry) -> bool,
        rewrites: bool,
    ) -> PaletteOutcome {
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
pub(super) fn isize_of(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}
