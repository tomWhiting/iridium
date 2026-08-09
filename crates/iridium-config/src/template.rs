//! The configuration file a first run leaves behind.
//!
//! # Why a file is written at all, when an absent one is not a problem
//!
//! It is not a problem for the *editor* — every setting has a default and
//! [`crate::load`] says so at length. It is a problem for the person, and on
//! 9 Aug 2026 it was: Tom was told to edit `~/.config/iridium/config.toml`,
//! went to open it, and found no such directory. Nothing had ever written
//! one, and nothing ever would have.
//!
//! A configuration file you have to know the path of, the format of and the
//! setting names of before you can begin is a configuration file for people
//! who have already read the documentation. One that is *there*, listing every
//! setting at its real value with the whole of it commented out, is one you
//! can start from by deleting a `#`.
//!
//! # Nothing here is transcribed
//!
//! The `[editor]` block is [`EditorConfig::default()`] serialized and then
//! commented out, so it cannot name a setting that does not exist, cannot miss
//! one that does, and cannot show a default that has since changed. That is
//! the same argument [`crate::fields`] makes about the *names*, applied to the
//! values: a hand-written template would agree with the struct exactly until
//! the day someone added a field, and would then be quietly, confidently
//! wrong.
//!
//! [`every_setting_appears_in_the_template`](self::tests) holds the two in
//! agreement by asking `field_names` for the set and checking each one is in
//! the text.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;

use iridium_editor::EditorConfig;

/// Settings whose default is "unset", with an example of a value.
///
/// ⚠️ **A `None` serializes to nothing at all**, so these would be absent from
/// a template built only from the serialized defaults — present in the editor,
/// missing from the one document that is supposed to list everything. The
/// example is labelled as an example rather than dressed up as a default,
/// because it is not one.
const UNSET_BY_DEFAULT: &[(&str, &str, &str)] = &[(
    "line_comment_token",
    "\"//\"",
    "unset — only used for a language that declares no comment token of its own",
)];

/// What [`create_if_absent`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Created {
    /// The file was not there and now is.
    Wrote,
    /// A file was already there and was not touched.
    AlreadyThere,
}

/// The default configuration file, as text.
///
/// Every line that sets anything is commented out, so the file means exactly
/// what no file means. Uncommenting a line is the whole interface.
#[must_use]
pub fn default_config() -> String {
    let mut text = String::from(HEADER);

    text.push_str("[editor]\n");
    // ⚠️ Serialization of a plain struct of scalars cannot fail, but an
    // `expect` here would be a panic in a path that runs at startup on a
    // stranger's machine. An empty block is survivable — the file still
    // explains itself and `[keys]` still works — and the comment below says
    // so rather than leaving a reader wondering where the settings went.
    match toml::to_string(&EditorConfig::default()) {
        Ok(defaults) => {
            for line in defaults.lines().filter(|line| !line.trim().is_empty()) {
                text.push_str("# ");
                text.push_str(line);
                text.push('\n');
            }
        },
        Err(error) => {
            text.push_str("# (the built-in defaults could not be written out here: ");
            text.push_str(&error.to_string());
            text.push_str(")\n");
        },
    }
    for (name, example, note) in UNSET_BY_DEFAULT {
        text.push_str("#\n# ");
        text.push_str(name);
        text.push_str(": ");
        text.push_str(note);
        text.push_str("\n#   e.g.  ");
        text.push_str(name);
        text.push_str(" = ");
        text.push_str(example);
        text.push('\n');
    }

    text.push_str(KEYS);
    text
}

/// Writes [`default_config`] to `path` when nothing is there.
///
/// # Errors
///
/// Returns the operating system's error when the directory cannot be made or
/// the file cannot be written.
pub fn create_if_absent(path: &Path) -> io::Result<Created> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    // ⚠️ `create_new`, not `exists()` then `write()`. The check and the write
    // must be one operation: this runs at startup, and two sessions opened
    // together would otherwise both see no file and the second would overwrite
    // whatever the first had just put there. The kernel answers the question
    // and does the thing at once, so there is no window between them.
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            file.write_all(default_config().as_bytes())?;
            Ok(Created::Wrote)
        },
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(Created::AlreadyThere),
        Err(error) => Err(error),
    }
}

/// What stands above the settings.
const HEADER: &str = "\
# Iridium configuration.
#
# You are not required to have this file — it was written on a first run so
# there is something to open. Everything below is COMMENTED OUT and shows the
# value Iridium uses when you say nothing, so deleting this file and keeping
# it exactly as it is mean the same thing. Change a setting by removing its
# leading `# ` and editing the value.
#
# Apply changes without restarting: press ctrl+alt+R (or cmd+alt+R), or run
# \"Reload Configuration\" from the command palette. Bindings you delete from
# this file stop working on reload, so this file is the whole truth about
# your keys and not a pile of additions.
#
# A mistake costs its own line and nothing else. A tab called config.toml
# opens listing every problem, and everything that was working keeps working.

";

