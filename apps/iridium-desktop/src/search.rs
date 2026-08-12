//! The search and replace overlay: state and rows, no pixels.
//!
//! This is the desktop counterpart of the terminal face's search panel, and
//! it keeps that panel's semantics exactly — the terminal's test suite is the
//! specification. The kernel's find/replace engine has existed for months
//! with a face in the terminal only; this brings it to the window.
//!
//! # The engine is the kernel's and none of it is repeated here
//!
//! Nothing in this module searches, matches, compiles a pattern, walks to the
//! next hit, or edits the document. Every one of those is a verb [`Editor`]
//! already has. What is here is two fields, a key map and rows for
//! [`crate::overlay`] to paint.
//!
//! The kernel calls made, and nothing else:
//!
//! | Doing | Kernel verb |
//! |---|---|
//! | Query or option changed | [`Editor::find`] |
//! | Next / previous match | [`Editor::goto_next_match`] / [`Editor::goto_previous_match`] |
//! | Replace one / all | [`Editor::replace_current_match`] / [`Editor::replace_all_matches`] |
//! | Escape | [`Editor::close_search`] |
//! | Feedback | [`Editor::search_match_count`], [`Editor::current_match_index`] |
//!
//! [`Editor::find`] rather than `update_search`, for the reason the terminal
//! face records: it is the one verb that re-runs the search *and* moves to
//! the match nearest the caret, which is what makes typing into the query
//! field incremental.
//!
//! # An invalid pattern is a normal state
//!
//! A user typing `[a-z` has typed a regex that does not compile, and will
//! type the `]` next. [`Editor::find`] reports that as `Err`, and the panel
//! keeps it: the count segment says `Invalid regex` and the feedback row
//! carries the engine's own words. Nothing is swallowed, nothing panics, and
//! every other key keeps working.
//!
//! # Keys
//!
//! | Key | Action |
//! |---|---|
//! | any printable | insert into the focused field |
//! | `Left` `Right` `Home` `End` `Backspace` `Delete` | edit the focused field |
//! | `Tab` | move between the query and the replacement |
//! | `Enter`, `Down` | next match (`Shift+Enter`, `Up` previous) |
//! | `Ctrl+R` | replace the current match |
//! | `Ctrl+Alt+R` | replace every match |
//! | `Alt+C` `Alt+W` `Alt+R` | case-sensitive, whole-word, regex |
//! | `Escape` | close |
//!
//! Anything else is [`SearchOutcome::Ignored`] and stays the host's to
//! handle, so a save chord is not dead while the panel is open — the panel is
//! deliberately *not* modal, unlike the palette and the undo tree.

use iridium_editor::search::SearchOptions;
use iridium_editor::theme::{Color, Theme};
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

use crate::line::{LineBuilder, skip_chars};
use crate::overlay::{PanelAnchor, PanelCaret, PanelContent, PanelFit, PanelRow, scroll_for};
use crate::prompt::Entry;

/// The width of the label column, which both labels are padded to.
const LABEL_WIDTH: usize = 8;

/// The label of the query field, padded to [`LABEL_WIDTH`].
const FIND_LABEL: &str = "Find    ";

/// The label of the replacement field, padded to [`LABEL_WIDTH`].
const REPLACE_LABEL: &str = "Replace ";

/// The narrowest field worth showing. Below this the right-hand segments go.
const MIN_FIELD_WIDTH: usize = 8;

/// The characters one toggle occupies: two of label between two of bracket.
const TOGGLE_WIDTH: usize = 4;

/// The characters all three toggles occupy, with one blank between each pair.
const TOGGLES_WIDTH: usize = TOGGLE_WIDTH * 3 + 2;

/// The blank characters between the toggles and the match count.
const COUNT_GAP: usize = 2;

/// The blank character kept between the field and whatever is to its right.
const FIELD_GAP: usize = 1;

/// What became of a key handed to the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOutcome {
    /// The overlay does not bind this key. It is still the host's to handle.
    Ignored,
    /// The overlay handled it and the document is unchanged.
    Handled,
    /// The overlay handled it and the document changed, replacing this many
    /// matches.
    Replaced(usize),
    /// The overlay was closed, and the kernel's search with it.
    Closed,
}

