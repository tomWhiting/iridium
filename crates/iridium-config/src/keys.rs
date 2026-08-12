//! The `[keys]` table — the bindings half of the file.
//!
//! ```toml
//! [keys]
//! "cmd+shift+p" = "palette.open"
//! "cmd+k cmd+c" = "edit.toggleLineComment"
//! "ctrl+k"      = ""                        # unbind, so `ctrl+k …` chords live
//! ```
//!
//! The key is the chord sequence, the value is the command it runs. Space
//! separates the chords of a multi-stroke sequence, exactly as
//! [`KeyBinding::parse_sequence`] reads it and exactly as a diagnostic quotes
//! it back — one spelling, in the file and in every message about the file.
//!
//! # What `cmd` means
//!
//! `cmd`, `meta`, `super` and `win` are one modifier, and on a Mac it is the
//! Command key. This is worth saying because the *browser* face forwards
//! Command as the kernel's `ctrl`, so the same chord means two things depending
//! on the face — and a reader who knows that will reasonably wonder which one
//! applies here.
//!
//! It never applies here: this crate is native-only, because a browser has no
//! configuration file to read. `cmd` in this file is Command on macOS and
//! Super elsewhere, with no translation anywhere in the path.
//!
//! # An empty command unbinds
//!
//! `"ctrl+k" = ""` suppresses that sequence in every lower layer rather than
//! binding it to nothing. It is here because it is the fix for the commonest
//! refusal: binding a bare chord strands every longer sequence starting with
//! it, and the diagnostic that reports the stranding quotes the sequence to
//! unbind in exactly this form.

use iridium_editor::{CommandId, KeyBinding, Keymap};
use toml::{Table, Value};

use crate::problem::Problem;

/// The heading this section is written under.
pub const SECTION: &str = "keys";

/// The name of the layer the user's bindings are pushed as.
///
/// It appears verbatim in every keymap diagnostic, so it is written the way a
/// user would want to read it rather than the way this crate is named.
pub const KEYMAP_NAME: &str = "user";

/// The value that unbinds a sequence rather than binding it.
pub const UNBIND: &str = "";

/// Reads the `[keys]` table out of a parsed document.
///
/// Bindings are returned in the order the file's keys sort, which is the order
/// `toml` yields a table in. Order matters — a later binding wins over an
/// earlier one for the same sequence — and a *stated* order is the point: the
/// alternative is a precedence that depends on hash iteration and therefore
/// differs between runs of the same file.
pub fn bindings(document: &Table, problems: &mut Vec<Problem>) -> Vec<KeyBinding> {
    let Some(value) = document.get(SECTION) else {
        return Vec::new();
    };
    let Some(table) = value.as_table() else {
        problems.push(Problem::keys(format!(
            "`{SECTION}` must be a table of bindings, not {}",
            crate::settings::type_name(value)
        )));
        return Vec::new();
    };

    let mut bindings: Vec<KeyBinding> = Vec::with_capacity(table.len());
    for (sequence, value) in table {
        match binding(sequence, value) {
            Ok(binding) => {
                if let Some(problem) = duplicate(&bindings, &binding, sequence) {
                    problems.push(problem);
                    continue;
                }
                bindings.push(binding);
            },
            Err(problem) => problems.push(problem),
        }
    }
    bindings
}

/// Collects `bindings` into the layer a face pushes.
#[must_use]
pub fn keymap(bindings: impl IntoIterator<Item = KeyBinding>) -> Keymap {
    let mut keymap = Keymap::new(KEYMAP_NAME);
    keymap.extend(bindings);
    keymap
}

/// Turns one `sequence = command` line into a binding.
fn binding(sequence: &str, value: &Value) -> Result<KeyBinding, Problem> {
    let Some(command) = value.as_str() else {
        return Err(Problem::keys(format!(
            "`{sequence}` must name a command as a string, not {}",
            crate::settings::type_name(value)
        )));
    };

    if command == UNBIND {
        let (first, rest) =
            KeyBinding::parse_sequence(sequence).map_err(|error| unparsable(sequence, &error))?;
        return Ok(KeyBinding::unbound(first, &rest));
    }

    KeyBinding::parse(sequence, CommandId::new(command.to_owned()))
        .map_err(|error| unparsable(sequence, &error))
}

