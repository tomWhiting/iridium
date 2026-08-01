//! The terminal event loop.
//!
//! This is the only part of the crate that cannot run in CI: it opens a real
//! terminal, blocks on it, and feeds what it gets to [`App`]. Everything it
//! could decide without one has been moved out — the command line is
//! [`cli`](crate::cli)'s, the editor is [`app`](crate::app)'s, and what bytes a
//! repaint is belongs to [`iridium_tui::driver`] — which is why it is this
//! short.
//!
//! # Order in the loop
//!
//! Draw, present, block. Painting *before* waiting is what makes the editor
//! feel immediate: the frame that answers a keystroke is on screen before this
//! process goes to sleep on the next one, rather than one event behind. An idle
//! editor costs nothing at all, because `termina`'s poll with no timeout blocks
//! — there is no interval to tune and no sleep to get wrong.
//!
//! # Leaving the terminal
//!
//! [`Driver`] restores everything it turned on from [`Drop`] and from the panic
//! hook, so a crash cannot leave a shell that echoes nothing. It is still
//! closed explicitly here, because `Drop` has nowhere to report a failure to
//! restore and this does. The loop's own error wins when both fail: it is what
//! went wrong first.
//!
//! # Nothing is printed with `println!`
//!
//! Help, version and errors are written through [`io::Write`] so that a failure
//! to write them is an outcome rather than a panic. `println!` panics on a
//! broken pipe, and `iridium --help | head` is a broken pipe.

use std::io::{self, IsTerminal as _, Write};
use std::process::ExitCode;

use iridium_tui::driver::{Driver, DriverEvent};

use crate::app::{App, Flow};
use crate::cli::{self, Invocation, Options, USAGE};

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

/// Runs the program: parses the command line, then edits.
#[must_use]
pub fn main() -> ExitStatus {
    match cli::parse(std::env::args_os().skip(1)) {
        Ok(Invocation::Help) => report(&mut io::stdout(), USAGE),
        Ok(Invocation::Version) => report(&mut io::stdout(), &format!("{}\n", cli::version())),
        Ok(Invocation::Edit(options)) => edit(options),
        Err(error) => fail(&format!("iridium: {error}\n\n{USAGE}")),
    }
}

/// Opens an editing session and runs it to its end.
fn edit(options: Options) -> ExitStatus {
    let mut app = match App::new(options) {
        Ok(app) => app,
        Err(error) => return fail(&format!("iridium: {error}\n")),
    };
    match session(&mut app) {
        Ok(()) => ExitStatus::Success,
        Err(error) => fail(&format!("iridium: {error}\n")),
    }
}

/// Enters the terminal, drives the editor, and leaves the terminal.
fn session(app: &mut App) -> io::Result<()> {
    let mut driver = Driver::open().map_err(|error| explain_missing_terminal(&error))?;
    let outcome = drive(app, &mut driver);
    let closed = driver.close();
    outcome.and(closed)
}

/// Turns a failure to enter the terminal into something a person can act on.
///
/// [`Driver::open`] reports the operating system's error unchanged, and by far
/// the most common one — running with output redirected, piped, or captured by
/// a tool — arrives as `Device not configured (os error 6)`. That names the
/// errno rather than the mistake, and the mistake is one sentence away from
/// being fixable.
fn explain_missing_terminal(error: &io::Error) -> io::Error {
    describe_missing_terminal(error, io::stdin().is_terminal(), io::stdout().is_terminal())
}

/// The message for a failed terminal open, given what the streams are.
///
/// Split from [`explain_missing_terminal`] so the wording can be tested: the
/// caller reads the real streams, and this decides. A test that asked the
/// process about its own standard input would be asserting on how the test
/// harness was launched, which is not a property of this program.
///
/// A driver can fail for reasons that have nothing to do with the streams — a
/// terminal that refuses raw mode, for one — so when both really are terminals
/// the operating system's error is passed through untouched rather than
/// dressed up in a guess.
fn describe_missing_terminal(
    error: &io::Error,
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
) -> io::Error {
    if stdin_is_terminal && stdout_is_terminal {
        return io::Error::new(error.kind(), error.to_string());
    }
    let which = if stdin_is_terminal {
        "standard output is not a terminal"
    } else if stdout_is_terminal {
        "standard input is not a terminal"
    } else {
        "neither standard input nor standard output is a terminal"
    };
    io::Error::new(
        error.kind(),
        format!(
            "an interactive terminal is required, and {which}. Run `iridium` \
             in a terminal rather than through a pipe, a redirect, or a tool \
             that captures output ({error})"
        ),
    )
}

