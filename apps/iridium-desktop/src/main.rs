//! The Iridium desktop editor.
//!
//! A shell over the library beside it. Everything this program does — parsing
//! the command line, opening the file, translating keys, composing frames —
//! lives in `iridium_desktop` the library, where what needs no window is
//! testable without one; see that crate's documentation for why, and for the
//! scaffold warning that applies to this whole slice. All that is left here is
//! handing the process to [`iridium_desktop::main`] and turning what comes
//! back into an exit code.

use std::process::ExitCode;

fn main() -> ExitCode {
    iridium_desktop::main().into()
}
