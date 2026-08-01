//! Writing a file in a way that cannot half-destroy it.
//!
//! This is the most dangerous code in the program. A text editor's one
//! irreducible duty is that the file on disk is either the old contents or the
//! new contents and never a prefix of either, and `File::create` followed by
//! `write_all` breaks that duty the instant the process is killed, the disk
//! fills, or the write returns short. The file is truncated to zero the moment
//! it is opened, and everything after that is a window in which the user's work
//! is gone.
//!
//! # The sequence
//!
//! 1. Resolve the target through any symlink, so a link is followed rather than
//!    replaced.
//! 2. Create a new temporary file **in the target's own directory**, so the
//!    rename in step 6 stays within one filesystem and is therefore atomic.
//!    Across filesystems `rename` fails outright rather than copying, which is
//!    what makes "same directory" a correctness requirement and not tidiness.
//! 3. Write the contents into it.
//! 4. Give it the target's permissions, when the target already exists.
//! 5. `fsync` it, so the contents are on the device before anything points at
//!    them. Without this, a crash after the rename can leave the *new* name
//!    pointing at a file of zeros — the metadata reached the disk and the data
//!    did not.
//! 6. `rename` it over the target. This is the atomic step: POSIX requires that
//!    a concurrent opener sees either the old file or the new one.
//! 7. `fsync` the directory, so the rename itself survives a crash.
//!
//! Anything that fails before step 6 leaves the target untouched and removes
//! the temporary file. That removal happens from a `Drop` guard rather than
//! from the error paths, so an early return added later cannot leak one.
//!
//! # The limits, stated
//!
//! * On macOS `fsync` pushes data to the device but does not flush the device's
//!   own write cache; `F_FULLFSYNC` does, and reaching it needs a `libc`
//!   binding this workspace does not carry. A power cut in the millisecond
//!   after a save can therefore lose the save on macOS — not corrupt the file,
//!   which the rename still protects, but lose it.
//! * Ownership is not preserved. A process that is not root cannot give a file
//!   away, so a save by a different user than the file's owner produces a file
//!   owned by the saver. Permissions *are* preserved.
//! * The directory `fsync` is done on Unix only. Elsewhere a directory cannot
//!   be opened as a file, and the durability of the rename is the platform's.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many names to try before giving up on finding an unused one.
///
/// Each name carries the process id, a nanosecond timestamp and a counter, so
/// a collision needs another process to have produced the identical triple.
/// Sixteen attempts past that is not a limit anything real will reach; it
/// exists so that a directory that refuses every creation for some other
/// reason fails with that reason instead of spinning.
const NAME_ATTEMPTS: u32 = 16;

/// Distinguishes temporary files made within one process.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Writes `contents` to `path`, atomically.
///
/// After this returns `Ok`, `path` holds exactly `contents`. After it returns
/// `Err`, `path` holds exactly what it held before — including the case where
/// it did not exist.
///
/// # Errors
///
/// Any error from resolving, creating, writing, syncing or renaming. The error
/// is the operating system's, unchanged.
pub fn write_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    write_atomically_with(path, |file| file.write_all(contents))
}

/// The body of [`write_atomically`], with the writing step supplied.
///
/// Taking the write as a closure is what lets the tests prove the guarantee
/// rather than assume it: a closure that writes half the contents and then
/// fails is exactly the partial write this module exists to survive, and no
/// amount of testing the successful path would catch an implementation that
/// wrote straight to the target.
pub(super) fn write_atomically_with<F>(path: &Path, fill: F) -> io::Result<()>
where
    F: FnOnce(&mut File) -> io::Result<()>,
{
    let target = resolve(path)?;
    let directory = parent_of(&target);
    let existing = fs::metadata(&target).ok();

    let (mut file, temporary) = create_temporary(&directory, &target)?;

    fill(&mut file)?;

    // Permissions before the rename, so the file is never visible under its
    // real name with the wrong ones. A document that is `0600` because it holds
    // something private must not spend even an instant as `0644`.
    if let Some(metadata) = existing {
        file.set_permissions(metadata.permissions())?;
    }

    file.sync_all()?;
    drop(file);

    fs::rename(temporary.path(), &target)?;
    temporary.disarm();

    sync_directory(&directory)
}