/// The loop itself.
fn drive(app: &mut App, driver: &mut Driver) -> io::Result<()> {
    app.resize(driver.surface().width(), driver.surface().height());
    loop {
        let cursor = app.render(driver.surface_mut());
        driver.set_cursor(cursor);
        driver.present()?;

        match driver.next_event()? {
            DriverEvent::Input(input) => {
                if app.handle_input(&input) == Flow::Exit {
                    return Ok(());
                }
            },
            // The surface has already been resized, which invalidates it, so
            // the next frame is a full repaint whatever the editor does.
            DriverEvent::Resize { columns, rows } => app.resize(columns, rows),
            // A key the VT parser could not name. There is nothing to act on
            // and nothing changed, so the next frame writes no bytes at all.
            DriverEvent::Unrecognised => {},
        }
    }
}

/// Writes `text` to a sink, reporting whether it arrived.
fn report(sink: &mut impl Write, text: &str) -> ExitStatus {
    match sink.write_all(text.as_bytes()).and_then(|()| sink.flush()) {
        Ok(()) => ExitStatus::Success,
        // There is nowhere left to complain: the sink that failed is the one a
        // complaint would go to. The exit code is the whole of the report.
        Err(_) => ExitStatus::Failure,
    }
}

/// Writes `text` to standard error and fails.
fn fail(text: &str) -> ExitStatus {
    let _ = report(&mut io::stderr(), text);
    ExitStatus::Failure
}

#[cfg(test)]
mod tests {
    use super::{ExitStatus, describe_missing_terminal, report};
    use std::io;

    /// The error a driver open really produces when there is no terminal.
    fn enxio() -> io::Error {
        io::Error::from_raw_os_error(6)
    }

    #[test]
    fn no_terminal_at_all_says_what_to_do_instead() {
        let explained = describe_missing_terminal(&enxio(), false, false).to_string();
        assert!(
            explained.contains("neither standard input nor standard output"),
            "{explained}"
        );
        assert!(
            explained.contains("Run `iridium` in a terminal"),
            "{explained}"
        );
        // The operating system's own words are kept: they are what a bug
        // report needs, and dropping them to look tidy would cost the one
        // detail nobody can reconstruct afterwards.
        assert!(explained.contains("os error 6"), "{explained}");
    }

    #[test]
    fn a_redirected_output_names_the_stream_that_is_wrong() {
        let explained = describe_missing_terminal(&enxio(), true, false).to_string();
        assert!(
            explained.contains("standard output is not a terminal"),
            "{explained}"
        );
        assert!(!explained.contains("neither"), "{explained}");
    }

    #[test]
    fn a_piped_input_names_the_other_stream() {
        let explained = describe_missing_terminal(&enxio(), false, true).to_string();
        assert!(
            explained.contains("standard input is not a terminal"),
            "{explained}"
        );
    }

    #[test]
    fn a_real_terminal_that_fails_is_reported_verbatim() {
        // Both streams are terminals, so the terminal is not the problem and
        // this program has nothing to add. Guessing here would bury the real
        // cause under a confident and wrong explanation.
        let original = io::Error::other("the terminal refused raw mode");
        let passed_through = describe_missing_terminal(&original, true, true);
        assert_eq!(passed_through.to_string(), original.to_string());
        assert!(!passed_through.to_string().contains("interactive terminal"));
    }

    #[test]
    fn a_sink_that_cannot_be_written_is_a_failure_not_a_panic() {
        // `iridium --help | head` closes the pipe early. `println!` panics on
        // that; this must not.
        struct Broken;
        impl io::Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
        }
        assert_eq!(report(&mut Broken, "anything"), ExitStatus::Failure);
    }
}
