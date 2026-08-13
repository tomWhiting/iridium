//! The menu itself: its rows, its state, and what a resolved verb does to it.
//!
//! Split out of `mod.rs` when the module became a directory for #117 —
//! `mod.rs` carries declarations, not the type. The keys that reach [`run`]
//! are [`super::keymap`]'s table and whatever a user's `[keys]` layer put over
//! it; see [`super::resolve`] for the seam between the two.
//!
//! [`run`]: ContextMenu::run

use iridium_editor::commands::builtin::{
    CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE, PALETTE_OPEN, SELECTION_SELECT_ALL,
};
use iridium_editor::theme::Theme;
use iridium_editor::{CommandId, Editor, KeyEvent, KeyPress, Keymap, KeymapStack};

use crate::line::LineBuilder;
use crate::overlay::{PanelAnchor, PanelContent, PanelFit, PanelRow};
use crate::verbs::{Availability, ResolvedVerb, Row, resolve as resolve_verbs};

use super::resolve::{self, Resolved};
use super::verb::Verb;

/// The blank characters between a row's label and its key hint.
const HINT_GAP: usize = 2;

/// What became of a key handed to the menu.
///
/// There is no `Ignored`: the menu is modal, and a key it does not bind is
/// swallowed rather than falling through to the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuOutcome {
    /// The menu consumed the key and stays open.
    Handled,
    /// The menu was closed without running anything.
    Closed,
    /// The menu was closed and this command should now run.
    Run(CommandId),
}

/// One row of the menu: a verb, or the rule between two groups of them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuItem {
    /// The command the row runs, or `None` for a separator.
    command: Option<CommandId>,
    /// The label, left-aligned.
    label: &'static str,
    /// The chord that runs the same verb, right-aligned, when one is bound.
    hint: Option<String>,
    /// Whether the row can run: `false` greys it and skips it in navigation.
    enabled: bool,
}

impl MenuItem {
    /// A separator: a hairline rule, never selectable.
    const fn separator() -> Self {
        Self {
            command: None,
            label: "",
            hint: None,
            enabled: false,
        }
    }

    /// Whether the row can be highlighted and run.
    const fn is_selectable(&self) -> bool {
        self.enabled && self.command.is_some()
    }

    /// The characters the row wants: label, gap and hint.
    fn width(&self) -> usize {
        let label = self.label.chars().count();
        self.hint
            .as_ref()
            .map_or(label, |hint| label + HINT_GAP + hint.chars().count())
    }
}

/// The ruled verb set (D-2), in order.
///
/// The ids are the kernel's own constants, so a renamed command is a compile
/// error here rather than a dead row.
pub(super) fn verbs() -> Vec<Row> {
    vec![
        Row::verb(CLIPBOARD_CUT, "Cut"),
        Row::verb(CLIPBOARD_COPY, "Copy"),
        Row::verb(CLIPBOARD_PASTE, "Paste"),
        Row::Rule,
        Row::verb(SELECTION_SELECT_ALL, "Select All"),
        Row::Rule,
        Row::verb(PALETTE_OPEN, "Command Palette…"),
    ]
}

/// The context menu panel.
///
/// One exists only while the menu is up: the host holds `Option<ContextMenu>`,
/// so a closed menu is not a menu with a flag beside it but no menu at all,
/// and nothing stale can be painted or hit-tested.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    /// The rows, built from the registry when the menu opened.
    items: Vec<MenuItem>,
    /// The highlighted row, as an index into `items`.
    selected: usize,
    /// The click the menu is anchored at, in physical pixels.
    anchor: (f32, f32),
    /// The menu's own bindings, with the user's `[keys]` layer over them.
    pub(super) keys: KeymapStack,
    /// The strokes of a multi-stroke sequence typed so far.
    pub(super) pending: Vec<KeyPress>,
}

