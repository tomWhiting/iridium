//! Turning a file into settings, bindings, and a list of what was wrong.

use std::io::ErrorKind;
use std::path::Path;

use iridium_editor::{EditorConfig, KeyBinding, Keymap};
use toml::Table;

use crate::keys;
use crate::location::user_config_path;
use crate::problem::Problem;
use crate::settings;
use crate::suggest::nearest;

/// The sections this crate understands.
///
/// Written out here rather than derived, because unlike the settings inside
/// `[editor]` these *are* this crate's own schema — the list and the code that
/// reads it are the same fact, declared next to each other, and adding a
/// section means editing both lines regardless.
const SECTIONS: &[&str] = &[settings::SECTION, keys::SECTION];

/// Everything the user's configuration file said.
///
/// Never an error. A file that could not be read, could not be parsed, or said
/// something impossible yields defaults and a populated [`Self::problems`] —
/// because the alternative is an editor that will not start until its
/// configuration file is correct, and the usual way to fix a configuration
/// file is to open it in the editor.
#[derive(Debug, Clone)]
pub struct UserConfig {
    /// The settings, with anything unusable at its default.
    pub editor: EditorConfig,
    /// The bindings, with anything unusable left out.
    pub bindings: Vec<KeyBinding>,
    /// Everything that could not be honoured, in the order it was found.
    pub problems: Vec<Problem>,
}

impl Default for UserConfig {
    /// The configuration of someone who has no configuration file.
    ///
    /// Identical to a file that exists and is empty, which is the point: there
    /// is one meaning of "unconfigured" and not two.
    fn default() -> Self {
        Self {
            editor: EditorConfig::default(),
            bindings: Vec::new(),
            problems: Vec::new(),
        }
    }
}

impl UserConfig {
    /// Reads the configuration file this process's environment names.
    ///
    /// The call a face makes at startup.
    #[must_use]
    pub fn read() -> Self {
        Self::load(user_config_path().as_deref())
    }