/// What stands below them.
const KEYS: &str = "
# ---------------------------------------------------------------------------
# Key bindings: the chord on the left, the command id on the right.
#
# Run \"List Every Command\" from the command palette (cmd+K or ctrl+K) for
# every id, its title, and the key that runs it TODAY — that list is rendered
# from the running editor, so it is never out of date.
#
# Modifiers:  ctrl  shift  alt (= option)  meta (= cmd, super, win)
# Joined with `+`. A space separates the chords of a sequence, so
# \"cmd+k cmd+c\" means press cmd+K, then cmd+C.
#
# An empty command unbinds a chord:  \"ctrl+f\" = \"\"
# Iridium will never unbind something for you.
# ---------------------------------------------------------------------------

[keys]
# \"cmd+shift+p\" = \"palette.open\"
# \"cmd+k cmd+c\" = \"comment.toggleLine\"
";

#[cfg(test)]
mod tests {
    use std::fs;

    use iridium_editor::EditorConfig;
    use iridium_file::test_support::TempDir;

    use super::{Created, UNSET_BY_DEFAULT, create_if_absent, default_config};
    use crate::fields::field_names;
    use crate::load::UserConfig;

    #[test]
    fn every_setting_appears_in_the_template() {
        // The anti-drift check, and the reason the template is generated
        // rather than typed. A field added to `EditorConfig` and not to the
        // serialized output — an `Option` whose `None` writes nothing is the
        // way that happens — fails here rather than going missing from the one
        // document that claims to list everything.
        let names = field_names::<EditorConfig>().expect("EditorConfig names its fields");
        let text = default_config();
        for name in names {
            assert!(
                text.contains(name),
                "{name} is a setting and is not in the template a first run writes"
            );
        }
    }

    #[test]
    fn nothing_on_the_unset_list_has_a_default_after_all() {
        // The staleness guard on the hand-written half. A field that gained a
        // real default would be listed twice — once as a value, once as an
        // "unset" example contradicting it — and the example is the copy that
        // would be wrong.
        let defaults = toml::to_string(&EditorConfig::default()).expect("the defaults serialize");
        for (name, _, _) in UNSET_BY_DEFAULT {
            assert!(
                !defaults.contains(&format!("{name} =")),
                "{name} has a default now — take it off UNSET_BY_DEFAULT"
            );
        }
    }

    #[test]
    fn the_template_is_valid_and_changes_nothing() {
        // Two claims in one, and the second is the one that matters: the file
        // parses, AND it means what an absent file means. A template with a
        // line accidentally left uncommented would still parse.
        let directory = TempDir::new("config-template");
        let path = directory.path().join("config.toml");
        fs::write(&path, default_config()).expect("the template was written");

        let loaded = UserConfig::load(Some(&path));
        assert!(
            loaded.problems.is_empty(),
            "the file we write must not be a file we complain about: {:?}",
            loaded.problems
        );
        assert_eq!(
            loaded.editor,
            EditorConfig::default(),
            "every line is commented out, so it must configure nothing"
        );
        assert!(
            loaded.bindings.is_empty(),
            "and bind nothing: {:?}",
            loaded.bindings
        );
    }

    #[test]
    fn a_file_that_is_already_there_is_never_touched() {
        // ⚠️ The one thing this must never do. A first-run helper that
        // overwrites is a first-run helper that eats a configuration someone
        // spent an evening on.
        let directory = TempDir::new("config-existing");
        let path = directory.path().join("config.toml");
        fs::write(&path, "[editor]\ntab_width = 2\n").expect("the fixture was written");

        assert_eq!(
            create_if_absent(&path).expect("the check ran"),
            Created::AlreadyThere
        );
        assert_eq!(
            fs::read_to_string(&path).expect("it is still readable"),
            "[editor]\ntab_width = 2\n",
            "byte for byte"
        );
    }

    #[test]
    fn the_first_run_writes_it_and_the_second_leaves_it_alone() {
        let directory = TempDir::new("config-first-run");
        // Two levels deep, because `~/.config/iridium/` is itself usually
        // absent on a machine that has never run this.
        let path = directory.path().join("nested/iridium/config.toml");

        assert_eq!(
            create_if_absent(&path).expect("the first run wrote it"),
            Created::Wrote
        );
        assert!(path.exists(), "the directories were made too");
        assert_eq!(
            create_if_absent(&path).expect("the second run ran"),
            Created::AlreadyThere
        );
    }

    #[test]
    fn a_directory_that_cannot_be_made_is_an_error_and_not_a_panic() {
        // A read-only home, a path through a file, a full disk. Startup must
        // survive all of it: the editor works without this file, so failing to
        // write it can never be the thing that stops the session.
        let directory = TempDir::new("config-blocked");
        let blocker = directory.path().join("not-a-directory");
        fs::write(&blocker, "x").expect("the fixture was written");

        let error = create_if_absent(&blocker.join("iridium/config.toml"))
            .expect_err("a file cannot hold a directory");
        assert!(!error.to_string().is_empty(), "and it says what went wrong");
    }
}