impl ContextMenu {
    /// Builds the menu for this editor state, anchored at a click in physical
    /// pixels, answering the user's bindings as well as its own.
    ///
    /// The rows are resolved against the registry *here*, once, rather than
    /// per frame: what a menu offers must not change under the hand that is
    /// aiming at it, and nothing that decides a row — the registry, the key
    /// hints, the read-only bit — can change while the menu holds key focus.
    ///
    /// ⭐ `user_keys` is a **parameter rather than a later setter** for the
    /// reason `FileExplorerPanel::open` takes one: a menu built without the
    /// user's layer answers the wrong keys, and a constructor that cannot be
    /// called without it makes forgetting a compile error rather than a bug
    /// report. See [`resolve`] for what is kept from that layer, and why the
    /// filtering matters.
    #[must_use]
    pub fn open(editor: &Editor, x: f32, y: f32, user_keys: &Keymap) -> Self {
        let items = build(editor);
        let selected = items.iter().position(MenuItem::is_selectable).unwrap_or(0);
        Self {
            items,
            selected,
            anchor: (x, y),
            keys: resolve::stack_for(user_keys),
            pending: Vec::new(),
        }
    }

    /// The click the menu was opened at, in physical pixels.
    #[must_use]
    pub const fn anchor(&self) -> (f32, f32) {
        self.anchor
    }

