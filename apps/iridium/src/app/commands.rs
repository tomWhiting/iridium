//! The commands this face adds, and the keys they answer to.
//!
//! # These are host commands, not a second dispatch
//!
//! Every verb here is a real entry in the kernel's [`CommandRegistry`], bound
//! in a [`Keymap`] layer pushed on top of the default one. A key resolves
//! through the kernel's resolver like any other, the kernel reports it as
//! [`EditorKeyResult::HostCommand`](iridium_editor::EditorKeyResult::HostCommand),
//! and [`App`](super::App) runs it. That is the mechanism the kernel provides
//! for a command whose behaviour it cannot own — see
//! [`builtin::host`](iridium_editor::commands::builtin) — and using it means
//! these verbs are visible to anything that reads the registry, rebindable by
//! any later keymap layer, and impossible to reach by a private back door.
//!
//! Two of them are not this face's behaviour at all. [`FOLD_TOGGLE`] and its
//! neighbours run [`Editor::toggle_fold_at`](iridium_editor::Editor::toggle_fold_at)
//! and friends, which are the kernel's fold verbs; the kernel simply never gave
//! them command ids, so this face names and binds them. Nothing about folding
//! is implemented here.
//!
//! # Why these keys
//!
//! A terminal is not a browser, and the chord that works in one may not survive
//! the other. Under the legacy xterm encoding a control chord is a single C0
//! byte, so `Ctrl+X` and `Ctrl+Shift+X` are indistinguishable — see
//! [`iridium_tui::input`] — while `Alt` is an escape prefix and survives
//! intact. Every binding below is therefore either a bare `Ctrl` chord or an
//! `Alt` chord, and none of them relies on `Shift` reaching us through a
//! control byte.
//!
//! | Key | Command | Note |
//! |---|---|---|
//! | `Ctrl+S` | [`FILE_SAVE`] | refuses a file that changed on disk |
//! | `Ctrl+Alt+S` | [`FILE_SAVE_FORCE`] | saves anyway |
//! | `F5` | [`FILE_RELOAD`] | re-reads the file |
//! | `Ctrl+Q` | [`APP_QUIT`] | asks again when there are unsaved changes |
//! | `Ctrl+G` | [`GOTO_LINE`] | opens the line prompt |
//! | `Alt+F` | [`FOLD_TOGGLE`] | the innermost fold around the caret |
//! | `Ctrl+Alt+F` | [`FOLD_ALL`] | |
//! | `Alt+U` | [`FOLD_NONE`] | |
//! | `Ctrl+Alt+D` | [`MULTI_CURSOR_SKIP_LAST_OCCURRENCE`] | a **kernel** verb the default keymap leaves unbound |
//!
//! `Ctrl+S` is the one chord a terminal can steal: it is XOFF under software
//! flow control. Raw mode clears `IXON`, which is what
//! [`Driver::open`](iridium_tui::driver::Driver::open) enters before anything
//! else, so the byte reaches this program rather than freezing the terminal.
//!
//! `Ctrl+Alt+D` is the reason this module binds a command it does not
//! implement. [`MULTI_CURSOR_SKIP_LAST_OCCURRENCE`] is implemented by the
//! kernel and deliberately left unbound by the default keymap, reachable
//! through the palette. This face bound it before it had a palette, and the
//! binding stays now that it does: skipping an occurrence happens mid-flow,
//! between two presses of `Ctrl+D`, and a palette round trip there would lose
//! the very selection being skipped.

