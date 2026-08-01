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

use std::io::{self, Write};
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
    let mut driver = Driver::open()?;
    let outcome = drive(app, &mut driver);
    let closed = driver.close();
    outcome.and(closed)
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
