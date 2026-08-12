//! The desktop event loop.
//!
//! This is the only part of the crate that cannot run headless: it builds the
//! winit event loop, hands it a [`DesktopApp`], and blocks until the window
//! closes. Everything it could decide without a window has been moved out —
//! the command line is parsed here but acted on by [`app`](crate::app), keys
//! are translated by [`keys`](crate::keys) — which is why it is this short.
//!
//! # The loop waits
//!
//! Control flow is `Wait`: the process sleeps until the OS has an event, and
//! frames are composed only when one asked for a repaint. There is no
//! interval to tune and no sleep to get wrong; an idle editor costs nothing.
//!
//! # Nothing is printed with `println!`
//!
//! Usage and errors are written through [`io::Write`] so that a failure to
//! write them is an outcome rather than a panic; `println!` panics on a
//! broken pipe.

use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use iridium_config::theme::ThemeChoice;
use winit::event_loop::{ControlFlow, EventLoop};

use crate::app::{DesktopApp, Options};
use crate::commands;
use crate::menubar::MenuCommand;

/// What an invocation looks like, shown for a command line that is not one.
pub const USAGE: &str = "\
usage: iridium-desktop [--theme <name|path>] [--] [file|directory]

      --theme <name|path>  `dark`, `light`, or a path to an Iridium or VS Code
                           theme file. Without it the window follows the
                           system appearance.
      --                   Stop reading options; the next argument is the path.

A directory opens as a project: the file explorer comes up rooted there, and
stays rooted there for the session. A file opens as a file. A path that does
not exist yet is a file to be written.
";

/// How the process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// Nothing went wrong.
    Success,
    /// Something did, and has already been reported on standard error.
    Failure,
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => Self::SUCCESS,
            ExitStatus::Failure => Self::FAILURE,
        }
    }
}

/// Runs the program: parses the command line, then edits until the window
/// closes.
#[must_use]
pub fn main() -> ExitStatus {
    let options = match parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(error) => return fail(&format!("iridium-desktop: {error}\n\n{USAGE}")),
    };

    // The first-run configuration file, written before the session reads one.
    //
    // An absent file is not a problem for the editor — every setting has a
    // default — but it is a problem for the person: "edit
    // `~/.config/iridium/config.toml`" is useless advice when no such
    // directory exists, which is what Tom found on 9 Aug 2026 having been
    // told exactly that. An existing file is never touched.
    //
    // ⚠️ **Here and not in [`DesktopApp::new`].** That constructor is what the
    // test suite builds sessions with, so writing from it means `cargo test`
    // creating files in the home directory of whoever ran it.
    //
    // The result is deliberately dropped. Written, already there, or
    // impossible on a read-only home all leave the session identical, because
    // the loader has always handled a missing file — so failing at a courtesy
    // must not become the thing that stops a session, and reporting it would
    // be reporting on a courtesy.
    let created =
        iridium_config::user_config_path().map(|path| commands::create_config_if_absent(&path));
    drop(created);

    let mut app = match DesktopApp::new(options) {
        Ok(app) => app,
        Err(error) => return fail(&format!("iridium-desktop: {error}\n")),
    };

    // ⭐ **`with_user_event` rather than `EventLoop::new`, for exactly one
    // reason**: the macOS menu bar. An `AppKit` menu action fires while
    // `run_app` holds the app, so it cannot touch it; the item sends the
    // command id through an [`EventLoopProxy`] instead, which both enqueues it
    // *and* wakes the loop — and with `ControlFlow::Wait` the loop is asleep
    // whenever nothing is happening, which is precisely when somebody reaches
    // for a menu.
    let event_loop = match EventLoop::<MenuCommand>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            return fail(&format!(
                "iridium-desktop: cannot start an event loop: {error}\n"
            ));
        },
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    // The proxy is made here because only an `EventLoop` can make one — an
    // `ActiveEventLoop`, which is all `resumed` gets, cannot. The app holds it
    // until `resumed` installs the bar and then hands it to the menu target.
    app.attach_menu_proxy(event_loop.create_proxy());

    match event_loop.run_app(&mut app) {
        // A failure inside the loop had no way out but the field; see
        // `DesktopApp::failure`.
        Ok(()) => {
            // The window is closed; if `IRIDIUM_LATENCY` armed the monitor,
            // the session's distribution prints now, once.
            app.report_latency();
            app.failure().map_or(ExitStatus::Success, |message| {
                fail(&format!("iridium-desktop: {message}\n"))
            })
        },
        Err(error) => fail(&format!(
            "iridium-desktop: the event loop failed: {error}\n"
        )),
    }
}

