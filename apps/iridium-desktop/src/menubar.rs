//! The macOS menu bar: the ruled set of menus, and what each row says.
//!
//! # What #108 actually is
//!
//! ⚠️ Not *"build a menu bar"*. winit already builds one and installs it —
//! `EventLoopBuilderExtMacOS::with_default_menu` defaults to on, and
//! `run.rs` builds a plain `EventLoop`, so every session has had a menu bar
//! since the first one. What it contains is **exactly one menu**, the
//! application menu: *About*, *Services*, *Hide*, *Hide Others*, *Show All*,
//! *Quit*. There is no *File*, no *Edit*, no *View*. So the task is adding
//! menus beside the one that is already there, and ⛔ **never replacing it** —
//! a replacement loses *Quit* and *Hide* for nothing. See
//! `docs/IN-FLIGHT-108-menu-bar.md` for the ground this was verified against.
//!
//! That is also why nothing here names *Quit*, *About* or *Hide*: they are
//! already on screen, and a second copy under *File* would be two doors to
//! one room, one of which the system draws itself.
//!
//! # ⛔ No key equivalents in this slice, and that is a ruling
//!
//! An `NSMenuItem` shows a chord by *claiming* it: `setKeyEquivalent` makes
//! `AppKit` intercept that chord before the window ever sees it. There is no
//! supported way to display one without taking it. Taking `⌘S`, `⌘O` and
//! `⌘Z` would move this face's most-used keys off the winit path — the one
//! the ten gates cover, the one the latency instrument measures, the one the
//! modal-panel guards sit on — and onto an `AppKit` path **no test in this
//! repository can reach**. That is the wrong trade for a task whose own title
//! is *"for the hand that does not know the chord"*: a hand that knows it
//! keeps the path that is proven.
//!
//! So rows carry titles and are clicked. The chord is resolved anyway — it is
//! [`ResolvedVerb::hint`], read from the keymap — because the moment the
//! action path has been exercised in a real session, turning it into a key
//! equivalent is one call and no new decision. Until then nothing displays it,
//! because a menu that shows a chord it does not own is furniture promising
//! what the routing does not deliver.
//!
//! # Where the words come from
//!
//! Titles and chords are read from the registry and the keymap through
//! [`crate::verbs::resolve`], shared with the drawn context menu. The labels
//! below are this menu's own, for the reason that module documents; the test
//! at the foot pins every one that must agree with a registry title.
//!
//! # The other faces
//!
//! The terminal face gets no menu bar and this module does not give it one —
//! its way in is the palette, and a terminal has nowhere to hang one. The web
//! face has the browser's own. Naming both so neither is silently assumed.

use iridium_editor::Editor;
use iridium_editor::commands::builtin::{
    AST_EXPAND_SELECTION, AST_SHRINK_SELECTION, CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE,
    COMMENT_TOGGLE_LINE, CONFIG_RELOAD, CURSOR_DOCUMENT_END, CURSOR_DOCUMENT_START,
    EXPLORER_TOGGLE_PANEL, EXPLORER_TOGGLE_SIDEBAR, HISTORY_REDO, HISTORY_TOGGLE_PANEL,
    HISTORY_UNDO, LINES_DELETE, MULTI_CURSOR_ADD_CURSOR_ABOVE, MULTI_CURSOR_ADD_CURSOR_BELOW,
    MULTI_CURSOR_SELECT_ALL_OCCURRENCES, PALETTE_OPEN, SEARCH_NEXT_MATCH, SEARCH_OPEN,
    SEARCH_PREVIOUS_MATCH, SELECTION_SELECT_ALL, VIEW_TOGGLE_THEME, WORKSPACE_CLOSE_TAB,
    WORKSPACE_NEXT_TAB, WORKSPACE_PREVIOUS_TAB,
};

use crate::commands::{
    COMMANDS_LIST, CONFIG_EDIT, FILE_NEW, FILE_OPEN, FILE_SAVE, FILE_SAVE_AS, PROJECT_OPEN,
    PROJECT_SET,
};
use crate::verbs::{Availability, ResolvedVerb, Row, resolve};

/// One menu of the bar, resolved against a live session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    /// The word on the bar.
    pub title: &'static str,
    /// The rows, in order; `None` is a separator.
    pub rows: Vec<Option<ResolvedVerb>>,
}

impl Menu {
    /// The commands this menu can run, greyed rows included.
    ///
    /// Used by the tests and by whatever installs the menu; separators have no
    /// command and are skipped.
    pub fn commands(&self) -> impl Iterator<Item = &ResolvedVerb> {
        self.rows.iter().filter_map(Option::as_ref)
    }
}

/// The number of menus this face contributes, beside the application menu
/// winit already installs.
///
/// Asserted against [`ruled`] in the tests so the documented count cannot
/// drift from the table.
pub const MENU_COUNT: usize = 6;

