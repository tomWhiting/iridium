//! Composing the explorer into rows the overlay can paint.
//!
//! Split from `panel` by the question each method answers: nothing here reads
//! a key or changes what is open. A defect in it presents as "the wrong thing
//! is on screen" — a row missing, a window that will not follow the
//! selection, a caret in the wrong column.
//!
//! # Four screens, and only one of them has the query field on it
//!
//! Browsing draws the field and the rows under it. Editing draws the buffer
//! instead, with a caret in the row being typed into, and a label where the
//! field was — the field is gone because the query is the *source* of those
//! rows, and typing into it while they are being edited would rebuild the list
//! the buffer is a snapshot of.
//!
//! **That one row therefore carries both halves of the panel**, and it is why
//! [`BROWSE_HINT`] is drawn where it is: the line that says what the keys mean
//! while editing is the line that says the editing screen is there at all. The
//! feature was otherwise reachable only by already knowing about it — there is
//! no menu to find `Tab` in and nothing else on the browse screen said a word.
//!
//! A refusal and a confirmation replace the body outright, field and all.
//! ⚠️ That is a safety property, not a layout preference: `y` on the
//! confirmation applies the plan, and `y` is a plain character. A query field
//! left on screen there would be an invitation to type one.

use iridium_editor::theme::Theme;

use super::buffer::Buffer;
use super::confirm::{plan_rows, refusal_rows};
use super::panel::{FileExplorer, PROMPT};
use super::rows::{RowShape, depths, edited_name_column, edited_row, entry_row};
use crate::line::{LineBuilder, skip_chars};
use crate::overlay::scroll_for;
use crate::overlay::{PanelAnchor, PanelCaret, PanelContent, PanelFit, PanelRow, Span};

/// What the label at the top of an editing session says after its verb.
const EDIT_HINT: &str = "⌘S to apply    esc to stop";

/// What the query row says, on its right edge, about the screen next door.
///
/// "these rows", not "this file": the key edits the *listing* — rename, move,
/// create, delete — and a hint in a file panel reading "tab to edit" would be
/// read as opening the selected file, which is what `Enter` already does.
///
/// `pub(super)` so the tests assert against this string rather than a copy of
/// it. A hint the tests spell out themselves is a hint that can be reworded
/// here and still pass.
pub(super) const BROWSE_HINT: &str = "tab to edit these rows";

/// How the query row spells the key that reveals dot-prefixed entries.
///
/// The `⌘` glyph unconditionally, as [`EDIT_HINT`] already does: this face is
/// built and shipped for macOS, and `Ctrl+.` is bound beside it for a keyboard
/// without a command key rather than as a second platform to write hints for.
///
/// `pub(super)` for the same reason as [`BROWSE_HINT`] — a test that spells
/// the string out itself proves only that two copies were typed the same way.
pub(super) const HIDDEN_KEY: &str = "⌘.";

/// The blank columns kept between the query caret and [`BROWSE_HINT`].
///
/// Two, which is the gap the editing label puts after its verb. One would
/// leave the caret touching the hint on a panel exactly wide enough for both.
const HINT_GAP: usize = 2;

impl FileExplorer {
    /// Composes the panel for painting.
    ///
    /// `&mut self` because composition is where the scroll window follows the
    /// selection and the page size is learned, exactly as in the palette.
    ///
    /// Which screen is asked of the state that can only be true on one of
    /// them, in the same order [`super::edit_keys`] asks it — the keys and the
    /// drawing agreeing about which screen is up is not something to leave to
    /// two independently written conditions.
    pub fn content(&mut self, theme: &Theme, fit: PanelFit) -> PanelContent {
        if self.mode.plan().is_some() {
            return self.confirmation_content(theme, fit);
        }
        if !self.mode.refusals().is_empty() {
            return self.refusal_content(theme, fit);
        }
        if self.mode.cursor().is_some() {
            return self.edit_content(theme, fit);
        }
        self.browse_content(theme, fit)
    }