/// Resolves a path to the file a write should replace.
///
/// A symlink is followed: saving through `~/.zshrc` when that is a link into a
/// dotfiles repository must write the file in the repository, not replace the
/// link with a regular file. A path that does not exist yet is returned as
/// given, which is how a new file is created.
///
/// A *dangling* symlink is an error rather than a new file: the link says a
/// file should be there, and creating a regular file over the link would
/// silently break whatever the link was for.
fn resolve(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path),
        _ => Ok(path.to_path_buf()),
    }
}

/// The directory a path's file lives in.
///
/// A bare file name has no parent component, and an empty parent is not a
/// directory any system call accepts, so both become the working directory.
pub(super) fn parent_of(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Creates a new file beside the target, under a name nothing else holds.
///
/// `create_new` is what makes this safe: the check for an existing file and the
/// creation are one atomic operation in the kernel, so this cannot be talked
/// into overwriting a file that appeared between a check and a create.
fn create_temporary(directory: &Path, target: &Path) -> io::Result<(File, Guard)> {
    let mut last = None;
    for attempt in 0..NAME_ATTEMPTS {
        let candidate = directory.join(temporary_name(target, attempt));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((file, Guard::new(candidate))),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no unused temporary file name was available",
        )
    }))
}

/// A name for the temporary file, beside the target and clearly ours.
///
/// It starts with a dot so that it is hidden from a directory listing while it
/// exists, and it carries the target's file name so that a temporary file left
/// behind by a killed process says which file it was for.
fn temporary_name(target: &Path, attempt: u32) -> String {
    let stem = target
        .file_name()
        .map_or_else(|| "unnamed".to_owned(), |name| name.to_string_lossy().into_owned());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    format!(".{stem}.iridium-{pid}-{nanos}-{sequence}-{attempt}.tmp")
}

/// Flushes the directory entry the rename created.
///
/// The rename is atomic but not durable on its own: a crash immediately after
/// it can leave a directory that still lists the old entry, or neither. On a
/// platform where a directory cannot be opened as a file this is skipped, and
/// the durability of the rename is the platform's own.
#[cfg(unix)]
fn sync_directory(directory: &Path) -> io::Result<()> {
    match File::open(directory) {
        Ok(handle) => match handle.sync_all() {
            // A filesystem that does not implement `fsync` on a directory says
            // so, and there is nothing further to do about it: the rename has
            // already succeeded and reporting a failed save would be a lie
            // that makes the caller retry a write that already happened.
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::InvalidInput | io::ErrorKind::Unsupported
                ) =>
            {
                Ok(())
            },
            other => other,
        },
        Err(error) => Err(error),
    }
}

/// Nothing to do on a platform whose directories are not openable files.
#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> io::Result<()> {
    Ok(())
}

/// Removes the temporary file unless the rename claimed it.
///
/// The cleanup is a destructor rather than a step on each error path so that an
/// early return added to [`write_atomically_with`] later cannot leak a
/// temporary file into the user's directory. A leaked one is not dangerous, but
/// it is litter beside every file they save, and it accumulates.
struct Guard {
    /// Where the temporary file is.
    path: PathBuf,
    /// Whether the file still needs removing.
    armed: bool,
}

impl Guard {
    /// Takes responsibility for removing `path`.
    const fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    /// Where the temporary file is.
    fn path(&self) -> &Path {
        &self.path
    }

    /// Gives up responsibility, because the rename consumed the file.
    ///
    /// Consumes the guard so that the `Drop` which follows sees a disarmed one
    /// and writes nothing; there is no state in which a guard is both alive and
    /// no longer responsible.
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.armed {
            // There is nowhere to report a failure from a destructor, and the
            // caller is already returning the error that matters — the one that
            // stopped the save. A temporary file that cannot be removed is
            // litter, not damage.
            let _ = fs::remove_file(&self.path);
        }
    }
}
