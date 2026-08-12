//! The command line.
//!
//! Hand-written rather than derived, for two reasons that matter here: a
//! dependency-free parser keeps start-up inside the plan's 50ms budget, and
//! arguments are taken as [`OsString`](std::ffi::OsString) throughout so that a
//! path this process
//! cannot decode as UTF-8 is still openable. A file name is not text — it is
//! bytes the operating system gave us — and refusing to open one because it is
//! not valid UTF-8 would be this program failing at its only job.
//!
//! # The grammar
//!
//! ```text
//! iridium [OPTIONS] [PATH]
//!
//!   -l, --line <N>     put the caret on line N, counting from one
//!   +N                 the same, in the form every terminal editor accepts
//!       --theme <T>    `dark`, `light`, or a path to a theme file
//!       --read-only    open the file without allowing edits
//!   -h, --help         print this and exit
//!   -V, --version      print the version and exit
//!       --             stop reading options; everything after is the path
//! ```
//!
//! With no path, an empty unnamed buffer is opened. Saving one asks for a name
//! rather than failing, so the no-path case is a complete path through the
//! program rather than a dead end.
//!
//! # ⭐ A file or a directory, decided where the filesystem is
//!
//! `iridium ~/project` opens the folder as a project, with the file tree
//! already up; `iridium notes.md` opens a file. **Which one it is, is not
//! decided here.** Telling them apart means asking the filesystem, and this
//! module is a pure function over its arguments — that is what lets every rule
//! above be tested with no fixture on disk, including the non-UTF-8 one that
//! cannot be written portably.
//!
//! So the path is carried whole and [`App::new`](crate::app::App::new) does the
//! one `is_dir` that decides. A path that does not exist is a **file** — a name
//! to write to — which is the existing promise and the reason a typo does not
//! silently become an empty project.
//!
//! # What is rejected
//!
//! Anything ambiguous. An unknown flag, an option with no value, a second file
//! name, a line number that is not a positive integer, and an empty path are
//! all errors that name the offending argument. A misread argument in a program
//! that writes files is worth an error message; guessing is not.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

use crate::theme::ThemeChoice;

/// The name the help text uses for this program.
const PROGRAM: &str = "iridium";

/// The usage text, printed by `--help` and after a parse error.
pub const USAGE: &str = "\
Usage: iridium [OPTIONS] [PATH]

A GPU-class editing kernel behind a terminal face.

Arguments:
  [PATH]              A file to open, or a directory to open as a project —
                      a directory comes up with the file tree already on
                      screen. A path that does not exist is a file, created
                      on save. With no path, an empty unnamed buffer is
                      opened.

Options:
  -l, --line <N>      Put the caret on line N, counting from one.
  +N                  The same, in the form every terminal editor accepts.
      --theme <T>     `dark`, `light`, or a path to a theme file. A theme file
                      may be in Iridium's own JSON format or VS Code's.
      --read-only     Open the file without allowing edits.
  -h, --help          Print this help and exit.
  -V, --version       Print the version and exit.
      --              Stop reading options; the next argument is the path.
";

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Print [`USAGE`] and exit successfully.
    Help,
    /// Print the version and exit successfully.
    Version,
    /// Open an editor with these options.
    Edit(Options),
}

/// The options an editing session starts with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// The file to open, if one was named.
    pub path: Option<PathBuf>,
    /// The line to put the caret on, counting from one.
    pub line: Option<usize>,
    /// The theme to load. `None` leaves the kernel's default in place.
    pub theme: Option<ThemeChoice>,
    /// Whether the document refuses edits.
    pub read_only: bool,
}

/// Why a command line could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// A flag this program does not define.
    UnknownFlag(String),
    /// An option that takes a value was given none.
    MissingValue(&'static str),
    /// A line number that was not a positive integer.
    InvalidLine(String),
    /// A second file name, when only one file can be opened.
    ExtraPath(PathBuf),
    /// An empty string where a path was expected.
    EmptyPath,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFlag(flag) => write!(f, "unknown option `{flag}`"),
            Self::MissingValue(option) => write!(f, "`{option}` needs a value"),
            Self::InvalidLine(value) => {
                write!(f, "`{value}` is not a line number: expected 1 or greater")
            },
            Self::ExtraPath(path) => write!(
                f,
                "`{}` is a second file: {PROGRAM} opens one at a time",
                path.display()
            ),
            Self::EmptyPath => write!(f, "the file name is empty"),
        }
    }
}

impl std::error::Error for CliError {}