/// Which of the two fields keystrokes go into.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Focus {
    /// The query field.
    #[default]
    Find,
    /// The replacement field.
    Replace,
}

/// What the last action had to say, when it had anything.
///
/// Separate from the query's validity: a query can be invalid *and* a
/// replacement have just been refused, and the two are reported by different
/// segments rather than overwriting each other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Feedback {
    /// Nothing to report.
    #[default]
    None,
    /// A replacement ran and changed this many matches.
    Replaced(usize),
    /// A replacement was asked for and there was nothing to replace.
    NoMatch,
    /// A replacement was asked for in a document that cannot be edited.
    ReadOnly,
}

/// The search and replace overlay.
///
/// The host owns one of these and keeps it across closes, which is what makes
/// the query survive: closing clears the kernel's search but leaves the text
/// in the field, so re-opening offers the last query back.
#[derive(Debug, Clone, Default)]
pub struct SearchOverlay {
    /// The query field.
    query: Entry,
    /// The replacement field.
    replacement: Entry,
    /// Which field keystrokes go into.
    focus: Focus,
    /// The options the search runs with.
    options: SearchOptions,
    /// The engine's complaint about the current query, if it has one.
    error: Option<String>,
    /// What the last action had to say.
    feedback: Feedback,
}

impl SearchOverlay {
    /// A panel with empty fields and default options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the panel, re-running whatever query it already holds.
    ///
    /// Re-running matters when a previous search was closed: the fields kept
    /// their text but [`Editor::close_search`] cleared the kernel's, so
    /// without this the panel would show a query nothing was searching for.
    pub fn open(&mut self, editor: &mut Editor) {
        self.focus = Focus::Find;
        self.feedback = Feedback::None;
        self.refresh(editor);
    }

    /// Puts the caret in the field on composed row `row`.
    ///
    /// ⭐ **This panel has no list, and that is the whole shape of what a
    /// click means here.** Its two rows are the Find field and the Replace
    /// field, so a press is a request to type into one of them — the same
    /// thing `Tab` asks for, said by pointing instead. A press anywhere else
    /// on the panel changes nothing.
    ///
    /// A window with room for one row shows whichever field has focus, so row
    /// zero is already that field and the press is a no-op rather than a jump
    /// to Find.
    pub const fn click_row(&mut self, row: usize, rows_shown: usize) {
        if rows_shown < 2 {
            return;
        }
        match row {
            0 => self.focus = Focus::Find,
            1 => self.focus = Focus::Replace,
            _ => {},
        }
    }

    /// Closes the panel and the kernel's search with it.
    ///
    /// The field text is kept; the kernel's state is not.
    pub fn close(&mut self, editor: &mut Editor) {
        editor.close_search();
        self.error = None;
        self.feedback = Feedback::None;
    }

    /// The query field's text.
    #[must_use]
    pub fn query(&self) -> &str {
        self.query.text()
    }

    /// The replacement field's text.
    #[must_use]
    pub fn replacement(&self) -> &str {
        self.replacement.text()
    }

