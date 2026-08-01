//! Reading and writing files, and noticing when one changed underneath us.
//!
//! # What is compared, and what that costs
//!
//! A [`TextFile`] remembers the **exact bytes** that were on disk the last time
//! it and the disk agreed — at load, and after each successful save. Before a
//! save it reads the file again and compares those bytes. Nothing else is
//! consulted for the decision.
//!
//! That is deliberately not the usual mtime-and-size stamp, and the reason is
//! that the usual stamp is wrong in the case that matters. Timestamp resolution
//! is one second on some filesystems and one *day* on FAT; a build script, a
//! formatter or a `git checkout` that rewrites a file inside the same tick
//! produces a file with the same size and the same mtime and different
//! contents, and an editor trusting the stamp overwrites it without a word.
//! Comparing the bytes cannot be wrong in that direction. Size is still checked
//! first, because a differing size settles it without reading anything, but
//! matching sizes are never taken as agreement.
//!
//! It costs one read of the file per save, and one copy of the file's bytes
//! held in memory for as long as it is open. For the files a person edits that
//! is nothing next to the write it is guarding; for a very large file it is a
//! real cost, and it is the right trade for the thing it prevents.
//!
//! What it still cannot promise:
//!
//! * **The gap between the check and the rename.** Another process can write in
//!   the microseconds between them. Closing that needs a lock the rest of the
//!   system would have to agree to take, which nothing on a Unix system does.
//! * **Anything about a file this editor never read.** A file opened as new —
//!   because nothing was at that path — is checked by the opposite rule: at
//!   save time the path must still be empty, so a file that appeared in the
//!   meantime is reported rather than overwritten.
//!
//! # The bytes are not touched on the way through
//!
//! Line endings are the kernel's business: [`Document`](iridium_editor::Document)
//! detects the file's convention on load and
//! [`LineEnding`](iridium_editor::document::LineEnding) is what every newline
//! the kernel inserts is made of. So this module normalises nothing. A CRLF
//! file stays CRLF in the rope, is displayed without its carriage returns
//! because the kernel strips them per line, and is written back byte for byte.
//! An editor that normalised on load would rewrite every line of a file in
//! which the user changed one.
//!
//! The single exception is a UTF-8 byte-order mark, which is stripped on load
//! and restored on save. It is not part of the text — leaving it in would put
//! an invisible character at the start of line one, before which the caret
//! could not go — and dropping it would silently change a file that some tools
//! require it in.

mod atomic;

#[cfg(test)]
pub(crate) mod tests;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub use atomic::write_atomically;

/// The bytes a UTF-8 byte-order mark is written as.
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// A file open for editing, and what it looked like when we last agreed with
/// the disk.
#[derive(Debug, Clone)]
pub struct TextFile {
    /// Where the file is.
    path: PathBuf,
    /// Whether the file began with a byte-order mark.
    bom: bool,
    /// The bytes on disk as of the last load or save, or `None` for a file that
    /// has never existed on disk.
    baseline: Option<Vec<u8>>,
}

/// What the disk says about a file, relative to what we last saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskState {
    /// The file holds exactly the bytes we last agreed on.
    Agrees,
    /// The file holds something else.
    Differs,
    /// The file is not there, and was.
    Vanished,
    /// A file is there, and was not.
    Appeared,
}

impl DiskState {
    /// Whether a save may go ahead without being forced.
    #[must_use]
    pub const fn permits_save(self) -> bool {
        matches!(self, Self::Agrees)
    }
}

/// Why a file operation failed.
#[derive(Debug)]
pub enum FileError {
    /// The file could not be read.
    Unreadable {
        /// The file.
        path: PathBuf,
        /// Why not.
        source: io::Error,
    },
    /// The file is not valid UTF-8.
    ///
    /// Refused rather than decoded with replacement characters. A lossy decode
    /// that is then saved has destroyed every byte it could not read, which is
    /// precisely the class of failure this program must not have.
    NotText {
        /// The file.
        path: PathBuf,
        /// The byte offset at which decoding failed.
        offset: usize,
    },
    /// The file changed on disk since it was read.
    Changed {
        /// The file.
        path: PathBuf,
        /// How it differs.
        state: DiskState,
    },
    /// The file could not be written.
    Unwritable {
        /// The file.
        path: PathBuf,
        /// Why not.
        source: io::Error,
    },
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, source } => {
                write!(f, "cannot read {}: {source}", name(path))
            },
            Self::NotText { path, offset } => write!(
                f,
                "{} is not valid UTF-8 at byte {offset}: refusing to open it rather \
                 than lose the bytes it cannot decode",
                name(path)
            ),
            Self::Changed { path, state } => match state {
                DiskState::Vanished => write!(f, "{} was deleted on disk", name(path)),
                DiskState::Appeared => write!(f, "{} was created by something else", name(path)),
                DiskState::Differs | DiskState::Agrees => {
                    write!(f, "{} changed on disk", name(path))
                },
            },
            Self::Unwritable { path, source } => {
                write!(f, "cannot write {}: {source}", name(path))
            },
        }
    }
}

impl std::error::Error for FileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unreadable { source, .. } | Self::Unwritable { source, .. } => Some(source),
            Self::NotText { .. } | Self::Changed { .. } => None,
        }
    }
}

/// A path as it should appear in a message.
fn name(path: &Path) -> String {
    path.display().to_string()
}