/// Parses the arguments of a process, excluding the program name.
///
/// # Errors
///
/// [`CliError`] for anything ambiguous; see the module documentation.
pub fn parse<I, T>(arguments: I) -> Result<Invocation, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let mut options = Options::default();
    let mut arguments = arguments.into_iter().map(Into::into);
    // Set by `--`: everything after it is a path, including something that
    // looks exactly like a flag. A file really can be called `--theme`.
    let mut literal = false;

    while let Some(argument) = arguments.next() {
        if literal {
            set_path(&mut options, argument)?;
            continue;
        }

        // A non-UTF-8 argument cannot be any flag this program defines, so it
        // is a path, and it reaches `set_path` with its bytes intact.
        let Some(text) = argument.to_str() else {
            set_path(&mut options, argument)?;
            continue;
        };

        match text {
            "--" => literal = true,
            "-h" | "--help" => return Ok(Invocation::Help),
            "-V" | "--version" => return Ok(Invocation::Version),
            "--read-only" => options.read_only = true,
            "-l" | "--line" => {
                let value = arguments.next().ok_or(CliError::MissingValue("--line"))?;
                options.line = Some(parse_line(&value)?);
            },
            "--theme" => {
                let value = arguments.next().ok_or(CliError::MissingValue("--theme"))?;
                options.theme = Some(ThemeChoice::parse(&value));
            },
            _ => match split_inline(text) {
                Some(("--line", value)) => options.line = Some(parse_line(OsStr::new(value))?),
                Some(("--theme", value)) => {
                    options.theme = Some(ThemeChoice::parse(OsStr::new(value)));
                },
                Some((flag, _)) => return Err(CliError::UnknownFlag(flag.to_owned())),
                None if is_line_shorthand(text) => {
                    options.line = Some(parse_line(OsStr::new(&text[1..]))?);
                },
                // A lone `+` is the line shorthand with its number missing, not
                // a file name. `-` earns its path treatment from the convention
                // that names standard input; `+` has no such convention, and
                // `iridium +` is far more likely a mistyped `+12` than a file
                // literally called `+`. That file is still reachable, as
                // `iridium -- +`, which is what `--` is for.
                None if text == "+" => return Err(CliError::UnknownFlag(text.to_owned())),
                None if is_flag(text) => return Err(CliError::UnknownFlag(text.to_owned())),
                None => set_path(&mut options, argument)?,
            },
        }
    }

    Ok(Invocation::Edit(options))
}

/// Splits `--option=value` into its two halves.
///
/// Only for long options: `-l=3` is not a form this program accepts, and
/// treating it as one would make `-l=3` and `-l 3` differ from each other in a
/// way nothing documents.
fn split_inline(text: &str) -> Option<(&str, &str)> {
    if !text.starts_with("--") {
        return None;
    }
    text.split_once('=')
}

/// Whether an argument is the `+N` line shorthand.
///
/// `+` alone is not: it names no line, and reading it as one would put the
/// caret somewhere the user did not ask for. It falls through to the unknown
/// flag path, which says so.
fn is_line_shorthand(text: &str) -> bool {
    text.len() > 1 && text.starts_with('+')
}

/// Whether an argument reads as a flag rather than a path.
///
/// A lone `-` is not: it is the conventional name for standard input, and
/// while this program does not read it, calling it an unknown *option* would
/// be the wrong complaint. It is treated as a path, and opening it fails with
/// a file error that names it.
fn is_flag(text: &str) -> bool {
    text.len() > 1 && text.starts_with('-')
}

/// Records the file name, refusing a second one.
fn set_path(options: &mut Options, argument: OsString) -> Result<(), CliError> {
    if argument.is_empty() {
        return Err(CliError::EmptyPath);
    }
    let path = PathBuf::from(argument);
    if options.path.is_some() {
        return Err(CliError::ExtraPath(path));
    }
    options.path = Some(path);
    Ok(())
}

/// Parses a one-based line number.
///
/// Zero is rejected rather than clamped. A user who types `+0` has made a
/// mistake — there is no line zero in any editor's numbering — and silently
/// landing on line one would hide it.
fn parse_line(value: &OsStr) -> Result<usize, CliError> {
    let invalid = || CliError::InvalidLine(value.to_string_lossy().into_owned());
    let text = value.to_str().ok_or_else(invalid)?;
    let line: usize = text.parse().map_err(|_| invalid())?;
    if line == 0 {
        return Err(invalid());
    }
    Ok(line)
}

