//! The commands this face adds, and the keys they answer to.
//!
//! # These are host commands, not a second dispatch
//!
//! Every verb here is a real entry in the kernel's `CommandRegistry`, bound in
//! a [`Keymap`] layer pushed on top of the default one — the identical
//! mechanism the terminal face uses, described at length in its
//! `app::commands`. A key resolves through the kernel's resolver like any
//! other, the kernel reports it as
//! [`EditorKeyResult::HostCommand`](iridium_editor::EditorKeyResult::HostCommand),
//! and [`DesktopApp`](crate::app::DesktopApp) runs it.
//!
//! The two ids this face contributes are **the terminal face's ids,
//! character for character**: `file.save` and `file.saveForce`. One id and
//! one meaning across every face is the whole argument of the kernel's
//! `builtin::host` module, and two faces spelling the same verb differently
//! is exactly the drift it warns about.
//!
//! # The ⌘ layer
//!
//! The kernel's default keymap binds only `Ctrl` chords — a terminal cannot
//! reliably see ⌘ — and every one of its patterns forbids `meta`, so a bare
//! ⌘C on this face would fall through dead. This layer is where the desktop
//! plan's "⌘C/⌘V/⌘Z work natively" line is honored: the ⌘ rows below bind
//! **kernel-implemented** commands (clipboard, undo, redo, select-all) to
//! their macOS chords. Nothing is reimplemented — the rows route the mac
//! chord to the verb the kernel already runs for the `Ctrl` spelling, so the
//! two spellings cannot disagree.
//!
//! | Key | Command | Note |
//! |---|---|---|
//! | `Ctrl+S` / `⌘S` | [`FILE_SAVE`] | refuses a file that changed on disk |
//! | `Ctrl+Alt+S` / `⌘⌥S` | [`FILE_SAVE_FORCE`] | saves anyway |
//! | `⌘C` | `clipboard.copy` | kernel verb, mac chord |
//! | `⌘X` | `clipboard.cut` | kernel verb, mac chord |
//! | `⌘V` | `clipboard.paste` | kernel verb, mac chord |
//! | `⌘Z` | `history.undo` | kernel verb, mac chord |
//! | `⌘⇧Z` | `history.redo` | kernel verb, mac chord |
//! | `⌘A` | `selection.selectAll` | kernel verb, mac chord |
//!
//! The `Ctrl` spellings of the kernel verbs stay bound by the default keymap
//! underneath this layer; both work.