/// The report for a sequence that is not a key chord.
///
/// The kernel's own error already names the offending chord, so the sequence
/// is repeated only when the sequence and the chord differ — which is the
/// multi-stroke case, and the one where knowing which line to edit is not
/// obvious from the chord alone.
fn unparsable(sequence: &str, error: &iridium_editor::KeymapError) -> Problem {
    Problem::keys(format!("`{sequence}` is not a key sequence: {error}"))
}

/// Why `candidate` cannot join `bindings`, or `None` when it can.
///
/// Two differently *written* keys can name the same sequence — `"cmd+k"` and
/// `"meta-k"` are the same chord, and TOML cannot object because they are
/// distinct table keys. Left alone, one of them silently wins and the other
/// looks like it was ignored for no reason.
fn duplicate(bindings: &[KeyBinding], candidate: &KeyBinding, sequence: &str) -> Option<Problem> {
    let written = candidate.display_sequence();
    let existing = bindings
        .iter()
        .find(|binding| binding.display_sequence() == written)?;
    let winner = existing.command().map_or_else(
        || "nothing (it is unbound)".to_owned(),
        |id| format!("`{id}`"),
    );
    Some(Problem::keys(format!(
        "`{sequence}` is the same key sequence as an earlier line in this table; \
         it is ignored and the sequence still runs {winner}"
    )))
}

#[cfg(test)]
mod tests {
    use toml::Table;

    use super::{KEYMAP_NAME, bindings, keymap};
    use crate::problem::{Problem, Section};

    /// Parses `text` and reads its `[keys]` table.
    fn read(text: &str) -> (Vec<iridium_editor::KeyBinding>, Vec<Problem>) {
        let document: Table = text.parse().expect("the fixture is valid TOML");
        let mut problems = Vec::new();
        let bindings = bindings(&document, &mut problems);
        (bindings, problems)
    }

    #[test]
    fn no_keys_table_is_not_a_problem() {
        let (bindings, problems) = read("");
        assert!(bindings.is_empty());
        assert!(problems.is_empty());
    }

    #[test]
    fn a_chord_becomes_a_binding_naming_its_command() {
        let (bindings, problems) = read("[keys]\n\"ctrl+shift+p\" = \"palette.open\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].command().map(ToString::to_string),
            Some("palette.open".to_owned())
        );
    }