    /// The engine's complaint about the current query, if it has one.
    ///
    /// `Some` means the query does not compile — almost always a half-typed
    /// regex. It is a state, not a failure: the panel keeps working and the
    /// next keystroke may well fix it.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Handles one key press, driving the kernel for anything it binds.
    ///
    /// Returns [`SearchOutcome::Ignored`] for a key it does not bind, which
    /// the host is then free to act on itself.
    pub fn handle_key(&mut self, event: &KeyEvent, editor: &mut Editor) -> SearchOutcome {
        match (chord(event.modifiers), event.key) {
            (Chord::Plain, KeyCode::Escape) => {
                self.close(editor);
                SearchOutcome::Closed
            },
            (Chord::Plain, KeyCode::Enter) => {
                if event.modifiers.shift {
                    editor.goto_previous_match();
                } else {
                    editor.goto_next_match();
                }
                SearchOutcome::Handled
            },
            (Chord::Plain, KeyCode::Down) => {
                editor.goto_next_match();
                SearchOutcome::Handled
            },
            (Chord::Plain, KeyCode::Up) => {
                editor.goto_previous_match();
                SearchOutcome::Handled
            },
            (Chord::Plain, KeyCode::Tab) => {
                self.focus = match self.focus {
                    Focus::Find => Focus::Replace,
                    Focus::Replace => Focus::Find,
                };
                SearchOutcome::Handled
            },
            (Chord::Plain, KeyCode::Left) => self.edit(editor, Entry::move_left, false),
            (Chord::Plain, KeyCode::Right) => self.edit(editor, Entry::move_right, false),
            (Chord::Plain, KeyCode::Home) => self.edit(editor, Entry::move_home, false),
            (Chord::Plain, KeyCode::End) => self.edit(editor, Entry::move_end, false),
            (Chord::Plain, KeyCode::Backspace) => self.edit(editor, Entry::backspace, true),
            (Chord::Plain, KeyCode::Delete) => self.edit(editor, Entry::delete, true),
            (Chord::Plain, KeyCode::Char(character)) => {
                self.edit(editor, |field| field.insert(character), true)
            },
            (Chord::Alt, KeyCode::Char(character)) => self.toggle(editor, character),
            (Chord::Ctrl, KeyCode::Char('r' | 'R')) => self.replace_current(editor),
            (Chord::CtrlAlt, KeyCode::Char('r' | 'R')) => self.replace_all(editor),
            _ => SearchOutcome::Ignored,
        }
    }

    /// Composes the panel for painting: the Find row with the toggles and the
    /// match count, the Replace row with the feedback, the caret on whichever
    /// field has focus.
    ///
    /// A window with room for only one interior row shows the focused field —
    /// keystrokes must always go somewhere visible.
    #[must_use]
    pub fn content(&self, editor: &Editor, theme: &Theme, fit: PanelFit) -> PanelContent {
        let (rows, caret) = if fit.max_interior_rows >= 2 {
            let (find, find_caret) = self.row(editor, theme, fit, Focus::Find, Right::Counters);
            let (replace, replace_caret) =
                self.row(editor, theme, fit, Focus::Replace, Right::Feedback);
            let caret = match self.focus {
                Focus::Find => find_caret.map(|column| PanelCaret { row: 0, column }),
                Focus::Replace => replace_caret.map(|column| PanelCaret { row: 1, column }),
            };
            (vec![find, replace], caret)
        } else {
            let (row, row_caret) = self.row(editor, theme, fit, self.focus, Right::Counters);
            let caret = row_caret.map(|column| PanelCaret { row: 0, column });
            (vec![row], caret)
        };
        PanelContent {
            anchor: PanelAnchor::Bottom,
            content_columns: fit.content_columns,
            rows,
            caret,
            hovered: None,
        }
    }

    /// Composes one panel row and reports where its field's caret landed.
    ///
    /// Every part degrades rather than colliding, exactly as the terminal
    /// face's panel does: the field keeps a floor of [`MIN_FIELD_WIDTH`]
    /// characters and the right-hand segments are dropped, widest group
    /// first, until what is left fits. The feedback message is *clipped*
    /// rather than dropped — the beginning of a regex error is worth more
    /// than nothing at all.
    fn row(
        &self,
        editor: &Editor,
        theme: &Theme,
        fit: PanelFit,
        field: Focus,
        right: Right,
    ) -> (PanelRow, Option<usize>) {
        let width = fit.content_columns;
        let quiet = theme.editor.line_number;
        let mut line = LineBuilder::new(width);
        let label = match field {
            Focus::Find => FIND_LABEL,
            Focus::Replace => REPLACE_LABEL,
        };
        line.push(label, quiet);
        if width <= LABEL_WIDTH {
            return (PanelRow::new(line.finish()), None);
        }

        let budget = width.saturating_sub(LABEL_WIDTH + MIN_FIELD_WIDTH + FIELD_GAP);
        let segments = match right {
            Right::Counters => self.counter_spans(editor, theme, budget),
            Right::Feedback => self.feedback_spans(theme, budget),
        };
        let used: usize = segments.iter().map(|(text, _)| text.chars().count()).sum();
        let reserved = if used == 0 { 0 } else { used + FIELD_GAP };
        let field_width = width - LABEL_WIDTH - reserved;

        let entry = match field {
            Focus::Find => &self.query,
            Focus::Replace => &self.replacement,
        };
        let caret_column = entry.caret_column();
        let scroll = scroll_for(caret_column, field_width);
        let color = if field == self.focus {
            theme.editor.foreground
        } else {
            quiet
        };
        let shown: String = skip_chars(entry.text(), scroll)
            .chars()
            .take(field_width)
            .collect();
        line.push(&shown, color);
        line.pad_to(width - used, quiet);
        for (text, segment_color) in segments {
            line.push(&text, segment_color);
        }

        let caret = (field == self.focus && caret_column >= scroll)
            .then_some(LABEL_WIDTH + caret_column - scroll);
        (PanelRow::new(line.finish()), caret)
    }