/// Parses the command line: `--theme`, and at most one file to edit.
///
/// The flag vocabulary is deliberately the terminal face's, spelling for
/// spelling — `--theme dark`, `--theme=dark`, and `--` to stop reading
/// options — because a flag that means one thing in one face and another in
/// the next is worse than a flag that exists in only one of them. What is
/// *not* copied is everything this face has no answer for: `--line`,
/// `--read-only`, `-h` and `-V` belong to a later slice, and claiming them
/// here with a stub would be a promise this program does not keep.
///
/// A second path is a mistake worth naming rather than a path worth guessing
/// about. So is an unrecognised flag: before this, `iridium-desktop --wat`
/// opened a file called `--wat`.
fn parse(mut args: impl Iterator<Item = std::ffi::OsString>) -> Result<Options, String> {
    let mut options = Options::default();
    // Set by `--`: everything after it is a path, including something that
    // looks exactly like a flag. A file really can be called `--theme`.
    let mut literal = false;

    while let Some(argument) = args.next() {
        // A non-UTF-8 argument cannot be any flag this program defines, so it
        // is a path, and it reaches `set_path` with its bytes intact.
        let text = argument.to_str().map(str::to_owned);
        match text.as_deref().filter(|_| !literal) {
            Some("--") => literal = true,
            Some("--theme") => {
                let value = args
                    .next()
                    .ok_or_else(|| "`--theme` needs a value".to_owned())?;
                options.theme = Some(ThemeChoice::parse(&value));
            },
            Some(flag) if flag.starts_with("--") && flag.contains('=') => {
                let (name, value) = flag.split_once('=').unwrap_or((flag, ""));
                if name == "--theme" {
                    options.theme = Some(ThemeChoice::parse(OsStr::new(value)));
                } else {
                    return Err(format!("unknown option `{name}`"));
                }
            },
            // `-` earns its path treatment from the convention that names
            // standard input; anything else beginning with a dash is a flag
            // this program does not define, and is named rather than opened.
            Some(flag) if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown option `{flag}`"));
            },
            _ => set_path(&mut options, argument)?,
        }
    }

    Ok(options)
}

/// Records the path to open, refusing a second one.
///
/// Whether it names a file or a directory is deliberately *not* asked here.
/// This function is pure — it is what makes the command line testable without
/// a filesystem to arrange — and the answer would be stale by the time it was
/// used anyway. [`crate::app::DesktopApp::new`] asks, at the moment it opens.
fn set_path(options: &mut Options, argument: std::ffi::OsString) -> Result<(), String> {
    if argument.is_empty() {
        return Err("the path is empty".to_owned());
    }
    if let Some(first) = options.path.as_ref() {
        return Err(format!(
            "`{}` is a second path: iridium-desktop opens one at a time (the first was `{}`)",
            argument.to_string_lossy(),
            first.display()
        ));
    }
    options.path = Some(PathBuf::from(argument));
    Ok(())
}

