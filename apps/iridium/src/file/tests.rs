//! Tests for reading, writing and change detection.
//!
//! The atomicity tests are the point of this file. A test that only saves a
//! file and reads it back passes whether or not the write was atomic, and is
//! therefore worth nothing: it would pass against `fs::write`, which truncates
//! the user's file before it writes a byte. The tests here inject a failure
//! *during* the write, from inside the closure
//! [`write_atomically_with`](super::atomic::write_atomically_with) takes, and
//! assert on the state of the directory at that exact moment — while the
//! temporary file exists and the target has not been touched.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::atomic::{parent_of, write_atomically_with};
use super::{DiskState, FileError, TextFile, decode, encode, write_atomically};

/// Distinguishes directories made by different tests in one run.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Whether a file name is one [`write_atomically`] would have left behind.
///
/// Asked through [`Path::extension`] rather than `ends_with(".tmp")` so that a
/// name which is *only* `.tmp` — a dotfile with no extension at all — does not
/// count, and so that the comparison does not quietly depend on case.
fn has_temporary_extension(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tmp"))
}

/// A directory that removes itself.
///
/// Written here rather than taken from `tempfile` because this workspace does
/// not carry that dependency and the whole of it is nine lines.
pub struct TempDir {
    /// Where it is.
    path: PathBuf,
}

impl TempDir {
    /// Creates an empty directory named after the test using it.
    pub fn new(label: &str) -> Self {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("iridium-{}-{label}-{sequence}", std::process::id()));
        fs::create_dir_all(&path).expect("the test directory could be created");
        Self { path }
    }

    /// Where it is.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Every entry in a directory, sorted, as strings.
