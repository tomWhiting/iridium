//! The `[keys]` block of the template — every binding, already written out.
//!
//! # Why the whole list is in the file
//!
//! Tom, on 13 Aug 2026, having just been handed a working `[keys]` section:
//!
//! > "You had to add the key bindings yourself and you don't know what they
//! > are. So we need to make sure that the key bindings thing has all of the
//! > key bindings available to change. You need to have them all already listed
//! > there because you couldn't possibly expect somebody to know the name of
//! > all the key bindings."
//!
//! He is right, and making a `[keys]` line *work* did nothing about it. The
//! section this replaced was a hand-written paragraph with two examples in it;
//! every real binding — the editor's, the file explorer's, the face's own, and
//! every command that ships with no key at all — could be found only by reading
//! Rust. A configuration format whose vocabulary is undiscoverable is a
//! configuration format for the person who wrote it.
//!
//! # Nothing here is transcribed, for the same reason as the settings
//!
//! [`crate::template`] generates the `[editor]` block by serializing the real
//! defaults. This does the same for the keys: the list is walked out of the
//! actual keymaps at the moment the file is written, so it cannot name a command
//! that does not exist, cannot miss one that does, and cannot show a chord that
//! has since moved.
//!
//! ⚠️ **Three sources, and one of them cannot be reached from here.** The
//! kernel's [`default_non_modal_keymap`] and the panel's
//! [`iridium_panel::explorer::default_keymap`] are both dependencies of this
//! crate. A *face's* own layer is not — a face depends on this crate, so the
//! edge cannot run the other way — and on macOS that layer is where every ⌘
//! chord lives. Leaving it out would omit the bindings a Mac user is most
//! likely to want to change, so the face hands its layer in: see [`FaceKeys`].
//!
//! # ⛔ Commented out, not live
//!
//! Every line is emitted behind a `# `. A live block would freeze today's
//! defaults into every user's file: change a default in code, and the file
//! written last year goes on pinning the old one, silently, forever.
//!
//! Commented, the file is a **menu**. The defaults stay authoritative in code,
//! and uncommenting a line means exactly "I want to change this one" — which is
//! also why each line is emitted at its *current* value rather than blank, so
//! the thing being changed is visible beside the change.
//!
//! # The left-hand side is the round-trippable spelling
//!
//! [`KeyBinding::display_sequence`] renders the form
//! [`KeyBinding::parse`](iridium_editor::KeyBinding::parse) reads back, which is
//! what makes an uncommented line *work*. It is not the prettiest form —
//! `~shift` for "shift is ignored" is the price — and the header block explains
//! it, because a prettified label would produce lines that either fail to parse
//! or, far worse, parse into a *stricter* chord than the one they replaced. That
//! second failure is silent: the binding still resolves, and simply stops
//! matching when shift is held.

use core::fmt::Write as _;
use std::collections::{BTreeMap, BTreeSet};

use iridium_editor::commands::builtin::default_registry;
use iridium_editor::commands::default_non_modal_keymap;
use iridium_editor::{CommandMeta, Keymap};

/// How far the command column is indented before its trailing comment.
///
/// A fixed column rather than one measured from the widest line: the widest is
/// an outlier, and letting it set the width pushes every other line so far right
/// that the titles fall off an eighty-column terminal. Lines wider than this
/// simply run on, which reads better than a page of padding.
const COMMENT_COLUMN: usize = 56;

/// A face's own bindings and commands, for the template to list beside the
/// kernel's.
///
/// ⭐ **Handed in rather than looked up, because it cannot be looked up.** A
/// face depends on this crate, so this crate cannot depend on a face. The
/// consequence, if this did not exist, is not a missing paragraph: on macOS the
/// desktop face's layer is where `⌘C`, `⌘S` and every other Command chord is
/// bound, so a file claiming to list every binding would omit exactly the ones a
/// Mac user reaches for first.
///
/// A face with no layer of its own passes an empty slice, which is a statement
/// rather than an omission — the parameter is not an `Option`, so a face cannot
/// leave the question unanswered by accident.
#[derive(Debug, Clone, Copy)]
pub struct FaceKeys<'a> {
    /// The heading this face's bindings appear under.
    pub title: &'a str,
    /// A sentence under the heading saying when these apply.
    pub note: &'a str,
    /// The layer the face pushes onto the kernel's default keymap.
    pub keymap: &'a Keymap,
    /// Commands the face registers that the kernel's tables do not hold.
    ///
    /// Needed for their *titles*, and so that a face command with no default key
    /// still appears in the reference at the foot of the section — a command
    /// nobody can name is a command nobody can bind.
    pub commands: &'a [CommandMeta],
}

