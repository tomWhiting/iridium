//! The `[editor]` table — the settings half of the file.
//!
//! # One bad line takes one setting
//!
//! Every key is checked on its own and applied on its own. A misspelled name,
//! or a value of the wrong type, costs exactly the line it is written on: the
//! setting falls back to its default, a [`Problem`] names it, and every other
//! setting in the table still applies.
//!
//! The alternative — deserialize the whole table and report the first failure
//! — is what makes "I changed one setting and everything reverted" a thing
//! users say, and the reverted settings are silent, so it is discovered much
//! later than it is caused.

use iridium_editor::EditorConfig;
use toml::{Table, Value};

use crate::fields::field_names;
use crate::problem::Problem;
use crate::suggest::nearest;

/// The heading this section is written under.
pub const SECTION: &str = "editor";

/// Reads the `[editor]` table out of a parsed document.
///
/// An absent table is not a problem: a configuration file that says nothing
/// about the editor is a configuration file that is happy with the defaults.
pub fn editor_settings(document: &Table, problems: &mut Vec<Problem>) -> EditorConfig {
    let Some(value) = document.get(SECTION) else {
        return EditorConfig::default();
    };
    let Some(table) = value.as_table() else {
        problems.push(Problem::editor(format!(
            "`{SECTION}` must be a table of settings, not {}",
            type_name(value)
        )));
        return EditorConfig::default();
    };

    let known = field_names::<EditorConfig>();
    let mut accepted = Table::new();
    for (key, value) in table {
        if let Some(problem) = reject(key, value, known) {
            problems.push(problem);
            continue;
        }
        accepted.insert(key.clone(), value.clone());
    }

    Value::Table(accepted)
        .try_into::<EditorConfig>()
        .unwrap_or_else(|error| {
            // Unreachable as the type stands: every surviving key deserialized
            // on its own, and the fields are independent of one another. It
            // stops being unreachable the moment a field is added whose value
            // is validated against another's, which is exactly the change that
            // would otherwise turn this into a panic on a user's config file.
            problems.push(Problem::editor(format!(
                "the settings could not be applied together ({error}); all settings are at their defaults"
            )));
            EditorConfig::default()
        })
}

/// Why `key` cannot be applied, or `None` when it can.
///
/// The value is tried *alone*, against the real type, rather than checked
/// against a description of what the type accepts. A description would be a
/// second answer to "what is a valid tab width", and the two would agree right
/// up until one of them changed.
fn reject(key: &str, value: &Value, known: Option<&'static [&'static str]>) -> Option<Problem> {
    if let Some(known) = known
        && !known.contains(&key)
    {
        let suggestion = nearest(key, known)
            .map_or_else(String::new, |name| format!(" — did you mean `{name}`?"));
        return Some(Problem::editor(format!(
            "there is no setting called `{key}`{suggestion}"
        )));
    }

    let mut alone = Table::new();
    alone.insert(key.to_owned(), value.clone());
    Value::Table(alone)
        .try_into::<EditorConfig>()
        .err()
        .map(|error| Problem::editor(format!("`{key}` was not accepted: {error}")))
}

/// What kind of thing a TOML value is, for a message that says so.
///
/// Shared with [`crate::keys`]: both sections report a value written in the
/// wrong shape, and two wordings for one condition is two things to keep in
/// step for no gain.
pub(crate) const fn type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "a string",
        // One word for both, because the distinction between them is TOML's
        // and not the reader's: somebody who wrote `tab_width = 2.5` needs to
        // know it is not a whole number, and the error from the field itself
        // is what says so.
        Value::Integer(_) | Value::Float(_) => "a number",
        Value::Boolean(_) => "a true/false value",
        Value::Datetime(_) => "a date",
        Value::Array(_) => "a list",
        Value::Table(_) => "a table",
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::EditorConfig;
    use toml::Table;

    use super::editor_settings;
    use crate::fields::field_names;
    use crate::problem::{Problem, Section};

    /// Parses `text` and reads its `[editor]` table.
    fn read(text: &str) -> (EditorConfig, Vec<Problem>) {
        let document: Table = text.parse().expect("the fixture is valid TOML");
        let mut problems = Vec::new();
        let config = editor_settings(&document, &mut problems);
        (config, problems)
    }

    /// The check that keeps [`crate::fields`] honest.
    ///
    /// `field_names` answers `None` for a type whose derive uses
    /// `#[serde(flatten)]`, and a `None` there silently switches off unknown-
    /// setting reporting for the whole file. Introducing a flatten on
    /// `EditorConfig` therefore fails here rather than quietly costing the
    /// check.
    #[test]
    fn the_settings_type_still_hands_over_its_field_names() {
        let names =
            field_names::<EditorConfig>().expect("EditorConfig must deserialize as a struct");
        assert!(names.contains(&"tab_width"), "got {names:?}");
    }

    #[test]
    fn no_editor_table_is_not_a_problem() {
        let (config, problems) = read("");
        assert_eq!(config, EditorConfig::default());
        assert!(problems.is_empty());
    }

    #[test]
    fn one_setting_applies_and_the_rest_stay_default() {
        let (config, problems) = read("[editor]\ntab_width = 2\n");
        assert_eq!(config.tab_width, 2);
        assert_eq!(
            EditorConfig {
                tab_width: 4,
                ..config
            },
            EditorConfig::default()
        );
        assert!(problems.is_empty());
    }

    /// **The rule this module exists for.** A bad value costs its own line and
    /// nothing else.
    #[test]
    fn a_value_of_the_wrong_type_costs_only_its_own_setting() {
        let (config, problems) = read("[editor]\ntab_width = \"two\"\nfont_size = 18.0\n");
        assert!(
            (config.font_size - 18.0).abs() < f32::EPSILON,
            "the good setting still applied"
        );
        assert_eq!(
            config.tab_width,
            EditorConfig::default().tab_width,
            "the bad setting fell back to its default"
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].section, Section::Editor);
        assert!(problems[0].detail.contains("tab_width"), "{problems:?}");
    }

    #[test]
    fn a_misspelled_setting_is_reported_with_the_spelling_that_works() {
        let (config, problems) = read("[editor]\ntab_widht = 2\n");
        assert_eq!(config, EditorConfig::default());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].detail.contains("did you mean `tab_width`?"),
            "{problems:?}"
        );
    }

    #[test]
    fn a_misspelled_setting_still_lets_the_correctly_spelled_ones_through() {
        let (config, problems) = read("[editor]\nshow_minimapp = false\nword_wrap = true\n");
        assert!(config.word_wrap);
        assert!(config.show_minimap, "the misspelled one never applied");
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    #[test]
    fn an_editor_key_that_is_not_a_table_is_reported_rather_than_ignored() {
        let (config, problems) = read("editor = 3\n");
        assert_eq!(config, EditorConfig::default());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].detail.contains("a number"), "{problems:?}");
    }

    #[test]
    fn an_enum_valued_setting_round_trips_through_the_file() {
        let (config, problems) = read("[editor]\nminimap_position = \"Left\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_ne!(
            config.minimap_position,
            EditorConfig::default().minimap_position
        );
    }

    #[test]
    fn an_optional_setting_can_be_given_a_value() {
        let (config, problems) = read("[editor]\nline_comment_token = \"#\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(config.line_comment_token.as_deref(), Some("#"));
    }

    #[test]
    fn an_empty_editor_table_changes_nothing() {
        let (config, problems) = read("[editor]\n");
        assert_eq!(config, EditorConfig::default());
        assert!(problems.is_empty());
    }
}
