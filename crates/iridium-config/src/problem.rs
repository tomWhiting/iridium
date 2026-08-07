//! What was wrong with the configuration file, said out loud.

use core::fmt;

/// Which part of the file a [`Problem`] was found in.
///
/// Carried separately from the message so a face can group problems, or show
/// only the ones about keys next to the keymap, without parsing prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// The file as a whole: it could not be read, or it is not valid TOML.
    ///
    /// This is the only section whose failure takes everything else with it.
    /// A file that does not parse has no sections to isolate.
    File,
    /// The `[editor]` table — one setting.
    Editor,
    /// The `[keys]` table — one binding.
    Keys,
}

impl Section {
    /// The section's name as it is written in the file.
    ///
    /// [`Self::File`] has no heading, and answers with the file's own name so
    /// a message built from this still reads as a sentence.
    const fn label(self) -> &'static str {
        match self {
            Self::File => "config.toml",
            Self::Editor => "[editor]",
            Self::Keys => "[keys]",
        }
    }
}

/// One thing the configuration file said that could not be honoured.
///
/// A problem is never fatal on its own. Every one of these is *collected*: the
/// setting or binding it names falls back to its default, everything else in
/// the file still applies, and the editor starts. The one exception is a
/// [`Section::File`] problem, which by construction means there was nothing
/// left to apply.
///
/// The reason for collecting rather than returning the first is the failure
/// this crate exists to prevent: a configuration file is edited by hand,
/// usually several lines at a time, and a loader that stops at the first
/// complaint sends the user round the loop once per mistake with no way to see
/// how many are left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Where in the file it was found.
    pub section: Section,
    /// What is wrong, and — wherever the underlying error names it — what to
    /// write instead.
    pub detail: String,
}

impl Problem {
    /// Records a problem in `section`.
    pub fn new(section: Section, detail: impl Into<String>) -> Self {
        Self {
            section,
            detail: detail.into(),
        }
    }

    /// Records a problem with the file as a whole.
    pub fn file(detail: impl Into<String>) -> Self {
        Self::new(Section::File, detail)
    }

    /// Records a problem with one setting.
    pub fn editor(detail: impl Into<String>) -> Self {
        Self::new(Section::Editor, detail)
    }

    /// Records a problem with one binding.
    pub fn keys(detail: impl Into<String>) -> Self {
        Self::new(Section::Keys, detail)
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} — {}", self.section.label(), self.detail)
    }
}

impl core::error::Error for Problem {}

#[cfg(test)]
mod tests {
    use super::{Problem, Section};

    #[test]
    fn a_problem_names_its_section_before_its_detail() {
        let problem = Problem::keys("`ctrl+q` names no command");
        assert_eq!(problem.to_string(), "[keys] — `ctrl+q` names no command");
    }

    #[test]
    fn a_file_problem_names_the_file_rather_than_an_empty_heading() {
        // There is no `[file]` table to point at, and a message beginning with
        // a bare dash reads as a bug in the message rather than a report.
        let problem = Problem::file("expected `=` after a key");
        assert_eq!(
            problem.to_string(),
            "config.toml — expected `=` after a key"
        );
    }

    #[test]
    fn the_section_survives_formatting_so_a_face_can_group_without_parsing() {
        assert_eq!(Problem::editor("x").section, Section::Editor);
    }
}
