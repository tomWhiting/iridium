use crate::{
    DiskState, Durability, FileError, Publication, TextFile, atomic::Replacement,
    test_support::TempDir,
};
use std::{
    io::{self, Write},
    path::Path,
};

fn publish_without_directory_sync(path: &Path, bytes: &[u8]) -> io::Result<Publication> {
    let mut replacement = Replacement::new(path)?;
    replacement.file_mut().write_all(bytes)?;
    replacement
        .prepare()?
        .commit_with(|_| Err(io::Error::other("injected directory sync failure")))
}

#[test]
fn post_publication_failure_retains_the_new_baseline() {
    let directory = TempDir::new("published-save");
    let path = directory.path().join("text");
    std::fs::write(&path, "old").expect("fixture");
    let (mut file, _) = TextFile::open(&path).expect("open");
    let error = file
        .save_using("new", false, publish_without_directory_sync)
        .expect_err("durability failure");
    assert!(matches!(error, FileError::Published { .. }));
    assert!(error.to_string().contains("saved, but"));
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "new");
    assert_eq!(file.saved_text(), Some("new"));
    assert_eq!(file.disk_state().expect("baseline"), DiskState::Agrees);
    std::fs::write(&path, "ext").expect("external write");
    assert_eq!(file.disk_state().expect("changed"), DiskState::Differs);
}

#[test]
fn pre_publication_failure_preserves_target_and_baseline() {
    let directory = TempDir::new("unpublished-save");
    let path = directory.path().join("text");
    std::fs::write(&path, "old").expect("fixture");
    let (mut file, _) = TextFile::open(&path).expect("open");
    let error = file
        .save_using("new", false, |target, bytes| {
            let mut replacement = Replacement::new(target)?;
            replacement.file_mut().write_all(bytes)?;
            Err(io::Error::other("injected pre-publication failure"))
        })
        .expect_err("failure");
    assert!(matches!(error, FileError::Unwritable { .. }));
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "old");
    assert_eq!(file.saved_text(), Some("old"));
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("directory")
            .count(),
        1
    );
}

#[test]
fn durability_outcome_distinguishes_unsupported_from_failed_sync() {
    let directory = TempDir::new("sync-outcome");
    let path = directory.path().join("text");
    let mut replacement = Replacement::new(&path).expect("stage");
    replacement.file_mut().write_all(b"saved").expect("write");
    let outcome = replacement
        .prepare()
        .expect("prepare")
        .commit_with(|_| Ok(false))
        .expect("publish");
    assert!(matches!(outcome.durability, Durability::Unsupported));
    let outcome = publish_without_directory_sync(&path, b"changed").expect("published");
    assert!(matches!(outcome.durability, Durability::Unconfirmed(_)));
    assert_eq!(std::fs::read(&path).expect("published bytes"), b"changed");
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("directory")
            .count(),
        1
    );
}