    /// Reads `path`, or returns defaults when there is nothing to read.
    ///
    /// **A missing file is not a problem.** Most people never write one, and
    /// an editor that complained about its absence would be complaining at
    /// everybody about nothing. A file that exists and cannot be *read* — a
    /// permission, a directory where a file was expected — is a problem,
    /// because somebody put something there on purpose.
    #[must_use]
    pub fn load(path: Option<&Path>) -> Self {
        let Some(path) = path else {
            return Self::default();
        };
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(error) if error.kind() == ErrorKind::NotFound => Self::default(),
            Err(error) => Self {
                problems: vec![Problem::file(format!(
                    "{} could not be read: {error}",
                    path.display()
                ))],
                ..Self::default()
            },
        }
    }

    /// Reads the text of a configuration file.
    ///
    /// A whole-file syntax error is the one failure that takes everything with
    /// it: there are no sections to isolate from one another until the file
    /// parses. It is reported with whatever position `toml` gives, and every
    /// setting and binding falls back to its default.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let document: Table = match text.parse() {
            Ok(document) => document,
            Err(error) => {
                return Self {
                    problems: vec![Problem::file(format!(
                        "the file is not valid TOML, so none of it was applied: {error}"
                    ))],
                    ..Self::default()
                };
            },
        };

        let mut problems = Vec::new();
        for key in document.keys() {
            if !SECTIONS.contains(&key.as_str()) {
                let suggestion = nearest(key, SECTIONS)
                    .map_or_else(String::new, |name| format!(" — did you mean `[{name}]`?"));
                problems.push(Problem::file(format!(
                    "there is no section called `{key}`{suggestion}"
                )));
            }
        }

        let editor = settings::editor_settings(&document, &mut problems);
        let bindings = keys::bindings(&document, &mut problems);
        Self {
            editor,
            bindings,
            problems,
        }
    }

    /// The bindings as the layer a face pushes.
    #[must_use]
    pub fn keymap(&self) -> Keymap {
        keys::keymap(self.bindings.iter().cloned())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use iridium_editor::EditorConfig;
    use iridium_file::test_support::TempDir;

    use super::UserConfig;
    use crate::problem::Section;

    #[test]
    fn a_whole_file_is_read_into_settings_and_bindings() {
        let config = UserConfig::parse(
            "[editor]\ntab_width = 2\nword_wrap = true\n\n[keys]\n\"ctrl+alt+j\" = \"cursor.lineDown\"\n",
        );
        assert!(config.problems.is_empty(), "{:?}", config.problems);
        assert_eq!(config.editor.tab_width, 2);
        assert!(config.editor.word_wrap);
        assert_eq!(config.bindings.len(), 1);
    }

    #[test]
    fn an_empty_file_is_the_default_configuration() {
        let config = UserConfig::parse("");
        assert_eq!(config.editor, EditorConfig::default());
        assert!(config.bindings.is_empty());
        assert!(config.problems.is_empty());
    }

    /// A syntax error is the only failure that takes the whole file, and it
    /// says so rather than silently starting with defaults.
    #[test]
    fn a_file_that_is_not_toml_is_reported_and_nothing_is_applied() {
        let config = UserConfig::parse("[editor\ntab_width = 2\n");
        assert_eq!(config.editor, EditorConfig::default());
        assert_eq!(config.problems.len(), 1, "{:?}", config.problems);
        assert_eq!(config.problems[0].section, Section::File);
    }

    /// **The rule the whole crate is arranged around.** A mistake in one
    /// section does not touch the other.
    #[test]
    fn a_bad_setting_does_not_cost_the_keybindings() {
        let config = UserConfig::parse(
            "[editor]\ntab_width = \"two\"\n\n[keys]\n\"ctrl+alt+j\" = \"cursor.lineDown\"\n",
        );
        assert_eq!(config.bindings.len(), 1, "the binding survived");
        assert_eq!(config.problems.len(), 1, "{:?}", config.problems);
        assert_eq!(config.problems[0].section, Section::Editor);
    }

    #[test]
    fn a_bad_binding_does_not_cost_the_settings() {
        let config =
            UserConfig::parse("[editor]\ntab_width = 2\n\n[keys]\n\"ctrl+nonsuch\" = \"a.one\"\n");
        assert_eq!(config.editor.tab_width, 2, "the setting survived");
        assert_eq!(config.problems.len(), 1, "{:?}", config.problems);
        assert_eq!(config.problems[0].section, Section::Keys);
    }

    #[test]
    fn an_unknown_section_is_reported_with_the_heading_that_works() {
        let config = UserConfig::parse("[editors]\ntab_width = 2\n");
        assert_eq!(config.problems.len(), 1, "{:?}", config.problems);
        assert!(
            config.problems[0]
                .detail
                .contains("did you mean `[editor]`?"),
            "{:?}",
            config.problems
        );
    }

    #[test]
    fn no_file_is_not_a_problem() {
        let directory = TempDir::new("config-absent");
        let config = UserConfig::load(Some(&directory.path().join("config.toml")));
        assert_eq!(config.editor, EditorConfig::default());
        assert!(config.problems.is_empty(), "{:?}", config.problems);
    }

    #[test]
    fn nowhere_to_look_is_not_a_problem_either() {
        let config = UserConfig::load(None);
        assert!(config.problems.is_empty(), "{:?}", config.problems);
    }

    /// Something that exists and cannot be read *is* a problem: somebody put
    /// it there deliberately, and silently ignoring it means their settings
    /// stop applying with nothing said.
    #[test]
    fn a_path_that_cannot_be_read_is_reported() {
        let directory = TempDir::new("config-unreadable");
        // A directory where a file was expected — the portable way to make a
        // read fail with something other than "not found".
        let path = directory.path().join("config.toml");
        std::fs::create_dir(&path).expect("the fixture directory was made");
        let config = UserConfig::load(Some(&path));
        assert_eq!(config.problems.len(), 1, "{:?}", config.problems);
        assert_eq!(config.problems[0].section, Section::File);
    }

    #[test]
    fn a_file_on_disk_is_read_through() {
        let directory = TempDir::new("config-present");
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "[editor]\nfont_size = 16.0\n").expect("the fixture was written");
        let config = UserConfig::load(Some(&path));
        assert!(config.problems.is_empty(), "{:?}", config.problems);
        assert!((config.editor.font_size - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_bindings_collect_into_a_layer() {
        let config = UserConfig::parse("[keys]\n\"ctrl+alt+j\" = \"cursor.lineDown\"\n");
        assert_eq!(config.keymap().len(), 1);
    }

    #[test]
    fn a_path_nobody_supplied_reads_nothing_from_disk() {
        // Guards the `None` arm against ever growing a fallback that reads the
        // process environment behind the caller's back.
        let config = UserConfig::load(Option::<&PathBuf>::None.map(PathBuf::as_path));
        assert!(config.bindings.is_empty());
    }
}