impl TextFile {
    /// Opens `path`, returning the file and its text.
    ///
    /// A path with nothing at it is not an error: it is a new file, and the
    /// text is empty. That is what makes `iridium notes.md` on a fresh
    /// directory do the obvious thing.
    ///
    /// # Errors
    ///
    /// [`FileError::Unreadable`] when the file exists and cannot be read — a
    /// directory, a permission denial, an I/O failure — and
    /// [`FileError::NotText`] when it is not valid UTF-8.
    pub fn open(path: &Path) -> Result<(Self, String), FileError> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok((Self::new(path), String::new()));
            },
            Err(source) => {
                return Err(FileError::Unreadable {
                    path: path.to_path_buf(),
                    source,
                });
            },
        };
        let (text, bom) = decode(&bytes).map_err(|offset| FileError::NotText {
            path: path.to_path_buf(),
            offset,
        })?;
        Ok((
            Self {
                path: path.to_path_buf(),
                bom,
                baseline: Some(bytes),
            },
            text,
        ))
    }

    /// A file that does not exist on disk yet.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            bom: false,
            baseline: None,
        }
    }

    /// Where the file is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file's name alone, for the statusline.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.path.file_name().map_or_else(
            || name(&self.path),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    /// Whether the file has never been written.
    #[must_use]
    pub const fn is_new(&self) -> bool {
        self.baseline.is_none()
    }

    /// The text this file last held, as we last saw it on disk.
    ///
    /// This is what a document is dirty *relative to*. `None` for a file that
    /// has never been on disk, whose empty buffer is nonetheless clean.
    #[must_use]
    pub fn saved_text(&self) -> Option<&str> {
        let baseline = self.baseline.as_deref()?;
        let body = baseline.strip_prefix(&BOM).unwrap_or(baseline);
        std::str::from_utf8(body).ok()
    }

    /// Compares the disk against what we last agreed on.
    ///
    /// # Errors
    ///
    /// Any error from reading the file other than its absence, which is a
    /// [`DiskState`] rather than a failure.
    pub fn disk_state(&self) -> Result<DiskState, FileError> {
        let current = match std::fs::read(&self.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(FileError::Unreadable {
                    path: self.path.clone(),
                    source,
                });
            },
        };
        Ok(match (self.baseline.as_deref(), current.as_deref()) {
            (None, None) => DiskState::Agrees,
            (None, Some(_)) => DiskState::Appeared,
            (Some(_), None) => DiskState::Vanished,
            // The length is compared first only because it settles the common
            // case without a byte-for-byte pass; equal lengths prove nothing
            // and are never taken as agreement.
            (Some(baseline), Some(current)) => {
                if baseline.len() == current.len() && baseline == current {
                    DiskState::Agrees
                } else {
                    DiskState::Differs
                }
            },
        })
    }

    /// Writes `text` to the file.
    ///
    /// Unless `force` is set, the disk is checked first and a file that changed
    /// underneath us is refused. The write itself is
    /// [`write_atomically`]: whatever happens, the file on disk is the old
    /// contents or the new contents and never a prefix of either.
    ///
    /// # Errors
    ///
    /// [`FileError::Changed`] when the file changed and the save was not
    /// forced, and [`FileError::Unwritable`] for anything the write itself
    /// refuses.
    pub fn save(&mut self, text: &str, force: bool) -> Result<(), FileError> {
        if !force {
            let state = self.disk_state()?;
            if !state.permits_save() {
                return Err(FileError::Changed {
                    path: self.path.clone(),
                    state,
                });
            }
        }

        let bytes = encode(text, self.bom);
        write_atomically(&self.path, &bytes).map_err(|source| FileError::Unwritable {
            path: self.path.clone(),
            source,
        })?;
        self.baseline = Some(bytes);
        Ok(())
    }

    /// Re-reads the file, discarding what we thought it held.
    ///
    /// # Errors
    ///
    /// As [`TextFile::open`], and [`FileError::Changed`] with
    /// [`DiskState::Vanished`] when the file is no longer there — a reload of
    /// nothing is not a reload, and quietly emptying the buffer would be the
    /// worst possible reading of the request.
    pub fn reload(&mut self) -> Result<String, FileError> {
        let bytes = std::fs::read(&self.path).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                FileError::Changed {
                    path: self.path.clone(),
                    state: DiskState::Vanished,
                }
            } else {
                FileError::Unreadable {
                    path: self.path.clone(),
                    source,
                }
            }
        })?;
        let (text, bom) = decode(&bytes).map_err(|offset| FileError::NotText {
            path: self.path.clone(),
            offset,
        })?;
        self.bom = bom;
        self.baseline = Some(bytes);
        Ok(text)
    }

    /// Points this file at a different path, as a save-as does.
    ///
    /// The baseline is dropped: nothing is known about the new path, and
    /// keeping the old file's bytes would have the next save compare the new
    /// path against the old file's contents and report a change that is not
    /// one.
    pub fn rename(&mut self, path: &Path) {
        self.path = path.to_path_buf();
        self.baseline = None;
    }
}

/// Decodes file bytes into text, reporting where they stop being UTF-8.
///
/// Returns the text and whether a byte-order mark was stripped.
///
/// # Errors
///
/// The byte offset at which decoding failed, counted in the file as it is on
/// disk so that it names a place the user can go and look at.
fn decode(bytes: &[u8]) -> Result<(String, bool), usize> {
    let bom = bytes.starts_with(&BOM);
    let body = if bom { &bytes[BOM.len()..] } else { bytes };
    match std::str::from_utf8(body) {
        Ok(text) => Ok((text.to_owned(), bom)),
        Err(error) => Err(error.valid_up_to() + if bom { BOM.len() } else { 0 }),
    }
}

/// Encodes text back into the bytes the file should hold.
fn encode(text: &str, bom: bool) -> Vec<u8> {
    if !bom {
        return text.as_bytes().to_vec();
    }
    let mut bytes = Vec::with_capacity(BOM.len() + text.len());
    bytes.extend_from_slice(&BOM);
    bytes.extend_from_slice(text.as_bytes());
    bytes
}