    /// How many rows the menu composes — the count a pointer hit test is
    /// resolved against.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.items.len()
    }

    /// Handles one key press. Every key is consumed; see the module docs.
    ///
    /// The key is resolved against the menu's stack rather than matched on
    /// directly, so a `[keys]` line reaches it — and a half-typed sequence, an
    /// unbind and a key nobody claimed all come to the same modal nothing.
    pub fn handle_key(&mut self, event: &KeyEvent) -> MenuOutcome {
        match self.resolve_key(event) {
            Resolved::Verb(verb) => self.run(verb),
            // Modal: everything else is swallowed, not passed to the document.
            Resolved::Pending | Resolved::Unclaimed => MenuOutcome::Handled,
        }
    }

    /// Runs one resolved verb.
    ///
    /// Exhaustive over [`Verb`], so a command added to the kernel's table and
    /// given a variant is a compile error here until it is answered.
    fn run(&mut self, verb: Verb) -> MenuOutcome {
        match verb {
            Verb::Dismiss => MenuOutcome::Closed,
            Verb::Accept => self.accept(),
            Verb::SelectPrevious => self.step(false),
            Verb::SelectNext => self.step(true),
            Verb::SelectFirst => self.jump(self.first_selectable()),
            Verb::SelectLast => self.jump(self.last_selectable()),
        }
    }

    /// Highlights the row the pointer is over, reporting whether it moved.
    ///
    /// A row that cannot run — a separator, or a greyed verb — does not take
    /// the highlight: the highlight is what `Enter` would run.
    pub fn hover(&mut self, row: usize) -> bool {
        if self.selected == row || !self.can_run(row) {
            return false;
        }
        self.selected = row;
        true
    }

    /// Runs the row a click landed on, if that row can run.
    pub fn activate(&mut self, row: usize) -> Option<CommandId> {
        if !self.can_run(row) {
            return None;
        }
        self.selected = row;
        self.items.get(row).and_then(|item| item.command.clone())
    }

    /// Composes the panel for painting: one row per verb, anchored at the
    /// click.
    ///
    /// The panel is exactly as wide as its widest row needs, capped by what
    /// the window can hold — a menu that stretched to the palette's width
    /// would be a list pretending to be a search.
    #[must_use]
    pub fn content(&self, theme: &Theme, fit: PanelFit) -> PanelContent {
        let columns = self.columns_for(fit);
        let rows = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| self.row(index, item, theme, columns))
            .collect();
        PanelContent {
            anchor: PanelAnchor::Point {
                x: self.anchor.0,
                y: self.anchor.1,
            },
            content_columns: columns,
            rows,
            caret: None,
            hovered: None,
        }
    }

    /// Composes one row: the label, then the key hint on the right edge when
    /// the panel is wide enough to hold both.
    fn row(&self, index: usize, item: &MenuItem, theme: &Theme, columns: usize) -> PanelRow {
        if item.command.is_none() {
            return PanelRow::separator();
        }
        let quiet = theme.editor.line_number;
        let voice = if item.enabled {
            theme.editor.foreground
        } else {
            // The palette's dim-text convention: a greyed row is still
            // readable, and reads as unavailable rather than as missing.
            quiet
        };

        let mut line = LineBuilder::new(columns);
        line.push(item.label, voice);
        if let Some(hint) = &item.hint {
            let hint_chars = hint.chars().count();
            if columns >= item.width() {
                line.pad_to(columns - hint_chars, quiet);
                line.push(hint, quiet);
            }
        }
        let spans = line.finish();
        if index == self.selected {
            PanelRow::selected(spans)
        } else {
            PanelRow::new(spans)
        }
    }

    /// The characters the rows are laid out against: the widest row's want,
    /// clamped to what the window offers.
    fn columns_for(&self, fit: PanelFit) -> usize {
        let widest = self.items.iter().map(MenuItem::width).max().unwrap_or(0);
        widest.max(1).min(fit.content_columns)
    }

    /// Resolves the highlighted row to the command `Enter` would run.
    fn accept(&self) -> MenuOutcome {
        match self.items.get(self.selected) {
            Some(item) if item.is_selectable() => item
                .command
                .clone()
                .map_or(MenuOutcome::Handled, MenuOutcome::Run),
            _ => MenuOutcome::Handled,
        }
    }

    /// Moves the highlight one row, skipping what cannot run and **clamping**
    /// at both ends.
    ///
    /// Clamping rather than wrapping is the palette's convention, for the
    /// palette's reason: wrapping overshoots on key repeat, and a list whose
    /// ends are walls can be leaned on.
    fn step(&mut self, forward: bool) -> MenuOutcome {
        let mut index = self.selected;
        loop {
            let next = if forward {
                index.checked_add(1).filter(|next| *next < self.items.len())
            } else {
                index.checked_sub(1)
            };
            let Some(next) = next else {
                // The end of the list with nothing runnable between here and
                // it: the highlight stays where it is.
                return MenuOutcome::Handled;
            };
            index = next;
            if self.can_run(index) {
                self.selected = index;
                return MenuOutcome::Handled;
            }
        }
    }

    /// Moves the highlight to a resolved row, if there is one.
    const fn jump(&mut self, row: Option<usize>) -> MenuOutcome {
        if let Some(row) = row {
            self.selected = row;
        }
        MenuOutcome::Handled
    }

    /// Whether row `index` exists and can run.
    fn can_run(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(MenuItem::is_selectable)
    }

    /// The first row that can run.
    fn first_selectable(&self) -> Option<usize> {
        self.items.iter().position(MenuItem::is_selectable)
    }

    /// The last row that can run.
    fn last_selectable(&self) -> Option<usize> {
        self.items.iter().rposition(MenuItem::is_selectable)
    }
}

/// Resolves the ruled verb set against a live registry.
///
/// The resolution itself is [`crate::verbs::resolve`], shared with the menu
/// bar — dropping a verb the registry does not carry, reading the label's
/// chord off the keymap, and trimming the rules left behind — because both
/// menus must answer those questions the same way. What is decided *here* is
/// only the shape a resolved row takes on screen.
///
/// `inert` is false: the context menu is not opened while anything modal is
/// up ([`secondary_pressed`](crate::app::DesktopApp) refuses), so a row greyed
/// for that reason could never be seen.
fn build(editor: &Editor) -> Vec<MenuItem> {
    let availability = Availability {
        // The one honest disable this face can state: a verb that would
        // change text the document refuses to change.
        read_only: editor.state().read_only,
        inert: false,
    };
    resolve_verbs(editor, &verbs(), availability)
        .into_iter()
        .map(|row| row.map_or_else(MenuItem::separator, MenuItem::from))
        .collect()
}

impl From<ResolvedVerb> for MenuItem {
    fn from(row: ResolvedVerb) -> Self {
        Self {
            command: Some(row.command),
            label: row.label,
            hint: row.hint,
            enabled: row.enabled,
        }
    }
}
