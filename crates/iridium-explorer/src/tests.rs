//! Tests for the filesystem source.
//!
//! Every one of these builds a real directory and reads it with the real
//! worker thread. A mocked filesystem would prove nothing about the two
//! properties that actually matter here — that a read never happens on the
//! calling thread, and that an id survives a reload — because both are
//! properties of the arrangement rather than of the code that would be
//! mocked out.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use iridium_file::test_support::TempDir;
use iridium_tree::TreeSource as _;

use crate::{EntryKind, FileTree, NodeId};

/// How long a test will wait for a directory read before calling it hung.
///
/// Generous, because it is a failure bound and not a measurement: a loaded
/// machine reading four files should never come near it, and a test that
/// hangs forever is worse than one that fails.
const PATIENCE: Duration = Duration::from_secs(10);

/// Waits for at least one directory read to land.
///
/// Returns `false` on timeout rather than panicking, so the calling test
/// names what it was waiting for.
fn settle(tree: &mut FileTree) -> bool {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if tree.drain() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    false
}

/// The names of `parent`'s children, in the order they would be drawn.
fn names(tree: &mut FileTree, parent: NodeId) -> Vec<String> {
    tree.children(Some(&parent))
        .into_iter()
        .filter_map(|child| tree.info(child).map(|info| info.name.to_owned()))
        .collect()
}

/// Opens a tree on `directory` and waits for the root listing.
fn opened(directory: &TempDir) -> FileTree {
    let mut tree = FileTree::open(directory.path().to_path_buf()).expect("the reader thread ran");
    assert!(settle(&mut tree), "the root listing never arrived");
    tree
}

#[test]
fn the_first_call_returns_nothing_rather_than_reading_the_directory() {
    let directory = TempDir::new("explorer-nonblocking");
    std::fs::write(directory.path().join("a.txt"), "a").expect("the fixture was written");

    let mut tree = FileTree::open(directory.path().to_path_buf()).expect("the reader thread ran");
    let root = tree.root();

    // The whole contract of this crate. Two halves, and they fail
    // differently on purpose.
    //
    // The first is a race, and knowingly so: the calling thread does a
    // `HashMap` lookup while the worker has to be scheduled, open a
    // directory and send down a channel, so the margin is orders of
    // magnitude — but it is a margin, not a guarantee. A flake here means
    // the worker won, not that the code is wrong.
    assert!(
        tree.children(Some(&root)).is_empty(),
        "the listing was answered immediately, which means either that something read the \
         directory on this thread — the defect — or that the worker beat a `HashMap` \
         lookup, which is the known race in this assertion and not a defect"
    );
    assert!(
        tree.info(root).expect("the root exists").is_loading,
        "a node whose read is outstanding must say so, or a face cannot tell it from an \
         empty directory"
    );

    // The second half is race-free, and it is the one that actually pins the
    // arrangement: a `settle` can only succeed if a request reached the
    // worker. An implementation that read the directory on the calling
    // thread would have posted nothing, and this would time out.
    assert!(settle(&mut tree), "the root listing never arrived");
    assert_eq!(names(&mut tree, root), vec!["a.txt".to_owned()]);
    assert!(
        !tree.info(root).expect("the root exists").is_loading,
        "the read landed, so nothing is outstanding"
    );
}

#[test]
fn directories_sort_before_files_and_names_sort_case_insensitively() {
    let directory = TempDir::new("explorer-order");
    for name in ["zebra.txt", "Apple.txt", "apple.md"] {
        std::fs::write(directory.path().join(name), "x").expect("the fixture was written");
    }
    for name in ["src", "Docs"] {
        std::fs::create_dir(directory.path().join(name)).expect("the fixture directory was made");
    }

    let mut tree = opened(&directory);
    let root = tree.root();

    // Directories first, then names by lowercase. `Docs` before `src`, and
    // `Apple.txt` beside `apple.md` rather than sorted away from it by the
    // capital — which is what a plain byte order would do.
    assert_eq!(
        names(&mut tree, root),
        vec![
            "Docs".to_owned(),
            "src".to_owned(),
            "apple.md".to_owned(),
            "Apple.txt".to_owned(),
            "zebra.txt".to_owned(),
        ]
    );
}