/// The version line printed by `--version`.
#[must_use]
pub fn version() -> String {
    format!("{PROGRAM} {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt as _;

    use super::*;

    /// The options a successful parse produced.
    fn options(arguments: &[&str]) -> Options {
        match parse(arguments.iter().map(OsString::from)) {
            Ok(Invocation::Edit(options)) => options,
            other => panic!("expected an edit invocation, got {other:?}"),
        }
    }

    /// The error a failing parse produced.
    fn error(arguments: &[&str]) -> CliError {
        match parse(arguments.iter().map(OsString::from)) {
            Err(error) => error,
            Ok(other) => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn no_arguments_opens_an_unnamed_buffer() {
        assert_eq!(options(&[]), Options::default());
    }

    #[test]
    fn a_bare_argument_is_the_file() {
        assert_eq!(options(&["notes.md"]).path, Some(PathBuf::from("notes.md")));
    }

    #[test]
    fn help_and_version_win_over_everything_after_them() {
        assert_eq!(
            parse(["--help", "--nonsense"].map(OsString::from)),
            Ok(Invocation::Help)
        );
        assert_eq!(
            parse(["-V", "--nonsense"].map(OsString::from)),
            Ok(Invocation::Version)
        );
    }

    #[test]
    fn the_line_option_takes_a_separate_or_joined_value() {
        assert_eq!(options(&["-l", "42", "a.rs"]).line, Some(42));
        assert_eq!(options(&["--line", "42", "a.rs"]).line, Some(42));
        assert_eq!(options(&["--line=42", "a.rs"]).line, Some(42));
    }

    #[test]
    fn the_plus_shorthand_is_a_line_number() {
        let parsed = options(&["+120", "a.rs"]);
        assert_eq!(parsed.line, Some(120));
        assert_eq!(parsed.path, Some(PathBuf::from("a.rs")));
    }

    #[test]
    fn a_line_number_must_be_one_or_greater() {
        // Zero is a mistake, not a synonym for one. Clamping it would put the
        // caret somewhere the user did not ask for and say nothing.
        assert_eq!(error(&["+0"]), CliError::InvalidLine("0".to_owned()));
        assert_eq!(error(&["-l", "0"]), CliError::InvalidLine("0".to_owned()));
        assert_eq!(
            error(&["-l", "twelve"]),
            CliError::InvalidLine("twelve".to_owned())
        );
        assert_eq!(error(&["-l", "-3"]), CliError::InvalidLine("-3".to_owned()));
    }

    #[test]
    fn a_lone_plus_is_not_a_line_number() {
        assert_eq!(error(&["+"]), CliError::UnknownFlag("+".to_owned()));
    }

    #[test]
    fn an_option_without_its_value_is_an_error() {
        assert_eq!(error(&["--line"]), CliError::MissingValue("--line"));
        assert_eq!(error(&["--theme"]), CliError::MissingValue("--theme"));
    }

    #[test]
    fn the_theme_option_names_a_builtin_or_a_file() {
        assert_eq!(options(&["--theme", "dark"]).theme, Some(ThemeChoice::Dark));
        assert_eq!(options(&["--theme=light"]).theme, Some(ThemeChoice::Light));
        assert_eq!(
            options(&["--theme", "./solarized.json"]).theme,
            Some(ThemeChoice::File(PathBuf::from("./solarized.json")))
        );
    }

    #[test]
    fn read_only_is_a_flag() {
        assert!(options(&["--read-only", "a.rs"]).read_only);
        assert!(!options(&["a.rs"]).read_only);
    }

    #[test]
    fn an_unknown_flag_is_refused() {
        assert_eq!(
            error(&["--colour"]),
            CliError::UnknownFlag("--colour".into())
        );
        assert_eq!(error(&["-x"]), CliError::UnknownFlag("-x".into()));
        assert_eq!(
            error(&["--theme=x", "--nope=1"]),
            CliError::UnknownFlag("--nope".into())
        );
    }

    #[test]
    fn a_second_file_is_refused() {
        // Silently ignoring the second name is how a user edits the wrong file.
        assert_eq!(
            error(&["a.rs", "b.rs"]),
            CliError::ExtraPath(PathBuf::from("b.rs"))
        );
    }

    #[test]
    fn an_empty_file_name_is_refused() {
        assert_eq!(error(&[""]), CliError::EmptyPath);
    }

    #[test]
    fn after_a_double_dash_everything_is_a_path() {
        let parsed = options(&["--", "--theme"]);
        assert_eq!(parsed.path, Some(PathBuf::from("--theme")));
        assert_eq!(parsed.theme, None);
    }

    #[test]
    fn a_lone_dash_is_a_path_not_an_option() {
        assert_eq!(options(&["-"]).path, Some(PathBuf::from("-")));
    }

    #[test]
    #[cfg(unix)]
    fn a_path_that_is_not_utf8_still_opens() {
        // The operating system gave us these bytes; refusing to open the file
        // because this process cannot decode its name would be the program
        // failing at its only job.
        let raw = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff, 0xfe]);
        let parsed = match parse([raw.clone()]) {
            Ok(Invocation::Edit(options)) => options,
            other => panic!("expected an edit invocation, got {other:?}"),
        };
        assert_eq!(parsed.path, Some(PathBuf::from(raw)));
    }

    #[test]
    fn options_may_follow_the_file() {
        let parsed = options(&["a.rs", "--read-only", "+7"]);
        assert_eq!(parsed.path, Some(PathBuf::from("a.rs")));
        assert!(parsed.read_only);
        assert_eq!(parsed.line, Some(7));
    }

    #[test]
    fn the_usage_text_documents_every_option_the_parser_accepts() {
        // The help is the only description of this grammar a user ever sees, so
        // a new option that is not in it is invisible.
        for flag in [
            "-l",
            "--line",
            "+N",
            "--theme",
            "--read-only",
            "-h",
            "--help",
            "-V",
            "--version",
            "--",
        ] {
            assert!(
                USAGE.contains(flag),
                "the usage text does not mention {flag}"
            );
        }
    }

    #[test]
    fn the_version_line_carries_the_package_version() {
        assert_eq!(version(), format!("iridium {}", env!("CARGO_PKG_VERSION")));
    }
}