/// Reports an error on standard error and answers with failure.
///
/// A standard error that cannot be written to leaves nothing else to say the
/// message with, so the write's own failure is ignored: the exit code is the
/// part that must survive.
fn fail(message: &str) -> ExitStatus {
    let _ = io::stderr().write_all(message.as_bytes());
    ExitStatus::Failure
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{ThemeChoice, parse};

    #[test]
    fn no_arguments_opens_an_empty_session() {
        let options = parse(std::iter::empty()).expect("empty invocation parses");
        assert_eq!(options.path, None);
    }

    #[test]
    fn one_argument_names_the_file() {
        let options = parse([OsString::from("notes.md")].into_iter()).expect("one path parses");
        assert_eq!(
            options.path.as_deref(),
            Some(std::path::Path::new("notes.md"))
        );
    }

    #[test]
    fn extra_arguments_are_refused() {
        let error = parse([OsString::from("a.rs"), OsString::from("b.rs")].into_iter())
            .expect_err("two paths do not parse");
        assert!(
            error.contains("b.rs"),
            "the extra argument is named: {error}"
        );
    }

    /// Both spellings, because `--theme=light` and `--theme light` differing
    /// from each other is the kind of thing nothing documents and everyone
    /// trips over.
    #[test]
    fn the_theme_flag_takes_its_value_either_way() {
        for arguments in [
            vec![OsString::from("--theme"), OsString::from("light")],
            vec![OsString::from("--theme=light")],
        ] {
            let options = parse(arguments.clone().into_iter())
                .unwrap_or_else(|error| panic!("{arguments:?} parses: {error}"));
            assert_eq!(
                options.theme,
                Some(ThemeChoice::Light),
                "{arguments:?} names the light preset"
            );
            assert_eq!(options.path, None, "{arguments:?} names no file");
        }
    }

    /// The built-in names are the terminal face's, and anything else is a
    /// path — the same rule, from the same `ThemeChoice::parse`.
    #[test]
    fn a_theme_that_is_not_a_builtin_name_is_a_path() {
        let options = parse(
            [
                OsString::from("--theme"),
                OsString::from("./themes/paper.json"),
            ]
            .into_iter(),
        )
        .expect("a theme path parses");
        assert_eq!(
            options.theme,
            Some(ThemeChoice::File(PathBuf::from("./themes/paper.json")))
        );
    }

    #[test]
    fn the_theme_flag_needs_a_value() {
        let error =
            parse([OsString::from("--theme")].into_iter()).expect_err("a bare --theme is refused");
        assert!(error.contains("--theme"), "the flag is named: {error}");
    }

    /// Before this flag existed there was no flag vocabulary, so
    /// `iridium-desktop --wat` opened a file called `--wat`.
    #[test]
    fn an_unknown_flag_is_named_rather_than_opened() {
        for flag in ["--wat", "--theme-ish=x", "-x"] {
            let error = match parse([OsString::from(flag)].into_iter()) {
                Ok(options) => panic!("`{flag}` is refused, but it parsed as {options:?}"),
                Err(error) => error,
            };
            assert!(error.contains('-'), "`{flag}` is reported: {error}");
        }
    }

    /// `-` keeps its path treatment: the convention that names standard input
    /// is older than this program, and a file really can be called `-`.
    #[test]
    fn a_lone_dash_is_still_a_path() {
        let options = parse([OsString::from("-")].into_iter()).expect("`-` parses as a path");
        assert_eq!(options.path.as_deref(), Some(std::path::Path::new("-")));
    }

    /// The escape hatch, and the reason it has to exist: a file can be called
    /// exactly what a flag is called.
    #[test]
    fn the_double_dash_opens_a_file_named_like_a_flag() {
        let options = parse([OsString::from("--"), OsString::from("--theme")].into_iter())
            .expect("`-- --theme` parses");
        assert_eq!(
            options.path.as_deref(),
            Some(std::path::Path::new("--theme")),
            "after `--` the flag spelling is a file name"
        );
        assert_eq!(options.theme, None, "and it set no theme");
    }

    #[test]
    fn a_theme_and_a_file_travel_together() {
        let options = parse(
            [
                OsString::from("--theme"),
                OsString::from("dark"),
                OsString::from("notes.md"),
            ]
            .into_iter(),
        )
        .expect("a flag and a path parse");
        assert_eq!(options.theme, Some(ThemeChoice::Dark));
        assert_eq!(
            options.path.as_deref(),
            Some(std::path::Path::new("notes.md"))
        );
    }

    #[test]
    fn an_empty_file_name_is_refused() {
        let error = parse([OsString::new()].into_iter()).expect_err("an empty path is refused");
        assert!(error.contains("empty"), "the emptiness is named: {error}");
    }
}
