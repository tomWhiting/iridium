//! The Iridium terminal editor.
//!
//! A shell over the library beside it. Everything this program does — parsing
//! the command line, opening and saving files, deciding what a key means,
//! filling a cell buffer — lives in `iridium` the library, where it is testable
//! with no terminal in sight; see that crate's documentation for why. All that
//! is left here is handing the process to [`iridium::main`] and turning what
//! comes back into an exit code.

use std::process::ExitCode;

fn main() -> ExitCode {
    iridium::main().into()
}