    /// The toggle and count segments that fit `budget`, in drawing order.
    ///
    /// Widest group first, then the count alone, then the toggles alone. The
    /// count outranks the toggles because it is the only segment that answers
    /// the question the user is asking; the toggles merely say how it was
    /// asked.
    fn counter_spans(&self, editor: &Editor, theme: &Theme, budget: usize) -> Vec<(String, Color)> {
        let (count, is_error) = self.count_segment(editor);
        let count_chars = count.chars().count();
        let (with_toggles, with_count) =
            if count_chars > 0 && budget >= TOGGLES_WIDTH + COUNT_GAP + count_chars {
                (true, true)
            } else if count_chars > 0 && budget >= count_chars {
                (false, true)
            } else if budget >= TOGGLES_WIDTH {
                (true, false)
            } else {
                return Vec::new();
            };

        let mut segments = Vec::new();
        if with_toggles {
            for (index, (label, is_active)) in [
                ("Aa", self.options.case_sensitive),
                ("\\b", self.options.whole_word),
                (".*", self.options.regex),
            ]
            .into_iter()
            .enumerate()
            {
                // An active toggle is bracketed — `[Aa]` — so state survives
                // any colour the theme picks.
                let text = if is_active {
                    format!("[{label}]")
                } else {
                    format!(" {label} ")
                };
                let color = if is_active {
                    theme.editor.line_number_active
                } else {
                    theme.editor.line_number
                };
                if index > 0 {
                    segments.push((" ".to_owned(), theme.editor.line_number));
                }
                segments.push((text, color));
            }
        }
        if with_count {
            if with_toggles {
                segments.push((" ".repeat(COUNT_GAP), theme.editor.line_number));
            }
            let color = if is_error {
                theme.editor.diagnostic_error
            } else {
                theme.editor.foreground
            };
            segments.push((count, color));
        }
        segments
    }

    /// The feedback segment clipped to `budget`, or nothing to say.
    fn feedback_spans(&self, theme: &Theme, budget: usize) -> Vec<(String, Color)> {
        let Some((message, is_error)) = self.feedback_segment() else {
            return Vec::new();
        };
        let clipped: String = message.chars().take(budget).collect();
        if clipped.is_empty() {
            return Vec::new();
        }
        let color = if is_error {
            theme.editor.diagnostic_error
        } else {
            theme.editor.foreground
        };
        vec![(clipped, color)]
    }

    /// The match-count segment, and whether it reports a failure.
    ///
    /// The counts are the kernel's, so the panel cannot disagree with what is
    /// highlighted. An empty query says nothing at all rather than `0 of 0`:
    /// the user has not asked a question yet.
    fn count_segment(&self, editor: &Editor) -> (String, bool) {
        if self.query.text().is_empty() {
            return (String::new(), false);
        }
        if self.error.is_some() {
            let text = if self.options.regex {
                "Invalid regex"
            } else {
                "Invalid query"
            };
            return (text.to_owned(), true);
        }
        let total = editor.search_match_count();
        if total == 0 {
            return ("No matches".to_owned(), false);
        }
        editor.current_match_index().map_or_else(
            || (format!("{total} matches"), false),
            |index| (format!("{} of {total}", index + 1), false),
        )
    }