fn entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("the directory could be listed")
        .map(|entry| {
            entry
                .expect("the entry could be read")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

// ===== Decoding and encoding =====

#[test]
fn text_survives_a_round_trip_byte_for_byte() {
    // Line endings are the kernel's to model, so nothing here rewrites them.
    // A file that came back with different line endings would be one this
    // editor had silently rewritten every line of.
    for original in [
        b"".as_slice(),
        b"one line, no newline",
        b"unix\nlines\n",
        b"windows\r\nlines\r\n",
        b"mixed\r\nlines\nand\rmore",
        b"trailing blank line\n\n",
        "unicode: \u{4f60}\u{597d} \u{1f600}\n".as_bytes(),
    ] {
        let (text, bom) = decode(original).expect("valid UTF-8");
        assert_eq!(
            encode(&text, bom),
            original,
            "round trip changed {original:?}"
        );
    }
}

#[test]
fn a_byte_order_mark_is_stripped_and_restored() {
    let original = b"\xef\xbb\xbffn main() {}\n";
    let (text, bom) = decode(original).expect("valid UTF-8");
    assert!(bom, "the mark was not noticed");
    assert_eq!(text, "fn main() {}\n", "the mark stayed in the text");
    assert_eq!(encode(&text, bom), original, "the mark was not restored");
}

#[test]
fn text_without_a_mark_does_not_gain_one() {
    let (text, bom) = decode(b"plain\n").expect("valid UTF-8");
    assert!(!bom);
    assert_eq!(encode(&text, bom), b"plain\n");
}

#[test]
fn invalid_utf8_is_refused_with_the_offset() {
    // Byte 4 is where the sequence goes wrong; the offset must count in the
    // file as it is on disk so it names somewhere the user can look.
    assert_eq!(decode(b"good\xffbad"), Err(4));
    assert_eq!(decode(b"\xef\xbb\xbfgood\xffbad"), Err(7));
}

// ===== Opening =====

#[test]
fn opening_a_missing_file_is_a_new_file() {
    let directory = TempDir::new("open-missing");
    let path = directory.path().join("notes.md");
    let (file, text) = TextFile::open(&path).expect("a missing file is not an error");
    assert!(file.is_new());
    assert_eq!(text, "");
    assert_eq!(file.saved_text(), None);
    assert!(
        entries(directory.path()).is_empty(),
        "opening created a file"
    );
}

#[test]
fn opening_reads_the_text_and_records_the_baseline() {
    let directory = TempDir::new("open-existing");
    let path = directory.path().join("a.rs");
    fs::write(&path, "fn main() {}\n").unwrap();

    let (file, text) = TextFile::open(&path).expect("the file opened");
    assert!(!file.is_new());
    assert_eq!(text, "fn main() {}\n");
    assert_eq!(file.saved_text(), Some("fn main() {}\n"));
    assert_eq!(file.display_name(), "a.rs");
}

#[test]
fn opening_a_directory_is_an_error() {
    let directory = TempDir::new("open-directory");
    let error = TextFile::open(directory.path()).expect_err("a directory is not a file");
    assert!(matches!(error, FileError::Unreadable { .. }), "{error:?}");
}

#[test]
fn opening_a_binary_file_is_refused_rather_than_mangled() {
    // A lossy decode that is then saved has destroyed every byte it could not
    // read. Refusing is the only answer that cannot lose data.
    let directory = TempDir::new("open-binary");
    let path = directory.path().join("image.png");
    fs::write(&path, b"\x89PNG\r\n\x1a\n\xff\xfe").unwrap();

    let error = TextFile::open(&path).expect_err("binary is not text");
    assert!(matches!(error, FileError::NotText { .. }), "{error:?}");
    assert!(error.to_string().contains("image.png"), "{error}");
}

// ===== Change detection =====

#[test]
fn an_untouched_file_agrees() {
    let directory = TempDir::new("state-agrees");
    let path = directory.path().join("a.txt");
    fs::write(&path, "hello").unwrap();
    let (file, _) = TextFile::open(&path).unwrap();
    assert_eq!(file.disk_state().unwrap(), DiskState::Agrees);
}

#[test]
fn a_rewrite_with_identical_bytes_still_agrees() {
    // This is what proves the decision is not made on a timestamp: rewriting
    // the file moves its mtime, and the contents are what did not change.
    let directory = TempDir::new("state-touch");
    let path = directory.path().join("a.txt");
    fs::write(&path, "hello").unwrap();
    let (mut file, _) = TextFile::open(&path).unwrap();

    fs::write(&path, "hello").unwrap();
    assert_eq!(file.disk_state().unwrap(), DiskState::Agrees);
    file.save("hello there", false)
        .expect("an unchanged file may be saved over");
}

#[test]
fn a_change_of_the_same_length_is_still_a_change() {
    // The case a size-and-mtime stamp gets wrong. A formatter that swaps two
    // characters within one filesystem tick produces exactly this, and an
    // editor that trusted the stamp would overwrite it silently.
    let directory = TempDir::new("state-same-length");
    let path = directory.path().join("a.txt");
    fs::write(&path, "aaaa").unwrap();
    let (mut file, _) = TextFile::open(&path).unwrap();

    fs::write(&path, "aaab").unwrap();
    assert_eq!(file.disk_state().unwrap(), DiskState::Differs);

    let error = file
        .save("mine", false)
        .expect_err("the save must be refused");
    assert!(
        matches!(
            error,
            FileError::Changed {
                state: DiskState::Differs,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "aaab",
        "the refused save wrote anyway"
    );
}

#[test]
fn a_deleted_file_is_reported_rather_than_recreated() {
    let directory = TempDir::new("state-vanished");
    let path = directory.path().join("a.txt");
    fs::write(&path, "hello").unwrap();
    let (mut file, _) = TextFile::open(&path).unwrap();

    fs::remove_file(&path).unwrap();
    assert_eq!(file.disk_state().unwrap(), DiskState::Vanished);
    let error = file
        .save("mine", false)
        .expect_err("the save must be refused");
    assert!(error.to_string().contains("deleted on disk"), "{error}");
}

#[test]
fn a_file_that_appeared_under_a_new_buffer_is_not_overwritten() {
    // `iridium notes.md` on an empty directory, and something else creates
    // `notes.md` while the buffer is open. Saving must not silently replace it.
    let directory = TempDir::new("state-appeared");
    let path = directory.path().join("notes.md");
    let (mut file, _) = TextFile::open(&path).unwrap();
    assert!(file.is_new());

    fs::write(&path, "someone else's work").unwrap();
    assert_eq!(file.disk_state().unwrap(), DiskState::Appeared);

    let error = file
        .save("mine", false)
        .expect_err("the save must be refused");
    assert!(
        error.to_string().contains("created by something else"),
        "{error}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "someone else's work");
}

#[test]
fn a_forced_save_overwrites_and_resets_the_baseline() {
    let directory = TempDir::new("state-forced");
    let path = directory.path().join("a.txt");
    fs::write(&path, "theirs").unwrap();
    let (mut file, _) = TextFile::open(&path).unwrap();
    fs::write(&path, "theirs, changed").unwrap();

    file.save("mine", true).expect("a forced save goes ahead");
    assert_eq!(fs::read_to_string(&path).unwrap(), "mine");
    assert_eq!(file.disk_state().unwrap(), DiskState::Agrees);
    assert_eq!(file.saved_text(), Some("mine"));
}

#[test]
fn saving_makes_the_buffer_agree_again() {
    let directory = TempDir::new("save-baseline");
    let path = directory.path().join("a.txt");
    let (mut file, _) = TextFile::open(&path).unwrap();

    file.save("first", false).expect("a new file is created");
    assert!(!file.is_new(), "the file is on disk now");
    assert_eq!(file.disk_state().unwrap(), DiskState::Agrees);

    file.save("second", false).expect("and saved again");
    assert_eq!(fs::read_to_string(&path).unwrap(), "second");
}

#[test]
fn a_byte_order_mark_survives_a_save() {
    let directory = TempDir::new("save-bom");
    let path = directory.path().join("a.txt");
    fs::write(&path, b"\xef\xbb\xbfone\n").unwrap();
    let (mut file, text) = TextFile::open(&path).unwrap();
    assert_eq!(text, "one\n");

    file.save("two\n", false).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"\xef\xbb\xbftwo\n");
}

#[test]
fn reloading_replaces_the_text_and_the_baseline() {
    let directory = TempDir::new("reload");
    let path = directory.path().join("a.txt");
    fs::write(&path, "before").unwrap();
    let (mut file, _) = TextFile::open(&path).unwrap();

    fs::write(&path, "after").unwrap();
    assert_eq!(file.reload().unwrap(), "after");
    assert_eq!(file.disk_state().unwrap(), DiskState::Agrees);
}

#[test]
fn reloading_a_deleted_file_does_not_empty_the_buffer() {
    let directory = TempDir::new("reload-missing");
    let path = directory.path().join("a.txt");
    fs::write(&path, "before").unwrap();
    let (mut file, _) = TextFile::open(&path).unwrap();
    fs::remove_file(&path).unwrap();

    let error = file.reload().expect_err("there is nothing to reload");
    assert!(
        matches!(
            error,
            FileError::Changed {
                state: DiskState::Vanished,
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn renaming_forgets_what_the_old_path_held() {
    // Otherwise a save-as would compare the new path against the old file's
    // bytes and report a change that is not one.
    let directory = TempDir::new("rename");
    let old = directory.path().join("old.txt");
    let new = directory.path().join("new.txt");
    fs::write(&old, "content").unwrap();
    let (mut file, _) = TextFile::open(&old).unwrap();

    file.rename(&new);
    assert_eq!(file.path(), new);
    assert!(file.is_new());
    assert_eq!(file.disk_state().unwrap(), DiskState::Agrees);
    file.save("content", false).expect("the new path is free");
    assert_eq!(fs::read_to_string(&old).unwrap(), "content");
    assert_eq!(fs::read_to_string(&new).unwrap(), "content");
}

// ===== Atomicity =====

#[test]
fn a_successful_write_replaces_the_file_and_leaves_nothing_behind() {
    let directory = TempDir::new("atomic-clean");
    let path = directory.path().join("a.txt");
    fs::write(&path, "old").unwrap();

    write_atomically(&path, b"new").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(
        entries(directory.path()),
        vec!["a.txt".to_owned()],
        "a temporary file was left behind"
    );
}

#[test]
fn the_target_is_untouched_while_the_write_is_in_progress() {
    // The whole guarantee, observed from inside the write. If the contents
    // were being written to the target, this is the moment at which the user's
    // file would be truncated — so this assertion is the one that fails
    // against any implementation that writes in place.
    let directory = TempDir::new("atomic-during");
    let path = directory.path().join("a.txt");
    fs::write(&path, "the original contents").unwrap();

    let observed = std::cell::RefCell::new(Vec::new());
    write_atomically_with(&path, |file| {
        observed.replace(entries(directory.path()));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "the original contents",
            "the target was modified before the rename"
        );
        file.write_all(b"replacement")
    })
    .unwrap();

    let during = observed.into_inner();
    assert_eq!(
        during.len(),
        2,
        "expected the target and one temporary file, saw {during:?}"
    );
    assert!(
        during.iter().any(|name| name == "a.txt"),
        "the target vanished during the write: {during:?}"
    );
    assert!(
        during
            .iter()
            .any(|name| name.contains("iridium") && has_temporary_extension(name)),
        "no temporary file was created beside the target: {during:?}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "replacement");
}

#[test]
fn a_failed_write_leaves_the_original_intact() {
    // A short write followed by a failure: the exact partial write this module
    // exists to survive.
    let directory = TempDir::new("atomic-failed");
    let path = directory.path().join("ledger.csv");
    fs::write(&path, "date,amount\n2026-01-01,100\n").unwrap();

    let error = write_atomically_with(&path, |file| {
        file.write_all(b"date,amo")?;
        Err(io::Error::other("no space left on device"))
    })
    .expect_err("the injected failure must be reported");
    assert_eq!(error.to_string(), "no space left on device");

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "date,amount\n2026-01-01,100\n",
        "the original was damaged by a failed write"
    );
    assert_eq!(
        entries(directory.path()),
        vec!["ledger.csv".to_owned()],
        "the temporary file was not cleaned up"
    );
}

#[test]
fn a_failed_write_to_a_new_file_leaves_no_file() {
    let directory = TempDir::new("atomic-failed-new");
    let path = directory.path().join("fresh.txt");

    write_atomically_with(&path, |_| Err(io::Error::other("nope")))
        .expect_err("the injected failure must be reported");

    assert!(!path.exists(), "a failed write created the file anyway");
    assert!(
        entries(directory.path()).is_empty(),
        "the temporary file was not cleaned up"
    );
}

#[test]
fn a_write_into_a_missing_directory_fails_without_creating_anything() {
    let directory = TempDir::new("atomic-no-directory");
    let path = directory.path().join("absent").join("a.txt");
    let error = write_atomically(&path, b"content").expect_err("there is no directory");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(entries(directory.path()).is_empty());
}

#[test]
fn a_file_with_no_directory_component_writes_into_the_working_directory() {
    // A bare file name has no parent, and an empty parent is not a directory
    // any system call accepts, so both become `.`. Checked on the function
    // rather than by changing this process's working directory, which every
    // other test in this binary shares.
    assert_eq!(parent_of(Path::new("bare.txt")), PathBuf::from("."));
    assert_eq!(parent_of(Path::new("")), PathBuf::from("."));
    assert_eq!(parent_of(Path::new("dir/a.txt")), PathBuf::from("dir"));
    assert_eq!(parent_of(Path::new("/a.txt")), PathBuf::from("/"));
}

#[test]
#[cfg(unix)]
fn permissions_are_preserved() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = TempDir::new("atomic-permissions");
    let path = directory.path().join("private.txt");
    fs::write(&path, "secret").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    write_atomically(&path, b"still secret").unwrap();

    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "a save widened the permissions of a private file"
    );
}

#[test]
#[cfg(unix)]
fn a_symlink_is_followed_rather_than_replaced() {
    // Saving `~/.zshrc` when that is a link into a dotfiles repository must
    // write the file in the repository. Replacing the link with a regular file
    // detaches it silently and the next `git status` is the first anyone hears.
    let directory = TempDir::new("atomic-symlink");
    let real = directory.path().join("real.txt");
    let link = directory.path().join("link.txt");
    fs::write(&real, "original").unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    write_atomically(&link, b"through the link").unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the symlink was replaced by a regular file"
    );
    assert_eq!(fs::read_to_string(&real).unwrap(), "through the link");
}

#[test]
#[cfg(unix)]
fn a_dangling_symlink_is_an_error_rather_than_a_new_file() {
    let directory = TempDir::new("atomic-dangling");
    let link = directory.path().join("link.txt");
    std::os::unix::fs::symlink(directory.path().join("nowhere.txt"), &link).unwrap();

    let error = write_atomically(&link, b"content").expect_err("the link points at nothing");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[test]
fn concurrent_writes_to_one_directory_do_not_collide() {
    // Each temporary name carries the process id, a timestamp and a counter.
    // Two saves in flight at once must not pick the same one; the counter is
    // what makes that true within a process, and this is what checks it.
    let directory = TempDir::new("atomic-concurrent");
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");

    let names = std::cell::RefCell::new(Vec::new());
    write_atomically_with(&first, |file| {
        names.borrow_mut().extend(entries(directory.path()));
        write_atomically_with(&second, |inner| {
            names.borrow_mut().extend(entries(directory.path()));
            inner.write_all(b"second")
        })?;
        file.write_all(b"first")
    })
    .unwrap();

    assert_eq!(fs::read_to_string(&first).unwrap(), "first");
    assert_eq!(fs::read_to_string(&second).unwrap(), "second");
    let seen = names.into_inner();
    let temporaries: std::collections::BTreeSet<&String> = seen
        .iter()
        .filter(|name| has_temporary_extension(name))
        .collect();
    assert_eq!(
        temporaries.len(),
        2,
        "the two writes shared a temporary name: {seen:?}"
    );
}
