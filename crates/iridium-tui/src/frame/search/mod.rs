//! The search and replace overlay: a panel above the statusline.
//!
//! Step 5 of `docs/TERMINAL-FACE-PLAN.md`. The kernel's find/replace engine has
//! existed for months with no user interface in any face; this is the first one.
//!
//! # The engine is the kernel's and none of it is repeated here
//!
//! Nothing in this module searches, matches, compiles a pattern, walks to the
//! next hit, or edits the document. Every one of those is a verb
//! [`Editor`] already has, and a face with its own copy is exactly the mistake
//! that produced three bugs this year. What is here is a panel, two text
//! fields, a key map and a set of cell coordinates.
//!
//! The kernel calls made, and nothing else:
//!
//! | Doing | Kernel verb |
//! |---|---|
//! | Query or option changed | [`Editor::find`] |
//! | Next / previous match | [`Editor::goto_next_match`] / [`Editor::goto_previous_match`] |
//! | Replace one / all | [`Editor::replace_current_match`] / [`Editor::replace_all_matches`] |
//! | Escape | [`Editor::close_search`] |
//! | Feedback | [`Editor::search_match_count`], [`Editor::current_match_index`], [`Editor::search_state`] |
//!
//! ## Why `find` and not `update_search` or `set_search_options`
//!
//! Both of those exist and both re-run the search correctly, and neither moves
//! the caret. That is what rules them out for a *live* search: the point of
//! typing into the query field is that the document follows, and a panel that
//! reported `3 of 17` while the screen still showed some other part of the file
//! would be answering about a place the user cannot see. [`Editor::find`] is
//! the one verb that re-runs the search *and* moves to the match nearest the
//! caret, which is what makes the search incremental. Refining a query does not
//! drift, either: the caret is already sitting at the start of the match, so the
//! nearest match to it is the same one for as long as the longer query still
//! matches there.
//!
//! # An invalid pattern is a normal state
//!
//! A user typing `[a-z` has typed a regex that does not compile, and will type
//! the `]` next. [`Editor::find`] reports that as `Err`, and the panel keeps it:
//! the count segment says `Invalid regex` and the message row carries the
//! engine's own words. Nothing is swallowed, nothing panics, and every other key
//! keeps working. The kernel's state stays consistent through it — the failed
//! query leaves zero matches — so the highlighting agrees with what the panel
//! says.
//!
//! # The panel takes rows, it does not float over them
//!
//! Its rows are subtracted from the document's, so the kernel's viewport shrinks
//! with it. A panel floating over the last two rows would leave the kernel
//! believing those rows showed text, and
//! [`ensure_cursor_visible`](iridium_editor::render::Viewport::ensure_cursor_visible)
//! would then scroll a match to a row the panel is covering — the match found
//! and then hidden by the thing that found it.
//!
//! # Keys
//!
//! | Key | Action |
//! |---|---|
//! | any printable | insert into the focused field |
//! | `Left` `Right` `Home` `End` `Backspace` `Delete` | edit the focused field |
//! | `Tab` | move between the query and the replacement |
//! | `Enter`, `Down` | next match |
//! | `Shift+Enter`, `Up` | previous match |
//! | `Ctrl+R` | replace the current match |
//! | `Ctrl+Alt+R` | replace every match |
//! | `Alt+C` `Alt+W` `Alt+R` | case-sensitive, whole-word, regex |
//! | `Escape` | close |
//!
//! Every chord is representable under the legacy xterm encoding as well as the
//! kitty protocol — `Alt` is an escape prefix there, and `Ctrl` is a C0 byte —
//! except `Shift+Enter`, which the legacy encoding cannot express at all (see
//! [`crate::input`]). `Up` and `Down` are bound to the same two actions for
//! exactly that reason, so nothing is unreachable on a terminal without the
//! kitty protocol.
//!
//! Anything else is [`SearchOutcome::Ignored`] and stays the host's to handle,
//! so a binding such as save is not dead while the panel is open.

mod field;
mod matches;
mod paint;

#[cfg(test)]
mod tests;

use iridium_editor::search::SearchOptions;
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

use self::field::Field;
use super::CellPosition;
use super::palette::Palette;
use crate::cell::CellBuffer;

pub(super) use self::matches::{MatchHighlights, MatchedLine};

/// The number of rows the panel occupies when the screen has room for them.
const PANEL_ROWS: usize = 2;

/// What became of a key handed to the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOutcome {
    /// The overlay does not bind this key. It is still the host's to handle.
    Ignored,
    /// The overlay handled it and the document is unchanged.
    Handled,
    /// The overlay handled it and the document changed, replacing this many
    /// matches. The host's unsaved-changes bookkeeping needs this.
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
/// This is separate from the query's validity: a query can be invalid *and* a
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