    #[test]
    fn a_configuration_file_can_ask_for_a_capturing_stroke() {
        // `{char}` reaches `StrokePattern::any_char` through
        // `KeyBinding::parse`, so a `[keys]` line can bind a sequence whose
        // matched character is handed to the command as an argument. Pinned
        // here because it is the *reachable* half of `CommandArgs` from a
        // configuration file — a count prefix is not: `KeyBinding::parse`
        // builds through `new` and never sets one.
        //
        // ⚠️ Why it is worth knowing: `no_host_command_is_bound_to_a_sequence_that_carries_arguments`
        // and its two face counterparts keep argument-carrying sequences off
        // host commands in every keymap Iridium ships, and **cannot see this
        // layer**. Bound to a host command, the browser forwards the captured
        // character and the two native faces discard it. It costs nothing
        // while no host command reads its arguments; it is where to look first
        // when one does.
        let (bindings, problems) = read("[keys]\n\"ctrl+f {char}\" = \"edit.toggleLineComment\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(bindings.len(), 1);
        assert!(
            bindings[0].has_capture_stroke(),
            "`{{char}}` should have parsed to a capturing stroke"
        );
    }

    #[test]
    fn a_configuration_file_cannot_ask_for_a_count_prefix() {
        // The other half, asserted rather than assumed. There is no spelling
        // for it: `accepts_count` is only ever set by `with_count_prefix`,
        // which the parser does not call. A plain chord is the control, so
        // this fails if `parse` ever starts setting it for everything.
        let (bindings, problems) = read("[keys]\n\"ctrl+alt+j\" = \"edit.toggleLineComment\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(bindings.len(), 1);
        assert!(!bindings[0].accepts_count());
    }

    #[test]
    fn a_multi_stroke_sequence_is_read_as_one_binding() {
        let (bindings, problems) = read("[keys]\n\"ctrl+k ctrl+c\" = \"edit.toggleLineComment\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].sequence().len(), 2);
    }

    #[test]
    fn cmd_and_meta_are_one_modifier() {
        // The whole point of saying so in the module documentation: a user who
        // writes `cmd` on a Mac and a user who writes `super` on Linux have
        // written the same binding, and the file is portable between them.
        let (bindings, problems) = read("[keys]\n\"cmd+p\" = \"file.open\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(bindings[0].display_sequence(), "meta+p");
    }

    #[test]
    fn an_empty_command_unbinds_rather_than_binding_to_nothing() {
        let (bindings, problems) = read("[keys]\n\"ctrl+k\" = \"\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(bindings.len(), 1);
        assert!(
            bindings[0].command().is_none(),
            "an unbind carries no command"
        );
    }

    #[test]
    fn a_chord_that_is_not_a_chord_costs_only_its_own_line() {
        let (bindings, problems) = read(
            "[keys]\n\"ctrl+nonsuch\" = \"palette.open\"\n\"ctrl+alt+j\" = \"cursor.lineDown\"\n",
        );
        assert_eq!(bindings.len(), 1, "the good binding survived");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].section, Section::Keys);
        assert!(problems[0].detail.contains("ctrl+nonsuch"), "{problems:?}");
    }

    #[test]
    fn a_command_that_is_not_a_string_is_reported() {
        let (bindings, problems) = read("[keys]\n\"ctrl+j\" = 3\n");
        assert!(bindings.is_empty());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].detail.contains("a number"), "{problems:?}");
    }

    #[test]
    fn two_spellings_of_one_chord_are_reported_rather_than_silently_merged() {
        // TOML cannot object — they are different table keys — so nothing but
        // this check stands between the user and a binding that vanished for
        // no visible reason.
        let (bindings, problems) = read("[keys]\n\"cmd+k\" = \"a.one\"\n\"meta-k\" = \"a.two\"\n");
        assert_eq!(bindings.len(), 1, "only one binding survives");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].detail.contains("a.one"),
            "the report names the binding that is actually in effect: {problems:?}"
        );
    }

    /// ⭐ **#91, from the file's side.** A panel verb is a registered command,
    /// so a line naming one is accepted and installs — where before it was
    /// reported as a command that did not exist.
    #[test]
    fn a_binding_to_a_panel_command_validates_against_the_editors_registry() {
        use iridium_editor::commands::builtin::{builtin_registry, default_registry};

        let (bindings, problems) = read("[keys]\n\"ctrl+j\" = \"explorer.moveDown\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        let layer = keymap(bindings);
        layer
            .validate(&default_registry().expect("the kernel tables are consistent"))
            .expect("a panel command is a registered command");

        // ⚠️ The control, and the reason this test means anything: a registry
        // without the panel table genuinely does not know the id and says so.
        // Without this half, the assertion above would pass just as well if
        // `validate` had quietly stopped checking anything.
        let error = layer
            .validate(&builtin_registry().expect("the built-in table is consistent"))
            .expect_err("the built-ins alone cannot know a panel command");
        assert!(
            matches!(error, iridium_editor::KeymapError::UnknownCommand { ref id, .. }
                if id.as_str() == "explorer.moveDown"),
            "expected the panel command to be the unknown one, got {error:?}"
        );
    }

    /// ⛔ **The trap the generated `[keys]` menu would have handed out 42 times.**
    ///
    /// #91 registered the panel's vocabulary in the kernel, which is what makes
    /// `"j" = "explorer.moveDown"` validate instead of being reported as a typo.
    /// But the binding the file produces names no mode, and a mode-free binding
    /// applies in *every* mode — so before this was fixed, that line installed
    /// into the editor's own stack and `j` stopped typing in the document, while
    /// the panel verb it named still never ran there.
    ///
    /// Measured before the fix: the letter produced an empty document.
    ///
    /// It is asserted here, against a real [`Editor`] taking a real keystroke,
    /// rather than against the binding's `mode()` — the mode is the mechanism,
    /// and "the letter still types" is the thing that was actually wrong.
    #[test]
    fn a_panel_binding_from_the_file_does_not_stop_a_letter_typing() {
        use iridium_editor::{Editor, EditorConfig, KeyCode, KeyEvent, Modifiers};

        let (bindings, problems) = read("[keys]\n\"j\" = \"explorer.moveDown\"\n");
        assert!(problems.is_empty(), "{problems:?}");

        let mut editor = Editor::new(EditorConfig::default());
        editor
            .push_keymap(keymap(bindings))
            .expect("a panel command is a registered command");
        editor.handle_key(&KeyEvent::new(KeyCode::Char('j'), Modifiers::none()));
        assert_eq!(
            editor.content(),
            "j",
            "a key bound to a command that only exists inside a panel must not \
             stop typing in the document"
        );
    }

    /// The control, and the reason the assertion above means anything: the same
    /// editor with no user layer at all types the same letter. Without this, the
    /// test would pass just as well if `handle_key` had stopped inserting text.
    #[test]
    fn the_letter_types_with_no_user_bindings_at_all() {
        use iridium_editor::{Editor, EditorConfig, KeyCode, KeyEvent, Modifiers};

        let mut editor = Editor::new(EditorConfig::default());
        editor.handle_key(&KeyEvent::new(KeyCode::Char('j'), Modifiers::none()));
        assert_eq!(editor.content(), "j");
    }

    /// And the binding is not merely disarmed — it is scoped, so it still runs
    /// the panel verb on the surface that owns it. A fix that dropped the
    /// binding would pass the test above and silently lose the user's key.
    #[test]
    fn the_scoped_binding_still_names_the_mode_the_panel_resolves_in() {
        use iridium_editor::commands::builtin::default_registry;

        let (bindings, _) = read("[keys]\n\"j\" = \"explorer.moveDown\"\n");
        let mut layer = keymap(bindings);
        layer
            .canonicalize(&default_registry().expect("the kernel tables are consistent"))
            .expect("a panel command is a registered command");
        assert_eq!(
            layer.bindings()[0].mode().map(ToString::to_string),
            Some("explorer".to_owned())
        );
    }

    /// A misspelling is still a misspelling. Registering the vocabulary must not
    /// turn the diagnostic off for everything that looks like a panel verb.
    #[test]
    fn a_misspelled_panel_command_is_still_reported() {
        use iridium_editor::commands::builtin::default_registry;

        let (bindings, _) = read("[keys]\n\"ctrl+j\" = \"explorer.moveDwon\"\n");
        keymap(bindings)
            .validate(&default_registry().expect("the kernel tables are consistent"))
            .expect_err("`explorer.moveDwon` is not a command and must be reported");
    }

    #[test]
    fn a_keys_key_that_is_not_a_table_is_reported_rather_than_ignored() {
        let (bindings, problems) = read("keys = \"none\"\n");
        assert!(bindings.is_empty());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].detail.contains("a string"), "{problems:?}");
    }

    #[test]
    fn the_layer_is_named_for_the_person_who_wrote_it() {
        // The name is quoted verbatim in every keymap diagnostic.
        let (bindings, _) = read("[keys]\n\"ctrl+alt+j\" = \"cursor.lineDown\"\n");
        let layer = keymap(bindings);
        assert_eq!(layer.name(), KEYMAP_NAME);
        assert_eq!(layer.len(), 1);
    }
}