    /// The feedback row's text, and whether it reports a failure.
    ///
    /// An unusable query outranks everything else: the count segment only has
    /// room to say *that* the pattern is broken, and this is where the engine
    /// says how.
    fn feedback_segment(&self) -> Option<(String, bool)> {
        if let Some(message) = self.error() {
            return Some((message.to_owned(), true));
        }
        Some(match self.feedback {
            Feedback::None => return None,
            Feedback::Replaced(1) => ("Replaced 1 match".to_owned(), false),
            Feedback::Replaced(count) => (format!("Replaced {count} matches"), false),
            Feedback::NoMatch => ("No match to replace".to_owned(), true),
            Feedback::ReadOnly => ("Document is read-only".to_owned(), true),
        })
    }

    /// Applies an edit to the focused field, re-searching when it changed the
    /// query.
    ///
    /// `rewrites` says whether the edit can change the field's text. A motion
    /// cannot, so it never re-runs the search; a caret moving in the query
    /// field must not cost a pass over the document.
    fn edit(
        &mut self,
        editor: &mut Editor,
        edit: impl FnOnce(&mut Entry) -> bool,
        rewrites: bool,
    ) -> SearchOutcome {
        let is_query = self.focus == Focus::Find;
        let field = match self.focus {
            Focus::Find => &mut self.query,
            Focus::Replace => &mut self.replacement,
        };
        let changed = edit(field);
        if changed && rewrites && is_query {
            self.refresh(editor);
            self.feedback = Feedback::None;
        }
        SearchOutcome::Handled
    }

    /// Flips one option and re-runs the search with it.
    fn toggle(&mut self, editor: &mut Editor, character: char) -> SearchOutcome {
        let option = match character.to_ascii_lowercase() {
            'c' => &mut self.options.case_sensitive,
            'w' => &mut self.options.whole_word,
            'r' => &mut self.options.regex,
            _ => return SearchOutcome::Ignored,
        };
        *option = !*option;
        self.refresh(editor);
        self.feedback = Feedback::None;
        SearchOutcome::Handled
    }

    /// Replaces the match the caret is on.
    fn replace_current(&mut self, editor: &mut Editor) -> SearchOutcome {
        if editor.state().read_only {
            self.feedback = Feedback::ReadOnly;
            return SearchOutcome::Handled;
        }
        if editor.replace_current_match(self.replacement.text()) {
            self.feedback = Feedback::Replaced(1);
            return SearchOutcome::Replaced(1);
        }
        self.feedback = Feedback::NoMatch;
        SearchOutcome::Handled
    }

    /// Replaces every match, as one undoable step.
    ///
    /// [`Editor::replace_all_matches`] closes the kernel's search when it
    /// changes anything, because every match it knew about is gone. The panel
    /// is still open with a query in it, so the search is re-run afterwards:
    /// a visible query field that nothing is searching for is a lie.
    fn replace_all(&mut self, editor: &mut Editor) -> SearchOutcome {
        if editor.state().read_only {
            self.feedback = Feedback::ReadOnly;
            return SearchOutcome::Handled;
        }
        let count = editor.replace_all_matches(self.replacement.text());
        self.refresh(editor);
        if count == 0 {
            self.feedback = Feedback::NoMatch;
            return SearchOutcome::Handled;
        }
        self.feedback = Feedback::Replaced(count);
        SearchOutcome::Replaced(count)
    }

    /// Runs the query against the document, keeping the engine's complaint if
    /// it has one.
    ///
    /// The error is *stored*, not returned and not logged away: it is the
    /// only thing that tells the user why a query they are still typing finds
    /// nothing.
    fn refresh(&mut self, editor: &mut Editor) {
        self.error = editor.find(self.query.text(), &self.options).err();
    }
}

/// What the right-hand end of a panel row carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Right {
    /// The option toggles and the match count.
    Counters,
    /// The feedback message from the last action.
    Feedback,
}

/// The modifier combinations the overlay distinguishes.
///
/// Shift is deliberately not part of it: it decided which *character* a
/// printable key produced, which the key translation has already resolved,
/// and it selects between next and previous on `Enter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chord {
    /// No modifier that changes the meaning of the key.
    Plain,
    /// Alt alone.
    Alt,
    /// Control alone.
    Ctrl,
    /// Control and Alt together.
    CtrlAlt,
    /// Anything involving meta, which the overlay leaves to the host.
    Other,
}

