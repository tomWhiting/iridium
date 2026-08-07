//! Loading a theme, through the kernel's theme module and nothing else.
//!
//! Every format decision here is [`iridium_editor::theme`]'s:
//! [`Theme::dark`](iridium_editor::Theme::dark),
//! [`Theme::light`](iridium_editor::Theme::light),
//! [`Theme::from_json`](iridium_editor::Theme::from_json) and
//! [`Theme::from_vscode_json`](iridium_editor::Theme::from_vscode_json) are the
//! whole of it. This module reads a file
//! and picks which of those four to call. It parses no colours, defines no
//! fields, and supplies no defaults of its own — a face with its own theme
//! format is a face whose themes stop working in the next one.
//!
//! # Which parser
//!
//! `--theme dark` and `--theme light` are the kernel's two built-ins and need
//! no file. Anything else is a path, and the file is offered to the native
//! parser first and the VS Code parser second.
//!
//! The order matters and is not arbitrary. The two formats are distinguishable
//! — a VS Code theme has `tokenColors` and string colours like `"#1e1e1e"`,
//! a native one has structured `{"r":…,"g":…,"b":…,"a":…}` — but only the
//! native parser is strict enough to reject the other outright. The VS Code
//! parser accepts a great deal, including a document with none of its fields,
//! because every one of them is optional in VS Code itself. Offering the
//! permissive parser first would turn a native theme with one typo into a
//! silently empty VS Code theme, which is a blank editor and no error message.
//!
//! When **both** parsers refuse the file, both complaints are reported. A
//! theme file is written by hand, and being told only that "it is not a VS Code
//! theme" when it was meant to be a native one is the least useful half of the
//! answer.

use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use iridium_editor::Theme;

/// Which theme a session was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeChoice {
    /// The kernel's built-in dark theme.
    Dark,
    /// The kernel's built-in light theme.
    Light,
    /// A theme file, in Iridium's own JSON format or VS Code's.
    File(PathBuf),
}

impl ThemeChoice {
    /// Reads a `--theme` value.
    ///
    /// `dark` and `light` name the built-ins; everything else is a path. A file
    /// really can be called `dark`, and is reachable as `./dark`, which is the
    /// same escape hatch every editor with named themes uses.
    #[must_use]
    pub fn parse(value: &OsStr) -> Self {
        match value.to_str() {
            Some("dark") => Self::Dark,
            Some("light") => Self::Light,
            _ => Self::File(PathBuf::from(value)),
        }
    }
}

/// Why a theme could not be loaded.
#[derive(Debug)]
pub enum ThemeError {
    /// The file could not be read.
    Unreadable {
        /// The file that could not be read.
        path: PathBuf,
        /// Why not.
        source: io::Error,
    },
    /// Neither parser accepted the file.
    Unparsable {
        /// The file that could not be parsed.
        path: PathBuf,
        /// What the native parser said.
        native: String,
        /// What the VS Code parser said.
        vscode: String,
    },
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, source } => {
                write!(f, "cannot read the theme `{}`: {source}", path.display())
            },
            Self::Unparsable {
                path,
                native,
                vscode,
            } => write!(
                f,
                "`{}` is neither an Iridium theme ({native}) nor a VS Code theme ({vscode})",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ThemeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unreadable { source, .. } => Some(source),
            Self::Unparsable { .. } => None,
        }
    }
}

/// Loads the theme a session asked for.
///
/// # Errors
///
/// [`ThemeError::Unreadable`] when the file cannot be read, and
/// [`ThemeError::Unparsable`] when neither parser accepts it.
pub fn load(choice: &ThemeChoice) -> Result<Theme, ThemeError> {
    match choice {
        ThemeChoice::Dark => Ok(Theme::dark()),
        ThemeChoice::Light => Ok(Theme::light()),
        ThemeChoice::File(path) => load_file(path),
    }
}