use iridium_editor::commands::builtin::{
    CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE, HISTORY_REDO, HISTORY_UNDO,
    SELECTION_SELECT_ALL,
};
use iridium_editor::{
    CommandCategory, CommandId, CommandMeta, KeyBinding, KeyCode, Keymap, ModifierPattern,
    ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

/// Write the document to its file.
pub const FILE_SAVE: CommandId = CommandId::from_static("file.save");
/// Write the document to its file even though the file changed on disk.
pub const FILE_SAVE_FORCE: CommandId = CommandId::from_static("file.saveForce");

/// Every command this face contributes, in declaration order.
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

/// A bare `Ctrl` chord: `Shift` ignored, `Alt` and `Meta` absent — the same
/// shape the terminal face binds its save under, so a keymap reads the same
/// across both.
const CTRL: ModifierPattern = pattern(Any, Required, Forbidden, Forbidden, Any);

/// A `Ctrl+Alt` chord. `AltGraph` is forbidden for the reason the kernel's
/// add-cursor chord forbids it: on many layouts `AltGr` is reported as
/// `Ctrl+Alt` while composing a character, and a forced save is not something
/// to do by accident while typing `@`.
const CTRL_ALT: ModifierPattern = pattern(Any, Required, Required, Forbidden, Forbidden);

/// A bare ⌘ chord: `Shift` ignored — ⌘C copies with or without it, exactly as
/// the default keymap's `Ctrl+C` does — `Ctrl` and `Alt` absent.
const META: ModifierPattern = pattern(Any, Forbidden, Forbidden, Required, Any);

/// A ⌘ chord that requires `Shift` to be absent: `⌘Z` must not swallow `⌘⇧Z`.
const META_NO_SHIFT: ModifierPattern = pattern(Forbidden, Forbidden, Forbidden, Required, Any);

/// A `⌘⇧` chord.
const META_SHIFT: ModifierPattern = pattern(Required, Forbidden, Forbidden, Required, Any);

/// A `⌘⌥` chord, with the `AltGraph` guard [`CTRL_ALT`] carries.
const META_ALT: ModifierPattern = pattern(Any, Forbidden, Required, Required, Forbidden);

/// No continuation: every binding here is a single chord.
const CHORD: &[StrokePattern] = &[];

/// The binding table: `(stroke, command)`.
const BINDINGS: &[(StrokePattern, CommandId)] = &[
    (StrokePattern::new(KeyCode::Char('s'), CTRL), FILE_SAVE),
    (StrokePattern::new(KeyCode::Char('s'), META), FILE_SAVE),
    (
        StrokePattern::new(KeyCode::Char('s'), CTRL_ALT),
        FILE_SAVE_FORCE,
    ),
    (
        StrokePattern::new(KeyCode::Char('s'), META_ALT),
        FILE_SAVE_FORCE,
    ),
    (StrokePattern::new(KeyCode::Char('c'), META), CLIPBOARD_COPY),
    (StrokePattern::new(KeyCode::Char('x'), META), CLIPBOARD_CUT),
    (
        StrokePattern::new(KeyCode::Char('v'), META),
        CLIPBOARD_PASTE,
    ),
    (
        StrokePattern::new(KeyCode::Char('z'), META_NO_SHIFT),
        HISTORY_UNDO,
    ),
    (
        StrokePattern::new(KeyCode::Char('z'), META_SHIFT),
        HISTORY_REDO,
    ),
    (
        StrokePattern::new(KeyCode::Char('a'), META),
        SELECTION_SELECT_ALL,
    ),
];

/// The number of bindings this face adds.
///
/// Derived from [`BINDINGS`] so it cannot drift from the table.
pub const BINDING_COUNT: usize = BINDINGS.len();

/// Builds the keymap layer this face pushes onto the kernel's default.
///
/// A layer rather than a replacement: everything the default keymap binds
/// keeps working, and a user keymap pushed later still overrides both.
#[must_use]
pub fn keymap() -> Keymap {
    let mut keymap = Keymap::new("iridium-desktop");
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
    fn every_borrowed_kernel_verb_is_still_a_kernel_verb() {
        // The ⌘ layer binds only verbs the kernel implements; a row here for
        // an id the kernel dropped would be a mac chord that consumes the key
        // and does nothing.
        let ours: BTreeSet<&str> = COMMANDS.iter().map(|meta| meta.id().as_str()).collect();
        for (_, command) in BINDINGS {
            if ours.contains(command.as_str()) {
                continue;
            }
            assert!(
                Editor::implements_command(command.as_str()),
                "{command} is bound here but the kernel does not implement it"
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
    fn every_command_this_face_adds_is_bound() {
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
    fn the_save_ids_are_the_terminal_faces_ids() {
        // One id, one meaning, across every face. The strings are asserted
        // here because the terminal face declares its own constants and
        // nothing else would notice the two crates drifting apart.
        assert_eq!(FILE_SAVE.as_str(), "file.save");
        assert_eq!(FILE_SAVE_FORCE.as_str(), "file.saveForce");
    }

    #[test]
    fn no_binding_collides_with_the_default_keymap() {
        // Two layers may legitimately bind one stroke — that is what a layer
        // is for — but this face is adding verbs and mac spellings, not
        // rebinding the kernel's `Ctrl` chords, so an overlap here means a key
        // silently stopped doing what it did.
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