#[test]
fn a_reload_keeps_the_id_of_a_file_that_is_still_there() {
    let directory = TempDir::new("explorer-identity");
    std::fs::write(directory.path().join("keep.txt"), "keep").expect("the fixture was written");
    std::fs::write(directory.path().join("gone.txt"), "gone").expect("the fixture was written");

    let mut tree = opened(&directory);
    let root = tree.root();
    let before = tree.node_at(&directory.path().join("keep.txt"));
    assert!(before.is_some(), "the file was listed");

    std::fs::remove_file(directory.path().join("gone.txt")).expect("the fixture was removed");
    std::fs::write(directory.path().join("new.txt"), "new").expect("the fixture was written");
    tree.reload(root);
    assert!(settle(&mut tree), "the reload never landed");

    // **The trap this crate's identity rule exists for.** An editable
    // listing applies changes by diffing rows by id. If a reload minted a
    // fresh id for a file that never moved, every refresh would read as
    // delete-plus-create — and for a large file that means destroying its
    // contents and writing them back rather than leaving it alone.
    assert_eq!(
        tree.node_at(&directory.path().join("keep.txt")),
        before,
        "a file that did not move kept its identity"
    );
    assert_eq!(
        names(&mut tree, root),
        vec!["keep.txt".to_owned(), "new.txt".to_owned()],
        "the membership is the new one"
    );
}

#[test]
fn a_directory_that_cannot_be_listed_says_why_instead_of_looking_empty() {
    let directory = TempDir::new("explorer-unreadable");
    let missing = directory.path().join("not-here");

    let mut tree = FileTree::open(missing).expect("the reader thread ran");
    assert!(settle(&mut tree), "the failed read never landed");
    let root = tree.root();

    let info = tree.info(root).expect("the root exists");
    assert!(
        info.error.is_some(),
        "an unreadable directory and an empty one look identical to a face unless the \
         failure is carried"
    );
    assert!(!info.is_loading, "the read finished, unsuccessfully");
    assert!(tree.children(Some(&root)).is_empty());

    // And it is not retried on every frame: a failed listing that re-posted
    // itself would hammer a missing network mount for as long as the node
    // stayed open.
    assert!(
        !tree.drain(),
        "a failed listing did not ask again by itself"
    );
}

#[test]
fn an_id_this_tree_never_issued_is_a_no_op_everywhere() {
    let directory = TempDir::new("explorer-stale-id");
    let mut tree = opened(&directory);

    // A face holds ids across mutations, so it can present one from a tree
    // that has since been rebuilt. None of these may panic.
    let stranger = NodeId(9_999);
    assert!(tree.info(stranger).is_none());
    assert!(!tree.has_children(&stranger));
    assert!(tree.children(Some(&stranger)).is_empty());
    tree.reload(stranger);
    assert!(
        tree.node_at(&PathBuf::from("/definitely/not/listed"))
            .is_none()
    );
}

#[test]
fn a_file_is_never_drawn_as_openable() {
    let directory = TempDir::new("explorer-leaf");
    std::fs::write(directory.path().join("a.txt"), "a").expect("the fixture was written");
    std::fs::create_dir(directory.path().join("sub")).expect("the fixture directory was made");

    let mut tree = opened(&directory);
    let file = tree
        .node_at(&directory.path().join("a.txt"))
        .expect("the file was listed");
    let subdirectory = tree
        .node_at(&directory.path().join("sub"))
        .expect("the directory was listed");

    assert!(!tree.has_children(&file));
    assert!(tree.has_children(&subdirectory));
    assert_eq!(
        tree.info(file).expect("the file exists").kind,
        EntryKind::File
    );

    // A file is not a directory, so there is nothing to re-read and nothing
    // is posted — a `reload` that queued a read for every row would turn a
    // refresh of a large tree into a storm of failing `readdir`s.
    tree.reload(file);
    assert!(!tree.drain(), "reloading a file asked for nothing");
}

#[cfg(unix)]
#[test]
fn a_symlink_to_a_directory_is_a_leaf_so_a_cycle_cannot_be_walked() {
    let directory = TempDir::new("explorer-symlink");
    std::fs::create_dir(directory.path().join("real")).expect("the fixture directory was made");
    // The cycle `iridium-tree` says it cannot see coming: a link inside a
    // directory, pointing at that directory.
    std::os::unix::fs::symlink(directory.path(), directory.path().join("loop"))
        .expect("the fixture link was made");

    let mut tree = opened(&directory);
    let link = tree
        .node_at(&directory.path().join("loop"))
        .expect("the link was listed");

    assert_eq!(
        tree.info(link).expect("the link exists").kind,
        EntryKind::Symlink,
        "a link reports as a link, not as whatever it points at"
    );
    assert!(
        !tree.has_children(&link),
        "a link that is never openable is a cycle that can never be walked"
    );
    assert!(tree.children(Some(&link)).is_empty());
}