/// Loads a theme from a file, trying the native format and then VS Code's.
fn load_file(path: &Path) -> Result<Theme, ThemeError> {
    let text = std::fs::read_to_string(path).map_err(|source| ThemeError::Unreadable {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&text).map_err(|(native, vscode)| ThemeError::Unparsable {
        path: path.to_path_buf(),
        native,
        vscode,
    })
}

/// Parses theme JSON in either supported format.
///
/// # Errors
///
/// Both parsers' complaints, native first, when neither accepts the text.
pub fn parse(text: &str) -> Result<Theme, (String, String)> {
    let native = match Theme::from_json(text) {
        Ok(theme) => return Ok(theme),
        Err(error) => error.to_string(),
    };
    let vscode = match Theme::from_vscode_json(text) {
        Ok(theme) => return Ok(theme),
        Err(error) => error.to_string(),
    };
    Err((native, vscode))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::file::test_support::TempDir;

    #[test]
    fn the_builtin_names_need_no_file() {
        assert_eq!(ThemeChoice::parse(OsStr::new("dark")), ThemeChoice::Dark);
        assert_eq!(ThemeChoice::parse(OsStr::new("light")), ThemeChoice::Light);
        assert!(load(&ThemeChoice::Dark).unwrap().is_dark);
        assert!(!load(&ThemeChoice::Light).unwrap().is_dark);
    }

    #[test]
    fn anything_else_is_a_path() {
        assert_eq!(
            ThemeChoice::parse(OsStr::new("./dark")),
            ThemeChoice::File(PathBuf::from("./dark"))
        );
        assert_eq!(
            ThemeChoice::parse(&OsString::from("/themes/nord.json")),
            ThemeChoice::File(PathBuf::from("/themes/nord.json"))
        );
    }

    #[test]
    fn a_native_theme_file_loads() {
        let directory = TempDir::new("theme-native");
        let path = directory.path().join("custom.json");
        let mut source = Theme::dark();
        source.name = "Custom Dark".to_owned();
        std::fs::write(&path, source.to_json().unwrap()).unwrap();

        let loaded = load(&ThemeChoice::File(path)).unwrap();
        assert_eq!(loaded.name, "Custom Dark");
        assert_eq!(loaded, source);
    }

    #[test]
    fn a_vscode_theme_file_loads() {
        let directory = TempDir::new("theme-vscode");
        let path = directory.path().join("vs.json");
        std::fs::write(
            &path,
            r##"{
                "name": "Editor Blue",
                "type": "dark",
                "colors": { "editor.background": "#101020" },
                "tokenColors": []
            }"##,
        )
        .unwrap();

        let loaded = load(&ThemeChoice::File(path)).unwrap();
        assert_eq!(loaded.name, "Editor Blue");
        assert!(loaded.is_dark);
    }

    #[test]
    fn a_native_theme_is_not_swallowed_by_the_permissive_parser() {
        // The VS Code parser accepts almost anything, so trying it first would
        // turn a native theme into a silently empty one. This asserts the
        // order: a native theme must come back with its own colours.
        let mut source = Theme::light();
        source.name = "Paper".to_owned();
        let parsed = parse(&source.to_json().unwrap()).unwrap();
        assert_eq!(parsed, source);
        assert!(!parsed.is_dark, "a native light theme stayed light");
    }

    #[test]
    fn a_file_neither_parser_accepts_reports_both_complaints() {
        let (native, vscode) = parse("this is not json at all").unwrap_err();
        assert!(!native.is_empty(), "the native parser said nothing");
        assert!(!vscode.is_empty(), "the VS Code parser said nothing");
    }

    #[test]
    fn an_unparsable_file_names_itself_and_both_parsers() {
        let directory = TempDir::new("theme-bad");
        let path = directory.path().join("broken.json");
        std::fs::write(&path, "{ nope").unwrap();

        let message = load(&ThemeChoice::File(path)).unwrap_err().to_string();
        assert!(message.contains("broken.json"), "{message}");
        assert!(message.contains("Iridium theme"), "{message}");
        assert!(message.contains("VS Code theme"), "{message}");
    }

    #[test]
    fn a_missing_theme_file_says_so() {
        let directory = TempDir::new("theme-missing");
        let path = directory.path().join("absent.json");
        let error = load(&ThemeChoice::File(path)).unwrap_err();
        assert!(matches!(error, ThemeError::Unreadable { .. }), "{error:?}");
        assert!(error.to_string().contains("absent.json"), "{error}");
    }
}
