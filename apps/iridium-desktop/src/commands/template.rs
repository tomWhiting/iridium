//! This face's contribution to the configuration file a first run writes.
//!
//! # Why the face has to hand its own keys over
//!
//! [`iridium_config`] generates the `[keys]` block from the real keymaps rather
//! than from a hand-written list, so it cannot name a chord that has moved. It
//! reaches the kernel's default keymap and the file explorer's directly, because
//! both are dependencies of it. **It cannot reach this one**: a face depends on
//! `iridium_config`, so the edge cannot run the other way.
//!
//! ⚠️ That is not a cosmetic gap. This face's layer is where every `⌘` chord
//! lives — `⌘C`, `⌘S`, `⌘⇧P`, the whole mac spelling — so a file that listed
//! only what the config crate could see would omit precisely the bindings the
//! person on this face is most likely to want to change, while claiming in its
//! own header to list them all.
//!
//! # One function, called from both places
//!
//! Startup writes the file as a courtesy; *Edit Configuration* writes it because
//! somebody asked. Both need the same contribution, and a second call site
//! assembling its own [`FaceKeys`] would be a second place to forget one — so
//! there is one function and neither caller sees the details.

use std::io;
use std::path::Path;

use iridium_config::{Created, FaceKeys};

use super::{command_metas, keymap};

/// The heading this face's bindings appear under in the written file.
const TITLE: &str = "The desktop app";

/// What is said under that heading.
const NOTE: &str = "\
The chords this window adds on top of everything above — the mac spelling of
the common verbs, and the things only a windowed app can do (open a file, save
one, switch tabs).";

/// The heading the command palette's own bindings appear under.
const PALETTE_TITLE: &str = "The command palette";

/// What is said under that heading.
const PALETTE_NOTE: &str = "\
The keys the palette answers while it is open. They apply there and nowhere
else, so binding one of these takes nothing away from the document.";

/// Writes the default configuration file at `path` if nothing is there,
/// including this face's own bindings and commands in the list.
///
/// # Errors
///
/// Whatever [`iridium_config::create_if_absent`] reports: the directory could
/// not be made, or the file could not be written.
pub fn create_config_if_absent(path: &Path) -> io::Result<Created> {
    // Both are built here and borrowed into the descriptor, rather than being
    // `'static` tables: `keymap()` assembles a `Keymap` and `command_metas()` a
    // `Vec`, and neither is worth caching for a call that happens at most twice
    // in a session.
    let keymap = keymap();
    let commands = command_metas();
    // ⚠️ **The palette is a second layer this face owns, and it needs its own
    // entry.** Its keys live in the panel (#117), which `iridium_config` cannot
    // reach for the same reason it cannot reach this face's — so sixteen chords
    // would otherwise be absent from a file whose header says it lists them all.
    // No commands are contributed alongside it: the palette's verbs are the
    // *kernel's*, and the generator already reads those from the registry.
    let palette_keymap = crate::command_palette::default_keymap();
    iridium_config::create_if_absent(
        path,
        &[
            FaceKeys {
                title: TITLE,
                note: NOTE,
                keymap: &keymap,
                commands: &commands,
            },
            FaceKeys {
                title: PALETTE_TITLE,
                note: PALETTE_NOTE,
                keymap: &palette_keymap,
                commands: &[],
            },
        ],
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::fs;

    use iridium_file::test_support::TempDir;

    use super::create_config_if_absent;

    /// ⭐ **The reason this module exists.** The written file must name this
    /// face's own bindings, not just the kernel's and the panel's.
    ///
    /// Asserted on a `⌘` chord specifically: those live only in this layer, so
    /// a file missing them is exactly the failure this module was written to
    /// prevent, and no other source could accidentally supply one.
    #[test]
    fn the_written_file_lists_this_faces_own_mac_chords() {
        let directory = TempDir::new("desktop-config-template");
        let path = directory.path().join("config.toml");
        create_config_if_absent(&path).expect("the file was written");
        let text = fs::read_to_string(&path).expect("it is readable");

        assert!(
            text.contains("\"clipboard.copy\""),
            "the file must list the command"
        );
        let mac_copy = text
            .lines()
            .filter(|line| line.contains("\"clipboard.copy\""))
            .any(|line| line.contains("meta+"));
        assert!(
            mac_copy,
            "no line binds Copy to a `meta` chord, so ⌘C is missing from a file \
             whose header says every binding is in it"
        );
    }

    /// ⭐ **#117's half of the same rule.** The palette's keys are a layer only
    /// this face can hand over, so a file that omitted them would claim to list
    /// every binding while leaving out sixteen.
    ///
    /// Asserted on the chord *and* the command together on one line, because
    /// the command alone would also appear in the unbound reference section —
    /// which is exactly the failure this is written to catch.
    #[test]
    fn the_written_file_lists_the_palettes_own_keys() {
        let directory = TempDir::new("desktop-config-palette");
        let path = directory.path().join("config.toml");
        create_config_if_absent(&path).expect("the file was written");
        let text = fs::read_to_string(&path).expect("it is readable");

        for binding in crate::command_palette::default_keymap().bindings() {
            let id = binding
                .command()
                .expect("no default palette binding is a suppression");
            let sequence = binding.display_sequence();
            assert!(
                text.lines()
                    .any(|line| line.contains(&sequence) && line.contains(id.as_str())),
                "`{sequence}` = `{id}` is a palette binding the written file does \
                 not carry"
            );
        }
    }

    /// And every command this face registers is findable by name, whether or not
    /// it has a key. A command nobody can name is a command nobody can bind.
    #[test]
    fn the_written_file_names_every_command_this_face_registers() {
        let directory = TempDir::new("desktop-config-commands");
        let path = directory.path().join("config.toml");
        create_config_if_absent(&path).expect("the file was written");
        let text = fs::read_to_string(&path).expect("it is readable");

        for meta in super::command_metas() {
            assert!(
                text.contains(meta.id().as_str()),
                "`{}` is a command this face registers and the written file does \
                 not name it",
                meta.id()
            );
        }
    }
}
