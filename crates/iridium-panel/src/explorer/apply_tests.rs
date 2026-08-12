//! Tests for carrying a plan out.
//!
//! Unlike the plan builder's, these must touch a real filesystem — the whole
//! claim is about what the operating system does, and a fake would be a
//! restatement of the belief being tested rather than a check on it.
//!
//! The load-bearing test is [`a_rename_refuses_to_overwrite_rather_than_
//! destroying_what_is_there`]. `std::fs::rename` would pass every other test
//! here and fail that one, which is the whole reason this module carries
//! platform code.

use std::fs;
use std::path::{Path, PathBuf};

use iridium_file::test_support::TempDir;

use super::apply::apply;
use super::plan::{Operation, Plan};

/// A plan of exactly these operations.
fn plan_of(operations: Vec<Operation>) -> Plan {
    Plan { operations }
}

fn write(path: &Path, contents: &str) {
    fs::write(path, contents).expect("the fixture was written");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("the file was read")
}

fn rename(from: PathBuf, to: PathBuf) -> Operation {
    Operation::Rename { from, to }
}

#[test]
fn a_rename_moves_the_file_and_its_contents() {
    let directory = TempDir::new("oil-apply-rename");
    let root = directory.path();
    write(&root.join("a.txt"), "contents");

    let done = apply(&plan_of(vec![rename(
        root.join("a.txt"),
        root.join("b.txt"),
    )]))
    .expect("the rename succeeded");

    assert_eq!(done, 1);
    assert!(!root.join("a.txt").exists(), "the old name is still there");
    assert_eq!(
        read(&root.join("b.txt")),
        "contents",
        "a rename must move the file, never copy and re-write it"
    );
}

#[test]
fn a_rename_refuses_to_overwrite_rather_than_destroying_what_is_there() {
    // THE test this module exists for. `std::fs::rename` replaces the
    // destination silently on Unix, so an implementation using it would pass
    // every other test here and lose the user's file in this one.
    let directory = TempDir::new("oil-apply-clobber");
    let root = directory.path();
    write(&root.join("source.txt"), "the file being moved");
    write(&root.join("target.txt"), "THE FILE THAT MUST SURVIVE");

    let failure = apply(&plan_of(vec![rename(
        root.join("source.txt"),
        root.join("target.txt"),
    )]))
    .expect_err("renaming onto an existing file must fail");

    assert_eq!(failure.completed, 0);
    assert_eq!(
        read(&root.join("target.txt")),
        "THE FILE THAT MUST SURVIVE",
        "the rename overwrote a file it was never allowed to touch"
    );
    assert!(
        root.join("source.txt").exists(),
        "the source vanished even though the rename failed"
    );
}

#[test]
fn a_rename_refuses_a_destination_that_is_a_directory() {
    // The same hazard wearing a different hat: a directory in the way is not
    // a reason to remove it.
    let directory = TempDir::new("oil-apply-clobber-dir");
    let root = directory.path();
    write(&root.join("source.txt"), "x");
    fs::create_dir(root.join("target")).expect("the fixture directory was made");
    write(&root.join("target/inside.txt"), "still here");

    apply(&plan_of(vec![rename(
        root.join("source.txt"),
        root.join("target"),
    )]))
    .expect_err("renaming onto a directory must fail");

    assert_eq!(read(&root.join("target/inside.txt")), "still here");
}

#[test]
fn creating_a_file_makes_it_empty_and_creating_a_directory_makes_it() {
    let directory = TempDir::new("oil-apply-create");
    let root = directory.path();

    apply(&plan_of(vec![
        Operation::Create {
            path: root.join("notes.md"),
            directory: false,
        },
        Operation::Create {
            path: root.join("docs"),
            directory: true,
        },
    ]))
    .expect("both were created");

    assert_eq!(read(&root.join("notes.md")), "");
    assert!(root.join("docs").is_dir());
}

#[test]
fn creating_over_an_existing_file_refuses_and_leaves_it_alone() {
    let directory = TempDir::new("oil-apply-create-over");
    let root = directory.path();
    write(&root.join("notes.md"), "years of notes");

    apply(&plan_of(vec![Operation::Create {
        path: root.join("notes.md"),
        directory: false,
    }]))
    .expect_err("creating over an existing file must fail");

    assert_eq!(
        read(&root.join("notes.md")),
        "years of notes",
        "creating truncated a file that was already there"
    );
}

#[test]
fn deleting_removes_a_file_and_a_whole_directory() {
    let directory = TempDir::new("oil-apply-delete");
    let root = directory.path();
    write(&root.join("gone.txt"), "x");
    fs::create_dir(root.join("tree")).expect("the fixture directory was made");
    write(&root.join("tree/inside.txt"), "x");

    apply(&plan_of(vec![
        Operation::Delete {
            path: root.join("gone.txt"),
        },
        Operation::Delete {
            path: root.join("tree"),
        },
    ]))
    .expect("both were deleted");

    assert!(!root.join("gone.txt").exists());
    assert!(!root.join("tree").exists());
}