/// The ruled set (M-1): which menus there are, and which ids in each.
///
/// ⚠️ **A ruled table, not a registry sweep.** The palette exists so that
/// every command is reachable by name; a menu bar that listed all of them
/// would be a hundred rows in six columns and no way in at all. What earns a
/// row here is being something a hand reaches for without knowing it has a
/// name — which is a judgement, and so it is written down rather than derived.
///
/// The order within each menu follows the platform: destructive and
/// document-scoped things last, the thing you came for first.
fn ruled() -> Vec<(&'static str, Vec<Row>)> {
    vec![
        (
            "File",
            vec![
                Row::verb(FILE_NEW, "New File"),
                Row::verb(FILE_OPEN, "Open File…"),
                Row::verb(PROJECT_OPEN, "Open Folder…"),
                Row::verb(PROJECT_SET, "Set Project to the Explorer's Folder"),
                Row::Rule,
                Row::verb(FILE_SAVE, "Save"),
                Row::verb(FILE_SAVE_AS, "Save As…"),
                Row::Rule,
                Row::verb(WORKSPACE_CLOSE_TAB, "Close Tab"),
            ],
        ),
        (
            "Edit",
            vec![
                Row::verb(HISTORY_UNDO, "Undo"),
                Row::verb(HISTORY_REDO, "Redo"),
                Row::Rule,
                Row::verb(CLIPBOARD_CUT, "Cut"),
                Row::verb(CLIPBOARD_COPY, "Copy"),
                Row::verb(CLIPBOARD_PASTE, "Paste"),
                Row::Rule,
                Row::verb(SEARCH_OPEN, "Find…"),
                Row::verb(SEARCH_NEXT_MATCH, "Find Next"),
                Row::verb(SEARCH_PREVIOUS_MATCH, "Find Previous"),
                Row::Rule,
                Row::verb(COMMENT_TOGGLE_LINE, "Toggle Comment"),
                Row::verb(LINES_DELETE, "Delete Line"),
            ],
        ),
        (
            "Selection",
            vec![
                Row::verb(SELECTION_SELECT_ALL, "Select All"),
                Row::Rule,
                // The daily driver, and the reason this menu exists rather
                // than folding into Edit: it is the verb hardest to discover
                // and the one most worth discovering.
                Row::verb(AST_EXPAND_SELECTION, "Expand Selection"),
                Row::verb(AST_SHRINK_SELECTION, "Shrink Selection"),
                Row::Rule,
                Row::verb(MULTI_CURSOR_ADD_CURSOR_ABOVE, "Add Cursor Above"),
                Row::verb(MULTI_CURSOR_ADD_CURSOR_BELOW, "Add Cursor Below"),
                Row::verb(
                    MULTI_CURSOR_SELECT_ALL_OCCURRENCES,
                    "Select All Occurrences",
                ),
            ],
        ),
        (
            "View",
            vec![
                Row::verb(EXPLORER_TOGGLE_SIDEBAR, "Sidebar"),
                Row::verb(EXPLORER_TOGGLE_PANEL, "File Explorer"),
                Row::verb(HISTORY_TOGGLE_PANEL, "History"),
                Row::Rule,
                Row::verb(VIEW_TOGGLE_THEME, "Switch Light and Dark"),
            ],
        ),
        (
            "Go",
            vec![
                Row::verb(PALETTE_OPEN, "Command Palette…"),
                Row::Rule,
                Row::verb(WORKSPACE_NEXT_TAB, "Next Tab"),
                Row::verb(WORKSPACE_PREVIOUS_TAB, "Previous Tab"),
                Row::Rule,
                Row::verb(CURSOR_DOCUMENT_START, "Start of Document"),
                Row::verb(CURSOR_DOCUMENT_END, "End of Document"),
            ],
        ),
        (
            "Help",
            vec![
                Row::verb(COMMANDS_LIST, "List Every Command"),
                Row::Rule,
                Row::verb(CONFIG_EDIT, "Edit Configuration"),
                Row::verb(CONFIG_RELOAD, "Reload Configuration"),
            ],
        ),
    ]
}