#[test]
fn a_root_with_no_final_component_shows_its_whole_path() {
    // `/` has no file name, and neither does anything ending in `..`. The
    // naive answer — `file_name()` unwrapped — panics on the one path every
    // filesystem has.
    let mut tree = FileTree::open(PathBuf::from("/")).expect("the reader thread ran");
    let root = tree.root();
    assert_eq!(tree.info(root).expect("the root exists").name, "/");
    assert!(settle(&mut tree), "the root listing never arrived");
}

#[test]
fn the_crawl_reaches_a_directory_nobody_asked_for() {
    let directory = TempDir::new("explorer-crawl");
    let deep = directory.path().join("a/b/c");
    std::fs::create_dir_all(&deep).expect("the fixture directories were made");
    std::fs::write(deep.join("buried.txt"), "b").expect("the fixture was written");

    let mut tree = opened(&directory);
    // Nothing below the root is known: `children` was never called on `a`.
    assert!(tree.node_at(&deep).is_none());
    assert!(
        !tree.is_fully_crawled(),
        "there is a frontier to work through"
    );

    // Each pass posts what it can and collects what landed. Bounded, so a
    // caller doing this once per frame is what fills the tree in.
    let deadline = Instant::now() + PATIENCE;
    while !tree.is_fully_crawled() && Instant::now() < deadline {
        tree.crawl(8);
        tree.drain();
        std::thread::sleep(Duration::from_millis(1));
    }

    assert!(tree.is_fully_crawled(), "the crawl never finished");
    assert!(
        tree.node_at(&deep.join("buried.txt")).is_some(),
        "a file three levels below anything anyone opened"
    );
    assert!(!tree.crawl_hit_its_limit());
}

#[test]
fn the_crawl_posts_nothing_it_was_not_given_budget_for() {
    let directory = TempDir::new("explorer-crawl-budget");
    for name in ["one", "two", "three"] {
        std::fs::create_dir(directory.path().join(name)).expect("the fixture directory was made");
    }

    let mut tree = opened(&directory);
    // The rate is the caller's to set: a frame that posted every directory it
    // could see would put a thousand requests behind the first answer.
    assert_eq!(tree.crawl(2), 2);
    assert_eq!(tree.crawl(2), 1, "only one directory was left to ask for");
    assert_eq!(tree.crawl(2), 0, "and then there is nothing to do");
}

#[test]
fn a_directory_already_being_read_is_not_read_a_second_time_by_the_crawl() {
    let directory = TempDir::new("explorer-crawl-overlap");
    std::fs::create_dir(directory.path().join("sub")).expect("the fixture directory was made");
    std::fs::write(directory.path().join("sub/a.txt"), "a").expect("the fixture was written");

    let mut tree = opened(&directory);
    let sub = tree
        .node_at(&directory.path().join("sub"))
        .expect("the directory was listed");

    // A hand opens it, which posts the read. The crawl then reaches the same
    // node in its queue and must leave it alone — asking again would double
    // the work for the one directory somebody is actually looking at.
    drop(tree.children(Some(&sub)));
    assert_eq!(
        tree.crawl(8),
        0,
        "the crawl re-requested a directory that was already outstanding"
    );
}

#[test]
fn an_ignored_directory_is_listed_but_never_crawled_into() {
    let directory = TempDir::new("explorer-crawl-ignored");
    let root = directory.path();
    std::fs::write(root.join(".gitignore"), "build/\n").expect("the fixture was written");
    std::fs::create_dir(root.join("build")).expect("the fixture directory was made");
    std::fs::write(root.join("build/artefact.txt"), "a").expect("the fixture was written");

    let mut tree = opened(&directory);
    // Listed like anything else — the rule is about the crawl, not the tree.
    let root_id = tree.root();
    assert!(
        names(&mut tree, root_id).contains(&"build".to_owned()),
        "an ignored directory is still shown"
    );

    let deadline = Instant::now() + PATIENCE;
    while !tree.is_fully_crawled() && Instant::now() < deadline {
        tree.crawl(8);
        tree.drain();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        tree.node_at(&root.join("build/artefact.txt")).is_none(),
        "the crawl walked into a directory the project said to ignore"
    );

    // And a hand still opens it.
    let build = tree
        .node_at(&root.join("build"))
        .expect("the directory was listed");
    drop(tree.children(Some(&build)));
    assert!(settle(&mut tree), "the manual read never landed");
    assert!(tree.node_at(&root.join("build/artefact.txt")).is_some());
}