#[test]
#[cfg(unix)]
fn deleting_a_symlink_removes_the_link_and_not_what_it_points_at() {
    // The difference between removing a shortcut and removing somebody's
    // home directory. `remove_dir_all` on a followed symlink would take the
    // target's contents with it.
    let directory = TempDir::new("oil-apply-symlink");
    let root = directory.path();
    fs::create_dir(root.join("real")).expect("the fixture directory was made");
    write(&root.join("real/precious.txt"), "must survive");
    std::os::unix::fs::symlink(root.join("real"), root.join("link"))
        .expect("the fixture symlink was made");

    apply(&plan_of(vec![Operation::Delete {
        path: root.join("link"),
    }]))
    .expect("the link was deleted");

    assert!(!root.join("link").exists(), "the link is still there");
    assert_eq!(
        read(&root.join("real/precious.txt")),
        "must survive",
        "deleting a symlink followed it and destroyed the target"
    );
}

#[test]
fn a_failure_stops_the_run_and_says_how_far_it_got() {
    let directory = TempDir::new("oil-apply-stop");
    let root = directory.path();
    write(&root.join("first.txt"), "x");
    write(&root.join("blocked.txt"), "in the way");
    write(&root.join("third.txt"), "x");

    let failure = apply(&plan_of(vec![
        rename(root.join("first.txt"), root.join("first-done.txt")),
        rename(root.join("third.txt"), root.join("blocked.txt")),
        rename(root.join("blocked.txt"), root.join("never.txt")),
    ]))
    .expect_err("the second operation must fail");

    assert_eq!(
        failure.completed, 1,
        "exactly one operation should have happened before the failure"
    );
    assert!(
        root.join("first-done.txt").exists(),
        "the operation before the failure did not happen"
    );
    assert!(
        !root.join("never.txt").exists(),
        "an operation after the failure ran anyway"
    );
    assert_eq!(read(&root.join("blocked.txt")), "in the way");
}

#[test]
fn a_failure_describes_what_it_was_doing() {
    let directory = TempDir::new("oil-apply-describe");
    let root = directory.path();
    write(&root.join("a.txt"), "x");
    write(&root.join("b.txt"), "y");

    let failure = apply(&plan_of(vec![rename(
        root.join("a.txt"),
        root.join("b.txt"),
    )]))
    .expect_err("the rename failed");

    let described = failure.describe();
    assert!(
        described.contains("rename") && described.contains("b.txt"),
        "the message does not say what failed: {described}"
    );
    assert!(
        described.contains("0 operations before it"),
        "the message does not say how far it got: {described}"
    );
}

#[test]
fn deleting_something_that_is_not_there_fails_rather_than_passing_quietly() {
    // Succeeding would hide a plan built from a stale tree, which is exactly
    // the situation where the panel needs to re-read the directory.
    let directory = TempDir::new("oil-apply-missing");
    let root = directory.path();

    apply(&plan_of(vec![Operation::Delete {
        path: root.join("never-existed.txt"),
    }]))
    .expect_err("deleting a path that is not there must be reported");
}

#[test]
fn an_empty_plan_does_nothing_and_says_so() {
    assert_eq!(apply(&Plan::default()).expect("nothing to do"), 0);
}

#[test]
fn a_swap_through_a_temporary_lands_both_files_in_the_right_place() {
    // The end-to-end of what `plan`'s cycle-breaking produces, run against a
    // real filesystem: the two files must genuinely exchange places with
    // neither destroyed on the way.
    let directory = TempDir::new("oil-apply-swap");
    let root = directory.path();
    write(&root.join("a"), "A");
    write(&root.join("b"), "B");

    apply(&plan_of(vec![
        rename(root.join("a"), root.join(".iridium-oil-swap")),
        rename(root.join("b"), root.join("a")),
        rename(root.join(".iridium-oil-swap"), root.join("b")),
    ]))
    .expect("the swap ran");

    assert_eq!(read(&root.join("a")), "B");
    assert_eq!(read(&root.join("b")), "A");
    assert!(
        !root.join(".iridium-oil-swap").exists(),
        "the temporary was left behind"
    );
}

#[test]
fn a_temporary_that_already_exists_stops_the_swap_instead_of_eating_it() {
    // `plan` can only avoid temporary names it knows about; a real directory
    // may already hold one. The no-replace rename is what turns that from a
    // check this module must remember into one the kernel makes.
    let directory = TempDir::new("oil-apply-temp-taken");
    let root = directory.path();
    write(&root.join("a"), "A");
    write(&root.join(".iridium-oil-swap"), "SOMEBODY ELSE'S FILE");

    apply(&plan_of(vec![rename(
        root.join("a"),
        root.join(".iridium-oil-swap"),
    )]))
    .expect_err("the temporary was occupied and the rename must refuse");

    assert_eq!(
        read(&root.join(".iridium-oil-swap")),
        "SOMEBODY ELSE'S FILE"
    );
}
