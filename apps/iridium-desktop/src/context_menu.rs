//! The right-click context menu: the verbs the pointer can reach.
//!
//! Drawn, not native. `docs/design/CONTEXT-MENU-MAP.md` priced both ways and
//! the owner ruled D-1 for the drawn overlay: it rides the same
//! `RoundedQuadRenderer` chrome as the palette ([`crate::overlay`]), moves the
//! lockfile not at all, stays inside the winit event model the face's latency
//! identity depends on, and is a pattern every future GPU face inherits. The
//! stated price is that macOS's own Services, Look Up and dictation rows are
//! not available.
//!
//! # The panel is modal, and is a poorer palette
//!
//! Everything here is the palette's shape with the search engine removed: a
//! fixed list of rows, a selection that **clamps** at both ends rather than
//! wrapping, `Enter` running the highlighted row, `Escape` closing, and every
//! other key swallowed — a chord falling through to the document while the
//! user is aiming at a verb list would edit text they are not looking at.
//!
//! | Key | Action |
//! |---|---|
//! | `Down`, `Ctrl+N` / `Up`, `Ctrl+P` | move the highlight, clamping, over separators and disabled rows |
//! | `Home` / `End` | the first / last row that can run |
//! | `Enter` | run the highlighted verb |
//! | `Escape` | close |
//!
//! # The verbs
//!
//! D-2 ruled the buildable floor: Cut, Copy, Paste, Select All and a
//! Command Palette… row. Every one of them is a command **already in the
//! kernel's registry** — this module registers nothing and implements no verb;
//! it resolves a [`CommandId`] and the host dispatches it down the same
//! kernel-first path a palette entry takes. A verb the registry does not carry
//! is dropped from the menu rather than offered and refused, and the test
//! module proves each id against a real registry.
//!
//! Labels are the menu's own, in the platform's menu vocabulary; for the four
//! editing verbs they agree with the registry's titles (a test pins that), and
//! the palette row is named for the panel it opens rather than for the
//! command's palette title.
//!
//! # What is greyed, and what deliberately is not
//!
//! A row is disabled when the command it runs mutates the document
//! ([`CommandMeta::mutates_document`](iridium_editor::commands::CommandMeta::mutates_document))
//! and the document is read-only — the one honest disable this face can state.
//! Cut and Copy stay **enabled** with a collapsed selection (D-5): the kernel's
//! clipboard verbs copy the caret lines when nothing is selected, so greying
//! them the way macOS does would misstate what they do.

use iridium_editor::commands::builtin::{
    CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE, PALETTE_OPEN, SELECTION_SELECT_ALL,
};
use iridium_editor::theme::Theme;
use iridium_editor::{CommandId, Editor, KeyCode, KeyEvent, KeyLabelStyle};

use crate::line::LineBuilder;
use crate::overlay::{PanelAnchor, PanelContent, PanelFit, PanelRow};

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

/// One entry of the ruled verb set, before the registry is consulted.
struct Verb {
    /// The registered command the row runs.
    id: CommandId,
    /// The menu's label for it.
    label: &'static str,
}