/// Builds the bar for this session.
///
/// A menu whose every verb was dropped is dropped with them — an empty word on
/// the bar that opens onto nothing is worse than no word. That can only happen
/// to a face that registers fewer commands than this one does, but the rule is
/// stated in code rather than in a comment about what cannot happen.
#[must_use]
pub fn menus(editor: &Editor, availability: Availability) -> Vec<Menu> {
    ruled()
        .into_iter()
        .map(|(title, verbs)| Menu {
            title,
            rows: resolve(editor, &verbs, availability),
        })
        .filter(|menu| menu.commands().next().is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use iridium_editor::Editor;

    use super::{MENU_COUNT, Menu, menus, ruled};
    use crate::verbs::{Availability, Row};

    /// A kernel carrying this face's commands and keymap — the session shape
    /// the bar is built against in a running window.
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

    /// The bar of an ordinary editable session.
    fn bar() -> Vec<Menu> {
        menus(&editor(), Availability::OPEN)
    }

    #[test]
    fn the_documented_count_matches_the_table() {
        assert_eq!(ruled().len(), MENU_COUNT);
    }

    /// ⭐ The assertion the whole module turns on. Every id in the ruled set
    /// must be one this face actually registers — a row the registry does not
    /// carry is silently *dropped*, so a typo or a renamed command would not
    /// fail anything, it would just quietly shorten a menu. This is the test
    /// that makes that impossible.
    #[test]
    fn every_ruled_id_is_a_command_this_face_registers() {
        let editor = editor();
        let mut missing: Vec<String> = Vec::new();
        for (title, verbs) in ruled() {
            for verb in verbs.iter().filter_map(Row::as_verb) {
                if editor.commands().get(verb.id.as_str()).is_none() {
                    missing.push(format!("{title} › {} ({})", verb.label, verb.id));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "the menu bar names commands nothing registers: {missing:?}"
        );
    }

    /// A menu bar is navigated by memory of where a thing was, so no verb may
    /// appear under two words.
    #[test]
    fn no_command_appears_in_two_menus() {
        let mut seen: HashSet<String> = HashSet::new();
        let mut twice: Vec<String> = Vec::new();
        for (_, verbs) in ruled() {
            for verb in verbs.iter().filter_map(Row::as_verb) {
                if !seen.insert(verb.id.as_str().to_owned()) {
                    twice.push(verb.id.as_str().to_owned());
                }
            }
        }
        assert!(twice.is_empty(), "these are on the bar twice: {twice:?}");
    }

    /// ⛔ The application menu is winit's, and a second *Quit* or *About*
    /// under *File* would be two doors to one room. Named as an assertion
    /// because the temptation to add them is the obvious next thought.
    #[test]
    fn nothing_duplicates_the_application_menu() {
        for menu in bar() {
            for row in menu.commands() {
                assert!(
                    !["Quit", "About", "Hide", "Services"].contains(&row.label),
                    "`{}` is already on the application menu winit installs",
                    row.label
                );
            }
        }
    }

    /// The bar is built, not typed: every menu resolved and none came back
    /// empty.
    #[test]
    fn every_menu_survives_a_session_that_registers_this_faces_commands() {
        let titles: Vec<&str> = bar().iter().map(|menu| menu.title).collect();
        assert_eq!(
            titles,
            vec!["File", "Edit", "Selection", "View", "Go", "Help"]
        );
    }

    /// ⚠️ A menu with a leading, trailing or doubled rule is a menu that lost
    /// a verb and kept its scar. The trimming lives in [`crate::verbs`]; this
    /// asserts the bar as a whole comes out clean.
    #[test]
    fn no_menu_has_a_stray_rule() {
        for menu in bar() {
            assert!(
                menu.rows.first().is_some_and(Option::is_some),
                "{} opens with a rule",
                menu.title
            );
            assert!(
                menu.rows.last().is_some_and(Option::is_some),
                "{} ends with a rule",
                menu.title
            );
            assert!(
                !menu
                    .rows
                    .windows(2)
                    .any(|pair| pair.iter().all(Option::is_none)),
                "{} has two rules together",
                menu.title
            );
        }
    }

    /// The chord is read off the keymap and never typed here. Not every row
    /// has one — several are deliberately palette-only — so this asserts about
    /// the ones that do rather than about all of them.
    #[test]
    fn the_chords_are_read_from_the_keymap() {
        let hints: Vec<String> = bar()
            .iter()
            .flat_map(|menu| menu.commands().filter_map(|row| row.hint.clone()))
            .collect();
        assert!(
            hints.len() >= 10,
            "only {} rows resolved a chord, so the hint index is not being read",
            hints.len()
        );
        for hint in hints {
            assert!(
                !hint.contains('~'),
                "`{hint}` is the round-trippable form, not the presentable one"
            );
        }
    }

    /// A read-only document greys what writes to it, and the greying reaches
    /// the bar rather than stopping at the context menu.
    #[test]
    fn a_read_only_document_greys_the_writers_on_the_bar() {
        let bar = menus(
            &editor(),
            Availability {
                read_only: true,
                inert: false,
            },
        );
        let edit = bar
            .iter()
            .find(|menu| menu.title == "Edit")
            .expect("the Edit menu is on the bar");
        let paste = edit
            .commands()
            .find(|row| row.label == "Paste")
            .expect("Paste is on the Edit menu");
        assert!(!paste.enabled, "paste writes to a document that refuses it");
        let go = bar
            .iter()
            .find(|menu| menu.title == "Go")
            .expect("the Go menu is on the bar");
        let start = go
            .commands()
            .find(|row| row.label == "Start of Document")
            .expect("Start of Document is on the Go menu");
        assert!(start.enabled, "moving the caret writes nothing");
    }

    /// ⭐ Every label is shown to somebody, so none may be an id, an enum name
    /// or empty — the failures that would each look like a broken build in the
    /// one place a user cannot avoid looking.
    #[test]
    fn every_label_reads_as_something_a_person_chose() {
        for menu in bar() {
            for row in menu.commands() {
                assert!(!row.label.is_empty(), "{} has a blank row", menu.title);
                assert!(
                    !row.label.contains('.'),
                    "`{}` reads as an id, not a menu row",
                    row.label
                );
                assert!(
                    row.label.chars().next().is_some_and(char::is_uppercase),
                    "`{}` is not title case",
                    row.label
                );
            }
        }
    }
}
