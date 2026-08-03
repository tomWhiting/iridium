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

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use winit::event_loop::{ControlFlow, EventLoop};

use crate::app::{DesktopApp, Options};

/// What an invocation looks like, shown for a command line that is not one.
pub const USAGE: &str = "usage: iridium-desktop [file]\n";

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

    let mut app = match DesktopApp::new(options) {
        Ok(app) => app,
        Err(error) => return fail(&format!("iridium-desktop: {error}\n")),
    };

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            return fail(&format!(
                "iridium-desktop: cannot start an event loop: {error}\n"
            ));
        },
    };
    event_loop.set_control_flow(ControlFlow::Wait);

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

/// Parses the command line: at most one argument, the file to edit.
///
/// There are no flags in this slice, so anything after a first argument is a
/// mistake worth naming rather than a path worth guessing about.
fn parse(mut args: impl Iterator<Item = std::ffi::OsString>) -> Result<Options, String> {
    let path = args.next().map(PathBuf::from);
    if let Some(extra) = args.next() {
        return Err(format!("unexpected argument `{}`", extra.to_string_lossy()));
    }
    Ok(Options { path })
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

    use super::parse;

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
}