/// The search and replace panel.
///
/// The host owns one of these and passes it to the frame in
/// [`Chrome`](super::Chrome) while it is open. Keeping it across closes is what
/// makes the query survive: closing clears the kernel's search but leaves the
/// text in the field, so re-opening offers the last query back.
#[derive(Debug, Clone, Default)]
pub struct SearchOverlay {
    /// The query field.
    query: Field,
    /// The replacement field.
    replacement: Field,
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

    /// The rows a panel takes on a screen with `available` rows for it.
    ///
    /// Two when there is room, and fewer when there is not: a panel that
    /// insisted on its full height would take the whole of a three-row terminal
    /// and leave no document to search. With one row it shows whichever field
    /// has focus, so keystrokes always go somewhere visible.
    #[must_use]
    pub const fn rows(available: usize) -> usize {
        if available < PANEL_ROWS {
            available
        } else {
            PANEL_ROWS
        }
    }

    /// Opens the panel, re-running whatever query it already holds.
    ///
    /// Re-running matters when a previous search was closed: the fields kept
    /// their text but [`Editor::close_search`] cleared the kernel's, so without
    /// this the panel would show a query that nothing was searching for.
    pub fn open(&mut self, editor: &mut Editor) {
        self.focus = Focus::Find;
        self.feedback = Feedback::None;
        self.refresh(editor);
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

    /// The options the search is running with.
    #[must_use]
    pub const fn options(&self) -> &SearchOptions {
        &self.options
    }

    /// The engine's complaint about the current query, if it has one.
    ///
    /// `Some` means the query does not compile — almost always a
    /// half-typed regex. It is a state, not a failure: the panel keeps working
    /// and the next keystroke may well fix it.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Whether the current query failed to compile.
    const fn has_error(&self) -> bool {
        self.error.is_some()
    }

    /// Handles one key press, driving the kernel for anything it binds.
    ///
    /// Returns [`SearchOutcome::Ignored`] for a key it does not bind, which the
    /// host is then free to act on itself.
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
            (Chord::Plain, KeyCode::Left) => self.edit(editor, Field::move_left, false),
            (Chord::Plain, KeyCode::Right) => self.edit(editor, Field::move_right, false),
            (Chord::Plain, KeyCode::Home) => self.edit(editor, Field::move_home, false),
            (Chord::Plain, KeyCode::End) => self.edit(editor, Field::move_end, false),
            (Chord::Plain, KeyCode::Backspace) => self.edit(editor, Field::backspace, true),
            (Chord::Plain, KeyCode::Delete) => self.edit(editor, Field::delete, true),
            (Chord::Plain, KeyCode::Char(character)) => {
                self.edit(editor, |field| field.insert(character), true)
            },
            (Chord::Alt, KeyCode::Char(character)) => self.toggle(editor, character),
            (Chord::Ctrl, KeyCode::Char('r' | 'R')) => self.replace_current(editor),
            (Chord::CtrlAlt, KeyCode::Char('r' | 'R')) => self.replace_all(editor),
            _ => SearchOutcome::Ignored,
        }
    }

    /// Applies an edit to the focused field, re-searching when it changed the
    /// query.
    ///
    /// `rewrites` says whether the edit can change the field's text. A motion
    /// cannot, so it never re-runs the search; a caret moving in the query field
    /// must not cost a pass over the document.
    fn edit(
        &mut self,
        editor: &mut Editor,
        edit: impl FnOnce(&mut Field) -> bool,
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
    /// changes anything, because every match it knew about is gone. The panel is
    /// still open with a query in it, so the search is re-run afterwards: a
    /// visible query field that nothing is searching for is a lie, and the
    /// highlighting would silently disappear.
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
    /// The error is *stored*, not returned and not logged away: it is the only
    /// thing that tells the user why a query they are still typing finds
    /// nothing.
    fn refresh(&mut self, editor: &mut Editor) {
        self.error = editor.find(self.query.text(), &self.options).err();
    }

    /// Paints the panel into the rows it owns, returning where the caret goes.
    pub(super) fn paint(
        &self,
        buffer: &mut CellBuffer,
        first_row: usize,
        row_count: usize,
        editor: &Editor,
        palette: &Palette,
    ) -> Option<CellPosition> {
        paint::paint(
            buffer,
            first_row,
            row_count,
            paint::Panel {
                overlay: self,
                editor,
                palette,
            },
        )
    }
}

/// The modifier combinations the overlay distinguishes.
///
/// Shift is deliberately not part of it: it decides which *character* a
/// printable key produced, which the input adapter has already resolved, and it
/// selects between next and previous on `Enter`.
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