use iridium_editor::commands::builtin::MULTI_CURSOR_SKIP_LAST_OCCURRENCE;
use iridium_editor::{
    CommandCategory, CommandId, CommandMeta, KeyBinding, KeyCode, Keymap, ModifierPattern,
    ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

/// Write the document to its file.
pub const FILE_SAVE: CommandId = CommandId::from_static("file.save");
/// Write the document to its file even though the file changed on disk.
pub const FILE_SAVE_FORCE: CommandId = CommandId::from_static("file.saveForce");
/// Re-read the file, discarding unsaved changes.
pub const FILE_RELOAD: CommandId = CommandId::from_static("file.reload");
/// Leave the editor.
pub const APP_QUIT: CommandId = CommandId::from_static("app.quit");
/// Move the caret to a line by number.
pub const GOTO_LINE: CommandId = CommandId::from_static("goto.line");
/// Fold or unfold the innermost region around the caret.
pub const FOLD_TOGGLE: CommandId = CommandId::from_static("fold.toggle");
/// Fold every foldable region.
pub const FOLD_ALL: CommandId = CommandId::from_static("fold.all");
/// Unfold every folded region.
pub const FOLD_NONE: CommandId = CommandId::from_static("fold.none");

/// Every command this face contributes, in the order the palette would list.
pub static COMMANDS: &[CommandMeta] = &[
    CommandMeta::described(
        FILE_SAVE,
        "Save",
        "Writes the document to its file, refusing if the file changed on disk.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["write", "w"]),
    CommandMeta::described(
        FILE_SAVE_FORCE,
        "Save Anyway",
        "Writes the document to its file even though the file changed on disk.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["overwrite", "force write", "w!"]),
    CommandMeta::described(
        FILE_RELOAD,
        "Reload From Disk",
        "Re-reads the file, discarding unsaved changes.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["revert", "e!"]),
    CommandMeta::described(
        APP_QUIT,
        "Quit",
        "Leaves the editor, asking again when there are unsaved changes.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["exit", "close", "q"]),
    CommandMeta::described(
        GOTO_LINE,
        "Go to Line",
        "Moves the caret to a line by number, revealing it if a fold hides it.",
        CommandCategory::NAVIGATION,
    )
    .with_aliases(&["jump", "line number"]),
    CommandMeta::described(
        FOLD_TOGGLE,
        "Toggle Fold",
        "Folds or unfolds the innermost region around the caret.",
        CommandCategory::from_static("Folding"),
    )
    .with_aliases(&["collapse", "expand", "fold"]),
    CommandMeta::described(
        FOLD_ALL,
        "Fold All",
        "Folds every foldable region in the document.",
        CommandCategory::from_static("Folding"),
    )
    .with_aliases(&["collapse all"]),
    CommandMeta::described(
        FOLD_NONE,
        "Unfold All",
        "Unfolds every folded region in the document.",
        CommandCategory::from_static("Folding"),
    )
    .with_aliases(&["expand all", "unfold"]),
];

/// The number of commands this face contributes.
///
/// Derived from [`COMMANDS`] so it cannot drift from the table.
pub const COMMAND_COUNT: usize = COMMANDS.len();

/// Shorthand for a modifier pattern, in the field order of [`ModifierPattern`].
const fn pattern(
    shift: ModifierState,
    ctrl: ModifierState,
    alt: ModifierState,
    meta: ModifierState,
    alt_graph: ModifierState,
) -> ModifierPattern {
    ModifierPattern::new(shift, ctrl, alt, meta, alt_graph)
}

/// A bare `Ctrl` chord: `Shift` is ignored because a terminal cannot always
/// report it, `Alt` and `Meta` must be absent so the `Ctrl+Alt` chords below
/// stay reachable.
const CTRL: ModifierPattern = pattern(Any, Required, Forbidden, Forbidden, Any);

/// A `Ctrl+Alt` chord. `AltGraph` is forbidden for the reason the kernel's
/// add-cursor chord forbids it: on many layouts `AltGr` is reported as
/// `Ctrl+Alt` while composing a character, and a save is not something to do by
/// accident while typing `@`.
const CTRL_ALT: ModifierPattern = pattern(Any, Required, Required, Forbidden, Forbidden);

/// An `Alt` chord with no other modifier.
const ALT: ModifierPattern = pattern(Forbidden, Forbidden, Required, Forbidden, Forbidden);

/// A function key with any modifiers.
const FUNCTION: ModifierPattern = ModifierPattern::ANY;

/// No continuation: every binding here is a single chord.
const CHORD: &[StrokePattern] = &[];

/// The binding table: `(stroke, command)`.
const BINDINGS: &[(StrokePattern, CommandId)] = &[
    (StrokePattern::new(KeyCode::Char('s'), CTRL), FILE_SAVE),
    (
        StrokePattern::new(KeyCode::Char('s'), CTRL_ALT),
        FILE_SAVE_FORCE,
    ),
    (StrokePattern::new(KeyCode::F5, FUNCTION), FILE_RELOAD),
    (StrokePattern::new(KeyCode::Char('q'), CTRL), APP_QUIT),
    (StrokePattern::new(KeyCode::Char('g'), CTRL), GOTO_LINE),
    (StrokePattern::new(KeyCode::Char('f'), ALT), FOLD_TOGGLE),
    (StrokePattern::new(KeyCode::Char('f'), CTRL_ALT), FOLD_ALL),
    (StrokePattern::new(KeyCode::Char('u'), ALT), FOLD_NONE),
    (
        StrokePattern::new(KeyCode::Char('d'), CTRL_ALT),
        MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
    ),
];

/// The number of bindings this face adds.
///
/// Derived from [`BINDINGS`] so it cannot drift from the table.
pub const BINDING_COUNT: usize = BINDINGS.len();

/// Builds the keymap layer this face pushes onto the kernel's default.
///
/// A layer rather than a replacement: everything the default keymap binds keeps
/// working, and a user keymap pushed later still overrides both.
#[must_use]
pub fn keymap() -> Keymap {
    let mut keymap = Keymap::new("iridium-terminal");
    for (stroke, command) in BINDINGS {
        keymap.push(KeyBinding::new(*stroke, CHORD, command.clone()));
    }
    keymap
}

/// Every command this face contributes, cloned for registration.
#[must_use]
pub fn command_metas() -> Vec<CommandMeta> {
    COMMANDS.to_vec()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use iridium_editor::Editor;
    use iridium_editor::commands::default_non_modal_keymap;

    use super::*;

    #[test]
    fn every_command_this_face_adds_is_one_the_kernel_does_not_implement() {
        // A face command that shadows a kernel one would be the exact mistake
        // this crate is written to avoid: two implementations of one id, and
        // whichever the dispatch reaches first wins.
        for meta in COMMANDS {
            assert!(
                !Editor::implements_command(meta.id().as_str()),
                "{} is already implemented by the kernel",
                meta.id()
            );
        }
    }

    #[test]
    fn no_two_commands_share_an_id() {
        let ids: BTreeSet<&str> = COMMANDS.iter().map(|meta| meta.id().as_str()).collect();
        assert_eq!(ids.len(), COMMAND_COUNT, "a command id is duplicated");
    }

    #[test]
    fn every_command_carries_a_title_and_a_description() {
        for meta in COMMANDS {
            assert!(!meta.title().is_empty(), "{} has no title", meta.id());
            assert!(
                meta.description().is_some_and(|text| !text.is_empty()),
                "{} has no description",
                meta.id()
            );
        }
    }

    #[test]
    fn every_command_except_the_borrowed_kernel_verb_is_bound() {
        // Every verb this face contributes is core enough to deserve a key:
        // save, quit, reload, go-to-line and the folds are pressed dozens of
        // times an hour, and "reachable through the palette" is the right
        // answer for the long tail, not for these.
        let bound: BTreeSet<&str> = BINDINGS
            .iter()
            .map(|(_, command)| command.as_str())
            .collect();
        for meta in COMMANDS {
            assert!(
                bound.contains(meta.id().as_str()),
                "{} has no key, and this face's own verbs all deserve one",
                meta.id()
            );
        }
    }

    #[test]
    fn the_kernel_verb_the_default_keymap_leaves_unbound_is_bound_here() {
        let bound: BTreeSet<&str> = BINDINGS
            .iter()
            .map(|(_, command)| command.as_str())
            .collect();
        assert!(
            bound.contains(MULTI_CURSOR_SKIP_LAST_OCCURRENCE.as_str()),
            "the skip-occurrence verb runs mid-flow and must keep its key"
        );
        assert!(
            Editor::implements_command(MULTI_CURSOR_SKIP_LAST_OCCURRENCE.as_str()),
            "the kernel stopped implementing the verb this face binds"
        );
    }

    #[test]
    fn no_binding_collides_with_the_default_keymap() {
        // Two layers may legitimately bind one stroke — that is what a layer is
        // for — but this face is adding verbs, not rebinding the kernel's, so
        // an overlap here means a key silently stopped doing what it did.
        let defaults = default_non_modal_keymap();
        for (stroke, command) in BINDINGS {
            for binding in defaults.bindings() {
                let Some((first, rest)) = binding.sequence().split_first() else {
                    continue;
                };
                if !rest.is_empty() {
                    continue;
                }
                assert!(
                    !first.overlaps(stroke),
                    "{command} shadows the default binding for {:?}",
                    binding.command()
                );
            }
        }
    }

    #[test]
    fn the_keymap_binds_every_row_of_the_table() {
        let keymap = keymap();
        assert_eq!(keymap.len(), BINDING_COUNT);
        let bound: BTreeSet<String> = keymap
            .bindings()
            .iter()
            .filter_map(|binding| binding.command().map(ToString::to_string))
            .collect();
        for (_, command) in BINDINGS {
            assert!(bound.contains(command.as_str()), "{command} is not bound");
        }
    }

    #[test]
    fn the_keymap_validates_against_a_registry_holding_these_commands() {
        // `push_keymap` runs this check for real at start-up; running it here
        // means a mistyped id fails in a unit test rather than at launch.
        let mut editor = Editor::with_defaults();
        for meta in command_metas() {
            editor
                .register_command(meta)
                .expect("the kernel accepted the command");
        }
        editor.push_keymap(keymap()).expect("the keymap validates");
    }
}