/// The chord one modifier set names.
const fn chord(modifiers: Modifiers) -> Chord {
    match (modifiers.ctrl, modifiers.alt, modifiers.meta) {
        (false, false, false) => Chord::Plain,
        (false, true, false) => Chord::Alt,
        (true, false, false) => Chord::Ctrl,
        (true, true, false) => Chord::CtrlAlt,
        _ => Chord::Other,
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::theme::Theme;
    use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

    use super::{SearchOutcome, SearchOverlay};
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

    /// An `Alt` chord.
    fn alt(character: char) -> KeyEvent {
        chord(
            KeyCode::Char(character),
            Modifiers {
                alt: true,
                ..Modifiers::none()
            },
        )
    }

    /// The pointer's way into a two-field panel: pressing a field is the same
    /// request `Tab` makes, said by pointing instead.
    #[test]
    fn a_press_on_the_replace_row_puts_the_caret_in_it() {
        let (mut panel, editor) = open_over("hello world");
        let theme = Theme::dark();
        let fit = PanelFit::popover(40, 12);
        assert_eq!(
            panel
                .content(&editor, &theme, fit)
                .caret
                .map(|caret| caret.row),
            Some(0),
            "the panel opens with the caret in Find"
        );

        panel.click_row(1, 2);
        assert_eq!(
            panel
                .content(&editor, &theme, fit)
                .caret
                .map(|caret| caret.row),
            Some(1),
            "the press moved the caret into Replace"
        );

        panel.click_row(0, 2);
        assert_eq!(
            panel
                .content(&editor, &theme, fit)
                .caret
                .map(|caret| caret.row),
            Some(0),
            "and back again"
        );
    }

    /// ⚠️ A window with room for one row is already showing the focused field,
    /// so a press on row zero must not silently mean "go to Find".
    #[test]
    fn a_one_row_panel_keeps_the_field_it_is_showing() {
        let (mut panel, editor) = open_over("hello world");
        let theme = Theme::dark();
        let narrow = PanelFit::popover(40, 1);
        panel.click_row(1, 2);
        panel.click_row(0, 1);
        assert_eq!(
            panel
                .content(&editor, &theme, narrow)
                .caret
                .map(|caret| caret.row),
            Some(0),
            "the one row on screen is Replace, and it still has the caret"
        );
        let roomy = PanelFit::popover(40, 12);
        assert_eq!(
            panel
                .content(&editor, &theme, roomy)
                .caret
                .map(|caret| caret.row),
            Some(1),
            "which the two-row composition confirms is still Replace"
        );
    }

    /// An open panel over a kernel holding `text`.
    fn open_over(text: &str) -> (SearchOverlay, Editor) {
        let mut editor = Editor::with_defaults();
        editor.set_content(text);
        let mut overlay = SearchOverlay::new();
        overlay.open(&mut editor);
        (overlay, editor)
    }

    /// Types text into the panel, one key per character.
    fn type_text(overlay: &mut SearchOverlay, editor: &mut Editor, text: &str) {
        for character in text.chars() {
            assert_eq!(
                overlay.handle_key(&press(KeyCode::Char(character)), editor),
                SearchOutcome::Handled
            );
        }
    }

    /// The panel's rows as plain text at a generous width.
    fn rows_text(overlay: &SearchOverlay, editor: &Editor) -> Vec<String> {
        let content = overlay.content(editor, &Theme::dark(), PanelFit::popover(60, 12));
        content.rows.iter().map(super::PanelRow::text).collect()
    }

    #[test]
    fn typing_a_query_searches_the_document_as_it_goes() {
        let (mut overlay, mut editor) = open_over("alpha beta alpha");
        type_text(&mut overlay, &mut editor, "alpha");
        assert_eq!(editor.search_match_count(), 2);
    }

    #[test]
    fn the_count_is_one_based_and_names_the_current_match() {
        let (mut overlay, mut editor) = open_over("one two one");
        type_text(&mut overlay, &mut editor, "one");
        let rows = rows_text(&overlay, &editor);
        assert!(
            rows[0].contains("1 of 2"),
            "the count segment reads honestly: {rows:?}"
        );
    }

    #[test]
    fn enter_walks_forwards_through_the_matches() {
        let (mut overlay, mut editor) = open_over("x x x");
        type_text(&mut overlay, &mut editor, "x");
        assert_eq!(editor.current_match_index(), Some(0));
        overlay.handle_key(&press(KeyCode::Enter), &mut editor);
        assert_eq!(editor.current_match_index(), Some(1));
        overlay.handle_key(&press(KeyCode::Up), &mut editor);
        assert_eq!(editor.current_match_index(), Some(0));
    }

    #[test]
    fn a_query_with_no_matches_says_so_rather_than_counting_to_zero() {
        let (mut overlay, mut editor) = open_over("alpha");
        type_text(&mut overlay, &mut editor, "zz");
        let rows = rows_text(&overlay, &editor);
        assert!(rows[0].contains("No matches"), "{rows:?}");
    }

    #[test]
    fn an_empty_query_makes_no_claim_at_all() {
        let (overlay, editor) = open_over("alpha");
        let rows = rows_text(&overlay, &editor);
        assert!(!rows[0].contains("of"), "{rows:?}");
        assert!(!rows[0].contains("matches"), "{rows:?}");
    }

    #[test]
    fn escape_closes_the_kernels_search_and_keeps_the_query() {
        let (mut overlay, mut editor) = open_over("alpha");
        type_text(&mut overlay, &mut editor, "alpha");
        assert_eq!(
            overlay.handle_key(&press(KeyCode::Escape), &mut editor),
            SearchOutcome::Closed
        );
        assert_eq!(editor.search_match_count(), 0, "the kernel's search closed");
        assert_eq!(overlay.query(), "alpha", "the field keeps its text");

        // Re-opening re-runs the kept query.
        overlay.open(&mut editor);
        assert_eq!(editor.search_match_count(), 1);
    }

    #[test]
    fn toggles_re_run_the_search_and_read_bracketed() {
        let (mut overlay, mut editor) = open_over("Alpha alpha");
        type_text(&mut overlay, &mut editor, "alpha");
        assert_eq!(editor.search_match_count(), 2);
        assert_eq!(
            overlay.handle_key(&alt('c'), &mut editor),
            SearchOutcome::Handled
        );
        assert_eq!(editor.search_match_count(), 1, "case sensitivity narrowed");
        let rows = rows_text(&overlay, &editor);
        assert!(
            rows[0].contains("[Aa]"),
            "the active toggle brackets itself"
        );
        assert!(rows[0].contains(" \\b "), "the inactive toggle does not");
    }

    #[test]
    fn an_unfinished_regex_is_feedback_rather_than_a_crash() {
        let (mut overlay, mut editor) = open_over("alpha");
        assert_eq!(
            overlay.handle_key(&alt('r'), &mut editor),
            SearchOutcome::Handled
        );
        type_text(&mut overlay, &mut editor, "[a-z");
        assert!(overlay.error().is_some(), "the engine's complaint is kept");
        let rows = rows_text(&overlay, &editor);
        assert!(rows[0].contains("Invalid regex"), "{rows:?}");

        // The `]` fixes it.
        type_text(&mut overlay, &mut editor, "]");
        assert!(overlay.error().is_none());
        assert_eq!(editor.search_match_count(), 5);
    }

    #[test]
    fn tab_moves_typing_into_the_replacement_without_disturbing_the_search() {
        let (mut overlay, mut editor) = open_over("aaa");
        type_text(&mut overlay, &mut editor, "a");
        let count = editor.search_match_count();
        overlay.handle_key(&press(KeyCode::Tab), &mut editor);
        type_text(&mut overlay, &mut editor, "b");
        assert_eq!(overlay.query(), "a");
        assert_eq!(overlay.replacement(), "b");
        assert_eq!(editor.search_match_count(), count);
    }

    #[test]
    fn replacing_the_current_match_edits_the_document_and_reports_it() {
        let (mut overlay, mut editor) = open_over("one two one");
        type_text(&mut overlay, &mut editor, "one");
        overlay.handle_key(&press(KeyCode::Tab), &mut editor);
        type_text(&mut overlay, &mut editor, "three");
        assert_eq!(
            overlay.handle_key(&chord(KeyCode::Char('r'), Modifiers::ctrl()), &mut editor),
            SearchOutcome::Replaced(1)
        );
        assert_eq!(editor.content(), "three two one");
        let rows = rows_text(&overlay, &editor);
        assert!(rows[1].contains("Replaced 1 match"), "{rows:?}");
    }

    #[test]
    fn replacing_every_match_reports_the_count_and_keeps_searching() {
        let (mut overlay, mut editor) = open_over("x y x y x");
        type_text(&mut overlay, &mut editor, "x");
        overlay.handle_key(&press(KeyCode::Tab), &mut editor);
        type_text(&mut overlay, &mut editor, "z");
        let ctrl_alt = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        };
        assert_eq!(
            overlay.handle_key(&chord(KeyCode::Char('r'), ctrl_alt), &mut editor),
            SearchOutcome::Replaced(3)
        );
        assert_eq!(editor.content(), "z y z y z");
        // The panel still shows a query, so the kernel must still be
        // searching for it — for zero matches now, honestly.
        assert_eq!(editor.search_match_count(), 0);
        let rows = rows_text(&overlay, &editor);
        assert!(rows[1].contains("Replaced 3 matches"), "{rows:?}");
    }

    #[test]
    fn a_read_only_document_refuses_a_replacement_and_says_why() {
        let (mut overlay, mut editor) = open_over("aaa");
        editor.state_mut().read_only = true;
        type_text(&mut overlay, &mut editor, "a");
        assert_eq!(
            overlay.handle_key(&chord(KeyCode::Char('r'), Modifiers::ctrl()), &mut editor),
            SearchOutcome::Handled
        );
        assert_eq!(editor.content(), "aaa");
        let rows = rows_text(&overlay, &editor);
        assert!(rows[1].contains("read-only"), "{rows:?}");
    }

    #[test]
    fn a_key_the_panel_does_not_bind_is_left_to_the_host() {
        let (mut overlay, mut editor) = open_over("aaa");
        assert_eq!(
            overlay.handle_key(&chord(KeyCode::Char('s'), Modifiers::ctrl()), &mut editor),
            SearchOutcome::Ignored,
            "a save chord must stay live while the panel is open"
        );
        let meta = Modifiers {
            meta: true,
            ..Modifiers::none()
        };
        assert_eq!(
            overlay.handle_key(&chord(KeyCode::Char('s'), meta), &mut editor),
            SearchOutcome::Ignored
        );
    }

    #[test]
    fn a_caret_motion_in_the_query_costs_no_search_and_keeps_the_caret_visible() {
        let (mut overlay, mut editor) = open_over("ab ab");
        type_text(&mut overlay, &mut editor, "ab");
        overlay.handle_key(&press(KeyCode::Home), &mut editor);
        let content = overlay.content(&editor, &Theme::dark(), PanelFit::popover(60, 12));
        let caret = content.caret.expect("the focused field has a caret");
        assert_eq!(caret.row, 0);
        assert_eq!(
            caret.column,
            super::LABEL_WIDTH,
            "home is the field's start"
        );
    }

    #[test]
    fn a_narrow_panel_drops_the_toggles_before_the_count() {
        let (mut overlay, mut editor) = open_over("one two one");
        type_text(&mut overlay, &mut editor, "one");
        let content = overlay.content(
            &editor,
            &Theme::dark(),
            // Label 8 + field floor 8 + gap 1 leaves 6: the count "1 of 2"
            // fits, the toggles do not.
            PanelFit::popover(23, 12),
        );
        let row = content.rows[0].text();
        assert!(row.contains("1 of 2"), "{row:?}");
        assert!(!row.contains("Aa"), "{row:?}");
    }

    #[test]
    fn one_interior_row_shows_the_focused_field() {
        let (mut overlay, mut editor) = open_over("aaa");
        overlay.handle_key(&press(KeyCode::Tab), &mut editor);
        type_text(&mut overlay, &mut editor, "b");
        let content = overlay.content(&editor, &Theme::dark(), PanelFit::popover(40, 1));
        assert_eq!(content.rows.len(), 1);
        assert!(
            content.rows[0].text().starts_with("Replace"),
            "the field keystrokes go into must be the one shown"
        );
    }
}