/// The ruled verb set (D-2), in order; `None` is a separator.
///
/// The ids are the kernel's own constants, so a renamed command is a compile
/// error here rather than a dead row.
fn verbs() -> Vec<Option<Verb>> {
    vec![
        Some(Verb {
            id: CLIPBOARD_CUT,
            label: "Cut",
        }),
        Some(Verb {
            id: CLIPBOARD_COPY,
            label: "Copy",
        }),
        Some(Verb {
            id: CLIPBOARD_PASTE,
            label: "Paste",
        }),
        None,
        Some(Verb {
            id: SELECTION_SELECT_ALL,
            label: "Select All",
        }),
        None,
        Some(Verb {
            id: PALETTE_OPEN,
            label: "Command Palette…",
        }),
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
}

impl ContextMenu {
    /// Builds the menu for this editor state, anchored at a click in physical
    /// pixels.
    ///
    /// The rows are resolved against the registry *here*, once, rather than
    /// per frame: what a menu offers must not change under the hand that is
    /// aiming at it, and nothing that decides a row — the registry, the key
    /// hints, the read-only bit — can change while the menu holds key focus.
    #[must_use]
    pub fn open(editor: &Editor, x: f32, y: f32) -> Self {
        let items = build(editor);
        let selected = items.iter().position(MenuItem::is_selectable).unwrap_or(0);
        Self {
            items,
            selected,
            anchor: (x, y),
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
    pub fn handle_key(&mut self, event: &KeyEvent) -> MenuOutcome {
        // Shift is not distinguished: it decides which character a printable
        // key produced, and none of the menu's keys are printable.
        let modifiers = event.modifiers;
        let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.meta;
        let ctrl_only = modifiers.ctrl && !modifiers.alt && !modifiers.meta;
        match event.key {
            KeyCode::Escape if plain => MenuOutcome::Closed,
            KeyCode::Enter if plain => self.accept(),
            KeyCode::Up if plain => self.step(false),
            KeyCode::Down if plain => self.step(true),
            KeyCode::Char('p' | 'P') if ctrl_only => self.step(false),
            KeyCode::Char('n' | 'N') if ctrl_only => self.step(true),
            KeyCode::Home if plain => self.jump(self.first_selectable()),
            KeyCode::End if plain => self.jump(self.last_selectable()),
            // Modal: everything else is swallowed, not passed to the document.
            _ => MenuOutcome::Handled,
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
/// A verb the registry does not carry is **dropped**, not greyed: a menu row
/// that could never run is a lie, and the registry is the only authority on
/// what exists. Separators are dropped with it where that would leave a rule
/// leading, trailing or doubled.
fn build(editor: &Editor) -> Vec<MenuItem> {
    let read_only = editor.state().read_only;
    let mut items: Vec<MenuItem> = Vec::new();
    for entry in verbs() {
        let Some(verb) = entry else {
            if items.last().is_some_and(|item| item.command.is_some()) {
                items.push(MenuItem::separator());
            }
            continue;
        };
        let Some(meta) = editor.commands().get(verb.id.as_str()) else {
            continue;
        };
        let hint = editor
            .key_hints()
            .primary_hint(verb.id.as_str())
            .map(|hint| hint.label(KeyLabelStyle::MacGlyphs).to_owned());
        items.push(MenuItem {
            // The one honest disable this face can state: a verb that would
            // change text the document refuses to change.
            enabled: !(meta.mutates_document() && read_only),
            command: Some(verb.id),
            label: verb.label,
            hint,
        });
    }
    while items.last().is_some_and(|item| item.command.is_none()) {
        items.pop();
    }
    items
}

#[cfg(test)]
mod tests {
    use iridium_editor::theme::Theme;
    use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

    use super::{ContextMenu, MenuOutcome};
    use crate::overlay::{PanelFit, PanelRow};

    /// The fit of a comfortable window.
    const FIT: PanelFit = PanelFit::popover(60, 24);

    /// A key press with no modifiers.
    fn press(key: KeyCode) -> KeyEvent {
        KeyEvent {
            key,
            modifiers: Modifiers::none(),
            is_repeat: false,
        }
    }

    /// A kernel carrying this face's commands and keymap.
    fn editor() -> Editor {
        let mut editor = Editor::with_defaults();
        for meta in crate::commands::command_metas() {
            editor
                .register_command(meta)
                .expect("the kernel accepted the command");
        }
        editor
            .push_keymap(crate::commands::keymap())
            .expect("the keymap validates");
        editor
    }

    /// An open menu over that kernel.
    fn open_menu() -> (ContextMenu, Editor) {
        let editor = editor();
        let menu = ContextMenu::open(&editor, 100.0, 200.0);
        (menu, editor)
    }

    /// The composed rows as plain text.
    fn rows_text(menu: &ContextMenu) -> Vec<String> {
        menu.content(&Theme::dark(), FIT)
            .rows
            .iter()
            .map(PanelRow::text)
            .collect()
    }

    #[test]
    fn every_menu_verb_is_a_command_the_registry_carries() {
        let editor = editor();
        for verb in super::verbs().into_iter().flatten() {
            assert!(
                editor.commands().contains(verb.id.as_str()),
                "the menu offers `{}`, which no registry carries",
                verb.id
            );
        }
    }

    #[test]
    fn the_menu_lists_the_ruled_verb_set_in_order() {
        let (menu, _editor) = open_menu();
        let labels: Vec<String> = rows_text(&menu);
        let verbs: Vec<&str> = labels
            .iter()
            .map(|row| row.trim())
            .filter(|row| !row.is_empty())
            .map(|row| row.split("  ").next().unwrap_or(row).trim())
            .collect();
        assert_eq!(
            verbs,
            vec!["Cut", "Copy", "Paste", "Select All", "Command Palette…"],
            "D-2's buildable floor, in order"
        );
    }

    #[test]
    fn the_labels_match_the_registry_titles_for_the_editing_verbs() {
        // The palette row is named for the panel it opens, not for the
        // command's own palette title; the four editing verbs must not drift
        // from the registry.
        let editor = editor();
        for verb in super::verbs().into_iter().flatten() {
            let meta = editor
                .commands()
                .get(verb.id.as_str())
                .expect("the verb is registered");
            if verb.id == iridium_editor::commands::builtin::PALETTE_OPEN {
                assert_eq!(verb.label, "Command Palette…");
            } else {
                assert_eq!(
                    verb.label,
                    meta.title(),
                    "`{}` drifted from its registry title",
                    verb.id
                );
            }
        }
    }

    #[test]
    fn escape_closes_the_menu() {
        let (mut menu, _editor) = open_menu();
        assert_eq!(
            menu.handle_key(&press(KeyCode::Escape)),
            MenuOutcome::Closed
        );
    }

    #[test]
    fn enter_runs_the_highlighted_verb() {
        let (mut menu, _editor) = open_menu();
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_CUT),
            "the first row is highlighted when the menu opens"
        );
        menu = ContextMenu::open(&editor(), 0.0, 0.0);
        menu.handle_key(&press(KeyCode::Down));
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_COPY)
        );
    }

    #[test]
    fn the_highlight_clamps_at_both_ends_rather_than_wrapping() {
        let (mut menu, _editor) = open_menu();
        // At the top already: `Up` must stay there, not wrap to the end.
        menu.handle_key(&press(KeyCode::Up));
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_CUT)
        );

        menu = ContextMenu::open(&editor(), 0.0, 0.0);
        for _ in 0..12 {
            menu.handle_key(&press(KeyCode::Down));
        }
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::PALETTE_OPEN),
            "`Down` past the end rests on the last verb"
        );
    }

    #[test]
    fn navigation_steps_over_the_separators() {
        let (mut menu, _editor) = open_menu();
        // Cut, Copy, Paste, ——, Select All: four `Down`s from Cut must land on
        // Select All, not on the rule above it.
        for _ in 0..3 {
            menu.handle_key(&press(KeyCode::Down));
        }
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::SELECTION_SELECT_ALL)
        );
    }

    #[test]
    fn home_and_end_reach_the_first_and_last_verbs() {
        let (mut menu, _editor) = open_menu();
        menu.handle_key(&press(KeyCode::End));
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::PALETTE_OPEN)
        );
        menu = ContextMenu::open(&editor(), 0.0, 0.0);
        menu.handle_key(&press(KeyCode::End));
        menu.handle_key(&press(KeyCode::Home));
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_CUT)
        );
    }

    #[test]
    fn a_read_only_document_greys_the_mutating_verbs_and_navigation_skips_them() {
        let mut editor = editor();
        editor.state_mut().read_only = true;
        let mut menu = ContextMenu::open(&editor, 10.0, 10.0);

        let content = menu.content(&Theme::dark(), FIT);
        let quiet = Theme::dark().editor.line_number;
        let cut = &content.rows[0];
        assert!(
            cut.spans.iter().all(|span| span.color == quiet),
            "a disabled row is drawn quiet: {:?}",
            cut.text()
        );

        // Cut and Paste mutate; Copy does not, so the highlight opens on Copy
        // and `Enter` runs it.
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_COPY)
        );
        menu = ContextMenu::open(&editor, 10.0, 10.0);
        menu.handle_key(&press(KeyCode::Down));
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::SELECTION_SELECT_ALL),
            "`Down` from Copy steps over the disabled Paste"
        );
    }

    #[test]
    fn cut_and_copy_stay_enabled_with_a_collapsed_selection() {
        // D-5: the kernel's clipboard verbs act on the caret line when nothing
        // is selected, so greying them would misstate them.
        let editor = editor();
        assert!(
            editor.state().cursor.primary.is_collapsed(),
            "a fresh document has no selection"
        );
        let menu = ContextMenu::open(&editor, 10.0, 10.0);
        let content = menu.content(&Theme::dark(), FIT);
        let base = Theme::dark().editor.foreground;
        for (index, verb) in [(0_usize, "Cut"), (1, "Copy")] {
            let row = &content.rows[index];
            assert!(
                row.spans.first().is_some_and(|span| span.color == base),
                "{verb} must not be greyed: {:?}",
                row.text()
            );
        }
    }

    #[test]
    fn the_menu_is_modal_and_swallows_what_it_does_not_bind() {
        let (mut menu, _editor) = open_menu();
        let save = KeyEvent {
            key: KeyCode::Char('s'),
            modifiers: Modifiers::ctrl(),
            is_repeat: false,
        };
        assert_eq!(menu.handle_key(&save), MenuOutcome::Handled);
        assert_eq!(
            menu.handle_key(&press(KeyCode::Char('x'))),
            MenuOutcome::Handled,
            "a printable key must not reach the document"
        );
    }

    #[test]
    fn a_row_carries_its_mac_key_hint() {
        let (menu, _editor) = open_menu();
        let rows = rows_text(&menu);
        let select_all = rows
            .iter()
            .find(|row| row.contains("Select All"))
            .expect("the menu lists Select All");
        assert!(
            select_all.contains('⌃') || select_all.contains('⌘'),
            "the hint is in mac glyphs: {select_all:?}"
        );
        assert!(
            !select_all.contains("Ctrl"),
            "the portable spelling must not appear on this face: {select_all:?}"
        );
    }

    #[test]
    fn a_click_runs_a_row_and_a_click_on_a_separator_runs_nothing() {
        let (mut menu, _editor) = open_menu();
        assert_eq!(menu.activate(3), None, "row 3 is the separator");
        assert_eq!(
            menu.activate(1),
            Some(iridium_editor::commands::builtin::CLIPBOARD_COPY)
        );
    }

    #[test]
    fn hovering_moves_the_highlight_only_onto_rows_that_can_run() {
        let (mut menu, _editor) = open_menu();
        assert!(menu.hover(2), "the highlight moved onto Paste");
        assert!(!menu.hover(2), "the same row does not move it again");
        assert!(!menu.hover(3), "a separator cannot be highlighted");
        assert_eq!(
            menu.handle_key(&press(KeyCode::Enter)),
            MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_PASTE)
        );
    }

    #[test]
    fn the_rows_never_exceed_the_windows_content_columns() {
        let (menu, _editor) = open_menu();
        for columns in [4_usize, 12, 20, 60] {
            let content = menu.content(&Theme::dark(), PanelFit::popover(columns, 24));
            assert!(content.content_columns <= columns);
            for row in &content.rows {
                assert!(
                    row.text().chars().count() <= content.content_columns,
                    "a row overran the panel: {:?}",
                    row.text()
                );
            }
        }
    }

    #[test]
    fn one_row_is_marked_selected_and_the_separators_are_marked_as_rules() {
        let (menu, _editor) = open_menu();
        let content = menu.content(&Theme::dark(), FIT);
        let selected: Vec<usize> = content
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.selected.then_some(index))
            .collect();
        assert_eq!(selected, vec![0], "the first verb is highlighted");
        let rules: Vec<usize> = content
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.separator.then_some(index))
            .collect();
        assert_eq!(rules, vec![3, 5], "the two ruled group breaks");
    }
}