/// The whole `[keys]` section, header and all.
///
/// Falls back to the prose header alone if the kernel's command tables cannot be
/// built — the same survivable degradation [`crate::template::default_config`]
/// applies to the settings. A first run on a stranger's machine must not fail to
/// write a file because a list could not be rendered into it.
pub fn section(faces: &[FaceKeys<'_>]) -> String {
    let mut text = String::from(HEADER);
    text.push_str("[keys]\n");

    let Ok(registry) = default_registry() else {
        text.push_str(
            "# (the built-in bindings could not be listed here — run\n\
             # \"List Every Command\" from the command palette instead)\n",
        );
        return text;
    };

    // One lookup over both vocabularies, built once. A face command is not in
    // the kernel's registry, and a line with no title beside it is a line whose
    // whole purpose — telling the reader what the command *is* — has gone.
    let mut titles: BTreeMap<&str, &str> = registry
        .commands()
        .map(|meta| (meta.id().as_str(), meta.title()))
        .collect();
    for face in faces {
        for meta in face.commands {
            titles.insert(meta.id().as_str(), meta.title());
        }
    }

    let editor = default_non_modal_keymap();
    let panel = iridium_panel::explorer::default_keymap();

    // ⚠️ Shared across every group, so a binding held by two keymaps is written
    // once. The explorer's own table binds `explorer.togglePanel` and
    // `explorer.toggleSidebar` to exactly the chords the editor binds them to —
    // deliberately, since the chord that opens the panel has to close it from
    // inside — and emitting both copies would put the same key twice in a table
    // that may hold each key once. A user uncommenting the pair would then be
    // told their own file contradicts itself, over a line this crate wrote.
    let mut written: BTreeSet<(String, String)> = BTreeSet::new();

    write_group(
        &mut text,
        "The editor",
        "Everything the document surface does. These apply wherever you are\n\
         typing.",
        &editor,
        &titles,
        &mut written,
    );
    write_group(
        &mut text,
        "The file explorer",
        EXPLORER_NOTE,
        &panel,
        &titles,
        &mut written,
    );
    for face in faces {
        write_group(
            &mut text,
            face.title,
            face.note,
            face.keymap,
            &titles,
            &mut written,
        );
    }
    write_unbound(&mut text, &titles, &written);
    text
}

/// Every `(chord, command)` pair [`section`] is obliged to write out.
///
/// The other half of the ratchet that holds the template and the keymaps in
/// agreement. A **set**, not a count: the emitted list drops the duplicates two
/// keymaps hold in common, so a count would have to encode that subtraction and
/// would then agree with the renderer by sharing its mistake. Membership does
/// not — one side walks the tables, the other reads the text that claims to
/// describe them.
///
/// Test-only, and deliberately so: this is the *assertion's* half of the pair,
/// and shipping it would offer callers a second, subtly different answer to
/// "which bindings are there" beside the keymaps themselves.
#[cfg(test)]
pub fn every_default_binding(faces: &[FaceKeys<'_>]) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    let keymaps = [
        default_non_modal_keymap(),
        iridium_panel::explorer::default_keymap(),
    ];
    let borrowed = keymaps.iter().chain(faces.iter().map(|face| face.keymap));
    for keymap in borrowed {
        for binding in keymap.bindings() {
            if let Some(id) = binding.command() {
                pairs.insert((binding.display_sequence(), id.as_str().to_owned()));
            }
        }
    }
    pairs
}

/// One heading, its note, and every binding in `keymap` grouped by namespace.
///
/// `written` carries across calls so the same chord-and-command pair is never
/// emitted twice; see the comment at its declaration in [`section`].
fn write_group(
    text: &mut String,
    title: &str,
    note: &str,
    keymap: &Keymap,
    titles: &BTreeMap<&str, &str>,
    written: &mut BTreeSet<(String, String)>,
) {
    // Grouped by the part of the id before its first dot rather than by the
    // command's category, because the namespace is the text the reader is about
    // to type: a heading and the lines under it then agree by construction.
    // `BTreeMap` and `BTreeSet` rather than sorting at the end — the ordering is
    // the whole point of the grouping, and a hash map here would emit the
    // sections in a different order on every run of the same code.
    let mut groups: BTreeMap<&str, BTreeSet<(String, &str, &str)>> = BTreeMap::new();
    let mut lines = 0_usize;
    for binding in keymap.bindings() {
        let Some(id) = binding.command() else {
            continue;
        };
        let id = id.as_str();
        let sequence = binding.display_sequence();
        if !written.insert((sequence.clone(), id.to_owned())) {
            continue;
        }
        let title = titles.get(id).copied().unwrap_or("");
        let namespace = id.split('.').next().unwrap_or(id);
        groups
            .entry(namespace)
            .or_default()
            .insert((sequence, id, title));
        lines += 1;
    }

    let _ = write!(text, "\n# ===== {title} — {lines} bindings =====\n#\n");
    for line in note.lines() {
        // A bare `#` for a blank line rather than `# `: trailing whitespace in a
        // file somebody is about to open in an editor is noise, and several
        // editors strip it on save, which would show up as a diff against a file
        // the user never edited.
        let _ = if line.trim().is_empty() {
            writeln!(text, "#")
        } else {
            writeln!(text, "# {line}")
        };
    }

    for (namespace, bindings) in groups {
        let _ = write!(text, "#\n# --- {namespace} ---\n");
        for (sequence, id, title) in bindings {
            write_binding(text, &sequence, id, title);
        }
    }
}

/// One `# "chord" = "command"` line, with the command's name beside it.
fn write_binding(text: &mut String, sequence: &str, id: &str, title: &str) {
    let line = format!("# \"{sequence}\" = \"{id}\"");
    // Padded in `char`s, not bytes: a chord is ASCII today, but padding by byte
    // length would pull a row out of line the day one is not, and the fix is
    // cheaper than the discovery.
    let width = line.chars().count();
    let padding = COMMENT_COLUMN.saturating_sub(width);
    let _ = writeln!(text, "{line}{:padding$}  # {title}", "");
}

/// Every command with no default key, as a reference rather than as a binding.
///
/// ⚠️ **Deliberately not written in `chord = id` form.** There is no chord to
/// put on the left, and a line emitted with an empty or invented one would
/// either fail to parse when uncommented or bind a key nobody chose. These are
/// listed so the *name* is findable, which is the half that was missing; the
/// note above them says what to do with it.
fn write_unbound(
    text: &mut String,
    titles: &BTreeMap<&str, &str>,
    written: &BTreeSet<(String, String)>,
) {
    let bound: BTreeSet<&str> = written.iter().map(|(_, id)| id.as_str()).collect();
    let rows: Vec<(&str, &str)> = titles
        .iter()
        .filter(|(id, _)| !bound.contains(*id))
        .map(|(id, title)| (*id, *title))
        .collect();

    let _ = write!(
        text,
        "\n# ===== Commands with no default key — {} of them =====\n\
         #\n\
         # These run from the command palette and nothing else. To give one a\n\
         # key, write a line for it in the form above — the chord you want on\n\
         # the left, the name below on the right.\n#\n",
        rows.len()
    );
    let width = rows
        .iter()
        .map(|(id, _)| id.chars().count())
        .max()
        .unwrap_or(0);
    for (id, title) in rows {
        let _ = writeln!(text, "#   {id:width$}   {title}");
    }
}

/// What stands under the file explorer's heading.
///
/// Long enough to be worth naming: it has to explain both that these keys are
/// scoped without the file saying so, and why the same chord appears more than
/// once under it.
const EXPLORER_NOTE: &str = "\
⚠️ These take effect only while the explorer panel has focus. You do not write
that anywhere — it comes from the command, which belongs to the panel.

A chord can appear more than once below: the panel browses rows, edits them as
text, and asks for confirmation, and `escape` means something different in each.
Moving one of them is a line naming that one command, and it moves only that
one. What you cannot do is write the *same* chord on two lines — this file holds
each chord once, whichever commands they were for.";

/// What stands above the list.
const HEADER: &str = "
# ---------------------------------------------------------------------------
# Key bindings: the chord on the left, the command it runs on the right.
#
# ⭐ EVERY binding Iridium ships is listed below, commented out, with the name
# of the command and what that command is called. You are not expected to know
# the names — that is what this list is for. Change a key by deleting the
# leading `# ` from its line and editing the chord; everything you leave alone
# goes on working exactly as it does now.
#
# Modifiers:  ctrl  shift  alt (= option)  meta (= cmd, super, win)
# Joined with `+`. A space separates the chords of a sequence, so
# \"cmd+k cmd+c\" means press cmd+K, then cmd+C.
#
# `~shift` means \"this binding does not care whether shift is held\". Leaving a
# modifier out entirely means the opposite — it must NOT be held — which is why
# most lines below spell out `~shift` and `~altgraph` rather than omitting
# them. Dropping a `~` from a line makes that chord stricter than the one it
# replaced, and the difference does not show up until you happen to hold shift.
#
# An empty command unbinds a chord:  \"ctrl+f\" = \"\"
# Iridium will never unbind something for you.
#
# Binding a bare chord takes away every longer sequence that starts with it.
# When that happens the chord you wrote is dropped rather than the ones it
# would have stranded, and the config.toml tab quotes the exact line to add if
# stranding them was what you wanted.
# ---------------------------------------------------------------------------

";

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::panic)]

    use std::collections::BTreeSet;

    use iridium_editor::commands::CommandCategory;
    use iridium_editor::commands::builtin::default_registry;
    use iridium_editor::input::KeyCode;
    use iridium_editor::{
        CommandId, CommandMeta, KeyBinding, Keymap, KeymapError, ModifierPattern, StrokePattern,
    };

    use super::{FaceKeys, every_default_binding, section};
    use crate::template::default_config;

    /// A face layer, for the tests that need one that is nobody else's.
    fn face_keymap() -> Keymap {
        let mut keymap = Keymap::new("a-face");
        keymap.push(KeyBinding::new(
            StrokePattern::new(KeyCode::F9, ModifierPattern::NONE),
            &[],
            CommandId::from_static("face.doTheThing"),
        ));
        keymap
    }

    /// The command that layer names, which no registry knows.
    fn face_commands() -> Vec<CommandMeta> {
        vec![CommandMeta::from_static(
            CommandId::from_static("face.doTheThing"),
            "Do The Thing",
            CommandCategory::from_static("Face"),
        )]
    }

    /// Every `# "chord" = "id"` line in `text`, as the pair it names.
    fn binding_lines(text: &str) -> Vec<(String, String)> {
        text.lines()
            .filter_map(|line| {
                let rest = line.strip_prefix("# \"")?;
                let (sequence, rest) = rest.split_once("\" = \"")?;
                let (id, _) = rest.split_once('"')?;
                Some((sequence.to_owned(), id.to_owned()))
            })
            .collect()
    }

    // ===== Completeness — the point of the task =====

    /// ⭐ **Tom's ask, as an assertion.** Every binding that ships is in the
    /// file. Set membership rather than a count, so a binding swapped for
    /// another cannot pass by keeping the total the same.
    #[test]
    fn every_default_binding_is_written_into_the_file() {
        let keymap = face_keymap();
        let commands = face_commands();
        let faces = [FaceKeys {
            title: "A face",
            note: "for the test",
            keymap: &keymap,
            commands: &commands,
        }];

        let written: BTreeSet<(String, String)> =
            binding_lines(&section(&faces)).into_iter().collect();
        for pair in every_default_binding(&faces) {
            assert!(
                written.contains(&pair),
                "`{}` = `{}` is bound by default and is not in the file that \
                 claims to list every binding",
                pair.0,
                pair.1
            );
        }
    }

    /// The other direction: the file invents nothing. A line naming a chord no
    /// keymap holds would be a chord the reader could not find in the editor.
    #[test]
    fn every_binding_line_names_a_binding_that_really_exists() {
        let defaults = every_default_binding(&[]);
        for pair in binding_lines(&section(&[])) {
            assert!(
                defaults.contains(&pair),
                "the file writes `{}` = `{}` and nothing binds it",
                pair.0,
                pair.1
            );
        }
    }

    /// **Every emitted line parses back.** The whole interface is "delete the
    /// `# `", so a line that did not round-trip through the very parser
    /// [`crate::keys`] uses would be an instruction that fails when followed.
    ///
    /// ⚠️ **What this does and does not catch, measured.** Mutating
    /// `write_binding` to strip the `~` prefixes — the prettification the module
    /// documentation warns against — leaves this test *passing*, because the
    /// prettified chord parses perfectly well and renders back to itself. It is
    /// [`every_default_binding_is_written_into_the_file`] and
    /// [`every_binding_line_names_a_binding_that_really_exists`] that fail
    /// there, by comparing against the real keymaps rather than against the
    /// file's own idea of itself. This one guards a narrower thing: output that
    /// cannot be parsed at all, or that parses into a chord other than the text
    /// on the line.
    #[test]
    fn every_emitted_line_parses_back_to_the_chord_it_names() {
        for (sequence, id) in binding_lines(&section(&[])) {
            let binding = match KeyBinding::parse(&sequence, CommandId::new(id.clone())) {
                Ok(binding) => binding,
                Err(error) => panic!("`{sequence}` = `{id}` does not parse: {error:?}"),
            };
            assert_eq!(
                binding.display_sequence(),
                sequence,
                "`{sequence}` parses to a different chord than the one written"
            );
        }
    }

    /// And every command the editor knows is somewhere in the file — bound with
    /// a chord, or named in the reference at the foot. This is the half that
    /// answers "I know what I want it to do and not what it is called".
    #[test]
    fn every_registered_command_is_named_somewhere_in_the_section() {
        let registry = default_registry().expect("the kernel tables are consistent");
        let text = section(&[]);
        for meta in registry.commands() {
            assert!(
                text.contains(meta.id().as_str()),
                "`{}` is a command and the section does not name it",
                meta.id()
            );
        }
    }

    /// A face's commands and keys reach the file too — the case the config crate
    /// cannot see for itself.
    #[test]
    fn a_faces_own_layer_is_listed() {
        let keymap = face_keymap();
        let commands = face_commands();
        let text = section(&[FaceKeys {
            title: "A face",
            note: "for the test",
            keymap: &keymap,
            commands: &commands,
        }]);

        assert!(text.contains("# ===== A face — 1 bindings ====="));
        assert!(
            text.contains("\"face.doTheThing\""),
            "the face's command is missing"
        );
        assert!(
            text.contains("Do The Thing"),
            "the face's own title is missing, so the line says nothing about \
             what the command is"
        );
    }

    // ===== The properties the file has to keep =====

    /// ⚠️ **No chord is written twice.** `[keys]` is a TOML table, so a repeated
    /// key is a duplicate the user would be told about — over two lines this
    /// crate wrote, which is the worst kind of diagnostic to receive.
    ///
    /// The explorer binds the two panel toggles to the same chords the editor
    /// does, so without deduplication this fails on four lines.
    #[test]
    fn no_chord_and_command_pair_is_written_twice() {
        let mut seen = BTreeSet::new();
        for pair in binding_lines(&section(&[])) {
            assert!(
                seen.insert(pair.clone()),
                "`{}` = `{}` is written twice",
                pair.0,
                pair.1
            );
        }
    }

    /// Every line is commented out, so the file a first run writes configures
    /// nothing. `the_template_is_valid_and_changes_nothing` asserts this through
    /// the loader; this asserts it at the source, where the failure would be one
    /// missing `# ` rather than a mystery about parsing.
    #[test]
    fn nothing_in_the_section_is_live() {
        for line in section(&[]).lines() {
            let trimmed = line.trim_start();
            assert!(
                trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "[keys]",
                "`{line}` is not commented out"
            );
        }
    }

    /// No line carries trailing whitespace. Several editors strip it on save,
    /// which would show up as a diff against a file the user never touched.
    #[test]
    fn no_line_carries_trailing_whitespace() {
        for line in default_config(&[]).lines() {
            assert_eq!(line.trim_end(), line, "`{line}` ends in whitespace");
        }
    }

    // ===== End to end =====

    /// ⭐ **Following the instruction works.** A line is taken out of the file
    /// the program writes, uncommented exactly as the header tells the reader
    /// to, and put back through the loader — and the binding that comes out
    /// names the command the line named.
    ///
    /// This is the assertion the whole task turns on: a menu whose entries do
    /// not work when chosen is worse than no menu.
    #[test]
    fn uncommenting_a_line_binds_the_command_it_names() {
        let text = default_config(&[]);
        let line = text
            .lines()
            .find(|line| line.contains("\"clipboard.copy\""))
            .expect("Copy is bound by default and must be in the file");
        let uncommented = line
            .strip_prefix("# ")
            .expect("every binding line is commented out");

        let loaded = crate::UserConfig::parse(&text.replace(line, uncommented));
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!(loaded.bindings.len(), 1, "exactly the line that was chosen");
        assert_eq!(
            loaded.bindings[0].command().map(ToString::to_string),
            Some("clipboard.copy".to_owned())
        );
    }

    /// And the layer that line produces is one the kernel accepts. Parsing is
    /// not enough: a chord the validator refuses would be reported as a problem
    /// in a file the user had followed the instructions in.
    #[test]
    fn the_layer_an_uncommented_line_produces_is_accepted_by_the_kernel() {
        let registry = default_registry().expect("the kernel tables are consistent");
        for (sequence, id) in binding_lines(&section(&[])) {
            let binding = KeyBinding::parse(&sequence, CommandId::new(id.clone()))
                .expect("every emitted line parses");
            let mut layer = crate::keys::keymap([binding]);
            match layer.canonicalize(&registry) {
                Ok(()) => {},
                Err(KeymapError::UnknownCommand { id, .. }) => {
                    panic!("the file names `{id}`, which is not a registered command")
                },
                Err(error) => panic!("`{sequence}` = `{id}` was refused: {error:?}"),
            }
        }
    }
}
