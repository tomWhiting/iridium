//! Executing a validated [`Plan`] against the filesystem.
//!
//! [`super::plan`] decided *what*; this decides nothing and only does it. The
//! separation is what makes the dangerous half testable without a disk and
//! this half small enough to read in one sitting.
//!
//! # Nothing here may overwrite anything
//!
//! Every operation is expressed with a primitive that **fails when its target
//! exists**, rather than one that checks first and then acts:
//!
//! - a new file is `create_new`, which is atomic,
//! - a new directory is `create_dir`, which already fails on an existing path,
//! - a rename goes through [`rename_no_replace`].
//!
//! That last one matters most and is the reason this module carries any
//! platform code at all. **`std::fs::rename` silently replaces the
//! destination** — that is its documented behaviour on Unix — so the obvious
//! implementation, checking `to.exists()` and then renaming, leaves a window
//! in which a rename destroys a file. The window is small and the consequence
//! is total, which is the worst combination to accept by default.
//!
//! It also removes a check the plan could not make. `plan` routes a rename
//! cycle through a temporary name and can only avoid colliding with names it
//! knows about; a real directory may already hold one. A no-replace rename
//! turns that from something this module must remember to check into something
//! the kernel refuses.
//!
//! # A failure stops the run
//!
//! Applying continues no further once an operation fails. There is no rollback
//! and there should not be: undoing a delete is not possible, and undoing half
//! a directory rename by guessing is how a bad situation becomes an
//! unrecoverable one. What the caller gets is exactly how far it got, so the
//! panel can re-read the directory and show the truth.

use std::fs;
use std::io;
use std::path::Path;

use super::plan::{Operation, Plan};

/// An operation that could not be carried out, and how far the run got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The operation that failed, so the message can name what it was doing.
    pub operation: Operation,
    /// The operating system's complaint, kept verbatim.
    pub reason: String,
    /// How many operations completed before this one. Everything before this
    /// index in the plan happened; nothing after it did.
    pub completed: usize,
}

impl Failure {
    /// How this reads to someone who just pressed apply.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "could not {}: {} — {} operation{} before it already happened",
            self.operation.describe(),
            self.reason,
            self.completed,
            if self.completed == 1 { "" } else { "s" }
        )
    }
}

/// Carries out every operation in order, stopping at the first failure.
///
/// # Errors
///
/// Returns the operation that failed and how many completed before it.
pub fn apply(plan: &Plan) -> Result<usize, Failure> {
    for (index, operation) in plan.operations.iter().enumerate() {
        if let Err(error) = run(operation) {
            return Err(Failure {
                operation: operation.clone(),
                reason: error.to_string(),
                completed: index,
            });
        }
    }
    Ok(plan.operations.len())
}

/// One operation.
fn run(operation: &Operation) -> io::Result<()> {
    match operation {
        Operation::Create { path, directory } => {
            if *directory {
                // Already fails when the path exists, in one call.
                fs::create_dir(path)
            } else {
                // `create_new` is the atomic "make this only if it is not
                // there", and the handle is dropped immediately — the point is
                // the empty file, not writing to it.
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map(drop)
            }
        },
        Operation::Rename { from, to } => rename_no_replace(from, to),
        Operation::Delete { path } => delete(path),
    }
}

/// Removes a path, and everything under it when it is a directory.
///
/// `symlink_metadata` rather than `metadata`, so a symlink pointing at a
/// directory is removed as the link it is instead of being followed and having
/// its target's contents deleted. That distinction is the difference between
/// removing a shortcut and removing somebody's home directory.
fn delete(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// Renames `from` to `to`, failing rather than replacing an existing `to`.
///
/// See the module note for why `std::fs::rename` will not do.
#[cfg(target_os = "macos")]
#[expect(
    unsafe_code,
    reason = "renamex_np is the only way to get a non-replacing rename on \
              macOS; std::fs::rename replaces the destination silently, which \
              for this feature means destroying a file the user did not ask to \
              lose. The call takes two NUL-terminated paths and a flag, all \
              three of which are constructed here and outlive it."
)]
fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::{CString, c_char};
    use std::os::unix::ffi::OsStrExt as _;

    unsafe extern "C" {
        fn renamex_np(from: *const c_char, to: *const c_char, flags: u32) -> i32;
    }

    /// `RENAME_EXCL` from `sys/stdio.h`: fail if the destination exists.
    const RENAME_EXCL: u32 = 0x0000_0004;

    /// A path as a NUL-terminated C string, or the error saying why not.
    fn terminated(path: &Path) -> io::Result<CString> {
        CString::new(path.as_os_str().as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "the path contains a null byte")
        })
    }

    let from = terminated(from)?;
    let to = terminated(to)?;

    // SAFETY: both pointers come from `CString`s that live until the end of
    // this function, and are NUL-terminated by construction.
    let result = unsafe { renamex_np(from.as_ptr(), to.as_ptr(), RENAME_EXCL) };
    if result == 0 {
        return Ok(());
    }
    Err(io::Error::last_os_error())
}

/// Renames `from` to `to`, failing rather than replacing an existing `to`.
///
/// The portable fallback: check, then rename. **This is not atomic**, and the
/// gap between the two is a window in which another process could create the
/// destination and have it silently replaced. It is used only where no
/// no-replace primitive is available; see the module note.
#[cfg(not(target_os = "macos"))]
fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    if fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the destination already exists",
        ));
    }
    fs::rename(from, to)
}