    /// The confirmation: what the buffer would do, in the order it would do
    /// it, and nothing else on screen.
    fn confirmation_content(&self, theme: &Theme, fit: PanelFit) -> PanelContent {
        let root = self.root_path();
        let rows = self.mode.plan().map_or_else(Vec::new, |plan| {
            plan_rows(
                plan,
                &root,
                theme,
                fit.content_columns,
                fit.max_interior_rows,
            )
        });
        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: fit.content_columns,
            rows,
            caret: None,
            hovered: None,
        }
    }

    /// Every reason the buffer cannot be applied, and nothing else on screen.
    fn refusal_content(&self, theme: &Theme, fit: PanelFit) -> PanelContent {
        let rows = self.mode.buffer().map_or_else(Vec::new, |buffer| {
            refusal_rows(
                buffer.rows(),
                self.mode.refusals(),
                theme,
                fit.content_columns,
                fit.max_interior_rows,
            )
        });
        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: fit.content_columns,
            rows,
            caret: None,
            hovered: None,
        }
    }

    /// The editable rows, with the caret in the one being typed into.
    ///
    /// The window arithmetic is the browse screen's, unchanged: [`row_count`]
    /// and [`selected_row`] answer for the buffer while a session is open, so
    /// [`follow_selection`] follows the cursor through it without knowing
    /// there are two lists.
    ///
    /// [`row_count`]: Self::row_count
    /// [`selected_row`]: Self::selected_row
    /// [`follow_selection`]: Self::follow_selection
    fn edit_content(&mut self, theme: &Theme, fit: PanelFit) -> PanelContent {
        let total = self.row_count();
        let visible = total.clamp(1, fit.max_browse_rows);
        self.follow_selection(total, visible);

        let cursor = self.mode.cursor();
        let buffer = self.mode.buffer().map_or(&[] as &[_], Buffer::rows);
        let depths = depths(buffer);

        let mut rows = Vec::with_capacity(visible + 1);
        rows.push(Self::editing_label(theme, fit.content_columns));
        for offset in 0..visible {
            let index = self.scroll + offset;
            match (buffer.get(index), depths.get(index)) {
                (Some(row), Some(&depth)) => rows.push(edited_row(
                    row,
                    depth,
                    cursor == Some(index),
                    theme,
                    fit.content_columns,
                )),
                // A buffer with nothing in it cannot be reached — a session
                // refuses to start without rows — but a window one row taller
                // than the list is exactly what `visible.clamp(1, …)` produces
                // for it, and a panel that panicked here would take the window
                // with it.
                _ => rows.push(PanelRow::new(vec![Span::new(
                    "No rows",
                    theme.editor.line_number,
                )])),
            }
        }

        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: fit.content_columns,
            rows,
            caret: self.edit_caret(cursor, &depths, visible, fit.content_columns),
            hovered: None,
        }
    }

    /// Where the caret sits in the editable rows.
    ///
    /// `row` is offset by one for the label, which is the same offset the
    /// query row imposes when browsing — the caret is placed against the
    /// composed rows, not against the list.
    ///
    /// A name wider than the panel is truncated with an ellipsis by
    /// [`edited_row`], and a caret past the right edge is parked on the last
    /// column rather than drawn outside the panel. The row is visibly cut, so
    /// the caret is not the only thing saying there is more.
    fn edit_caret(
        &self,
        cursor: Option<usize>,
        depths: &[usize],
        visible: usize,
        width: usize,
    ) -> Option<PanelCaret> {
        let index = cursor?;
        let name = self.mode.name()?;
        let &depth = depths.get(index)?;
        let row = index.checked_sub(self.scroll)?;
        if row >= visible {
            return None;
        }
        let column = edited_name_column(depth).saturating_add(name.caret_column());
        Some(PanelCaret {
            row: row + 1,
            column: column.min(width.saturating_sub(1)),
        })
    }

    /// The label that stands where the query field does while browsing.
    ///
    /// Unmistakable on purpose: the same physical keys now mean different
    /// things, and a screen that looked like the browse screen would be a
    /// screen where `d` filters and `Ctrl+D` marks a directory for removal
    /// with nothing to tell the two apart.
    fn editing_label(theme: &Theme, width: usize) -> PanelRow {
        let mut line = LineBuilder::new(width);
        line.push("edit", theme.editor.change_modified);
        line.push("  ", theme.editor.line_number);
        line.push(EDIT_HINT, theme.editor.line_number);
        PanelRow::new(line.finish())
    }

    /// The query row, then the visible slice of whichever list is showing.
    fn browse_content(&mut self, theme: &Theme, fit: PanelFit) -> PanelContent {
        let total = self.row_count();
        // At least one row, so a tree whose root has not listed yet still says
        // "Reading…" rather than composing an empty panel; and never more than
        // the placement affords. ⭐ `max_browse_rows` has already given up the
        // query row and applied whichever ceiling the placement carries — this
        // screen deliberately does not know whether it is a popover or a
        // sidebar, which is what keeps the two from drifting apart.
        let visible = total.clamp(1, fit.max_browse_rows);
        self.follow_selection(total, visible);

        let mut rows = Vec::with_capacity(visible + 1);
        let (input, caret_column) = self.input_row(theme, fit.content_columns);
        rows.push(input);
        for offset in 0..visible {
            rows.push(self.list_row(self.scroll + offset, theme, fit.content_columns));
        }

        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: fit.content_columns,
            rows,
            caret: caret_column.map(|column| PanelCaret { row: 0, column }),
            hovered: None,
        }
    }

    /// How many rows the list under the label has.
    ///
    /// The buffer answers first while a session is open, and it has to: a
    /// typed row makes the buffer longer than either source list immediately,
    /// and a window sized from the source would stop drawing at the row before
    /// the one just created.
    pub(super) fn row_count(&self) -> usize {
        if let Some(buffer) = self.mode.buffer() {
            return buffer.rows().len();
        }
        if self.is_filtering() {
            self.view.rows.len()
        } else {
            self.tree.len()
        }
    }

    /// The selected row's index within the list under the label.
    ///
    /// ⚠️ **Three sources, and they are not interchangeable.** The tree's
    /// selection and the filtered view's index the *source* lists; the cursor
    /// indexes the buffer, which diverges from both the moment a row is typed.
    /// Also read by [`super::keys`] to decide which row an edit session opens
    /// on.
    pub(super) const fn selected_row(&self) -> Option<usize> {
        if let Some(cursor) = self.mode.cursor() {
            return Some(cursor);
        }
        if self.is_filtering() {
            Some(self.filtered)
        } else {
            self.tree.selected()
        }
    }

    /// Composes the query row and reports where its caret landed.
    ///
    /// # [`BROWSE_HINT`] shares this row, and only while the field is empty
    ///
    /// There is no second line to put it on and no column to spare once
    /// something is being typed: the field owns everything right of the prompt
    /// and scrolls through it, so a reserved zone on the right edge would be a
    /// zone the text runs under. The hint is therefore drawn against an
    /// *empty* field and goes the moment a character arrives — which is the
    /// moment it has finished its job, because whoever typed that character
    /// has already read it.
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
        let caret_column = prompt_chars + caret - scroll;
        if self.query.text().is_empty() {
            self.push_hints(&mut line, theme, width, caret_column);
        }
        (PanelRow::new(line.finish()), Some(caret_column))
    }

    /// Right-aligns whatever the query row has to say, or nothing at all.
    ///
    /// Narrow wins over informative: a hint that had to be truncated, or that
    /// sat against the caret, reads as damage rather than as help. A panel
    /// that cannot hold one keeps the field, which is the half that does
    /// something.
    ///
    /// ⭐ **When only one of the two fits, the hidden count is the one that
    /// stays.** [`BROWSE_HINT`] advertises a feature, and the worst case of
    /// losing it is that `Tab` stays undiscovered for another session. The
    /// count is the sentence that keeps hiding entries from being a lie about
    /// the disk — `iridium-explorer`'s own rule — and dropping it for want of
    /// two columns would leave the panel quietly showing less than `ls` with
    /// nothing on screen admitting it.
    fn push_hints(&self, line: &mut LineBuilder, theme: &Theme, width: usize, caret_column: usize) {
        let hidden = self.hidden_on_screen();
        let hidden_hint = (hidden > 0).then(|| format!("{hidden} hidden ({HIDDEN_KEY})"));
        let tab_hint = self.tab_would_edit().then_some(BROWSE_HINT);
        let both = match (hidden_hint.as_deref(), tab_hint) {
            (Some(count), Some(tab)) => Some(format!("{count}{:gap$}{tab}", "", gap = HINT_GAP)),
            _ => None,
        };

        // Widest first, and the count before the tab hint: the first that fits
        // is drawn and the rest are not tried.
        let Some(text) = [both.as_deref(), hidden_hint.as_deref(), tab_hint]
            .into_iter()
            .flatten()
            .find(|text| Self::hint_fits(text, width, caret_column))
        else {
            return;
        };
        let start = width.saturating_sub(text.chars().count());
        line.pad_to(start, theme.editor.line_number);
        line.push(text, theme.editor.line_number);
    }

    /// Whether `text` can be right-aligned without touching the caret.
    fn hint_fits(text: &str, width: usize, caret_column: usize) -> bool {
        width
            .checked_sub(text.chars().count())
            .is_some_and(|start| start >= caret_column + HINT_GAP)
    }

    /// Whether `Tab` would open an editing session on the rows showing now.
    ///
    /// ⚠️ **The two conditions [`super::keys`]'s `begin_editing` refuses on**,
    /// asked of the rows the panel is drawing rather than of the snapshot it
    /// would build from them: a listing still on its way, and no rows to
    /// snapshot at all. A hint offered over a panel that says "Reading…" is a
    /// hint offered over a refusal, and a key that answers "not yet" the first
    /// time it is pressed is a key nobody presses twice.
    ///
    /// Not `source_rows` itself, which allocates a vector of every drawn row
    /// and would do it once per frame on a hint's behalf. The two answers
    /// differ only for a row the arena has no record of, which
    /// [`super::buffer`] documents as unreachable — and where the cost of
    /// being wrong is a hint over a refusal that says exactly why.
    fn tab_would_edit(&self) -> bool {
        self.wanted.is_none() && self.row_count() > 0
    }

    /// What an empty result list says.
    ///
    /// **Five different statements, because they mean five different
    /// things.** A pattern that does not compile has not searched at all — it
    /// is a query halfway through being typed, and the useful thing to show
    /// is why it was rejected. A search still reading directories has not
    /// failed to find anything either; it has not looked everywhere yet, and
    /// saying "no matching files" while the crawl is running is a lie that
    /// resolves itself a second later, which is exactly long enough for
    /// someone to have given up. A search that stopped at its limit has
    /// genuinely not looked everywhere and never will, and that is worth
    /// saying out loud rather than passing off as an answer. A panel that
    /// will not crawl at all has searched exactly what is open, and the
    /// honest answer names that scope rather than implying the file is not on
    /// the disk.
    ///
    /// **The order is load-bearing, and the two cases at the top are why.**
    /// Neither a bad pattern nor a crawl-off panel ever drains the frontier,
    /// so `is_fully_crawled` stays `false` under both of them for good.
    /// Checked in any other order, "Searching…" is the only thing this
    /// function would ever say in either case.
    fn nothing_found_yet(&self) -> String {
        if let Some(reason) = self.pattern.invalid_reason() {
            return format!("Bad pattern — {reason}");
        }
        if !self.crawl {
            return "No matches in what is open".to_owned();
        }
        if !self.files.is_fully_crawled() {
            return "Searching…".to_owned();
        }
        if self.files.crawl_hit_its_limit() {
            return "No matches — the search stopped at its limit".to_owned();
        }
        "No matching files".to_owned()
    }

    /// One row of whichever list is showing.
    fn list_row(&self, index: usize, theme: &Theme, width: usize) -> PanelRow {
        if self.is_filtering() {
            let Some(row) = self.view.rows.get(index) else {
                return PanelRow::new(vec![Span::new(
                    self.nothing_found_yet(),
                    theme.editor.line_number,
                )]);
            };
            let shape = RowShape {
                depth: row.depth,
                // Every folder in a filtered view is showing what is inside
                // it, so the disclosure points down — and no folder is drawn
                // at all unless something under it matched.
                has_children: self.files.info(row.id).is_some_and(|info| {
                    info.kind.is_expandable()
                        && self.files.visible_children(row.id).next().is_some()
                }),
                expanded: true,
                selected: index == self.filtered && row.is_selectable(),
                positions: row.positions.clone(),
            };
            return entry_row(&self.files, row.id, &shape, theme, width);
        }

        let Some(row) = self.tree.row(index) else {
            // Only reachable for a tree with no rows at all, which is a root
            // whose read has not landed yet.
            return PanelRow::new(vec![Span::new("Reading…", theme.editor.line_number)]);
        };
        let shape = RowShape {
            depth: row.depth,
            has_children: row.has_children,
            expanded: row.expanded,
            selected: self.tree.selected() == Some(index),
            positions: Vec::new(),
        };
        entry_row(&self.files, row.id, &shape, theme, width)
    }

    /// Slides the window so the selection is inside it, and never past the
    /// end of the rows.
    ///
    /// ⭐ **The window follows the selection when the selection *moves*, and
    /// not otherwise — and for a browsed panel that distinction is the whole
    /// difference between a list you can look through and one you cannot.**
    /// The wheel moves the window and deliberately leaves the selection alone,
    /// so a rule that re-centred on the selection every composition would undo
    /// the scroll on the very next frame; the list would spring back under the
    /// pointer, which is the same class of defect as the scroll bounce — a
    /// consumer feeding its own answer back in.
    ///
    /// Following unconditionally was correct for as long as the keyboard was
    /// the only thing that could move either of them.
    ///
    /// The clamp is applied on the way out as well as the way in: a refresh
    /// that removed rows can leave a scroll pointing past the end, and a
    /// window starting past the last row draws nothing at all. It is also what
    /// bounds an over-scrolled wheel, which is why [`scroll_rows`] does not
    /// have to know how tall the window is.
    ///
    /// [`scroll_rows`]: Self::scroll_rows
    fn follow_selection(&mut self, total: usize, visible: usize) {
        let last_start = total.saturating_sub(visible);
        let selected = self.selected_row();
        if selected != self.followed {
            self.followed = selected;
            if let Some(selected) = selected {
                if selected < self.scroll {
                    self.scroll = selected;
                } else if visible > 0 && selected >= self.scroll + visible {
                    self.scroll = selected + 1 - visible;
                }
            }
        }
        self.scroll = self.scroll.min(last_start);
    }
}
