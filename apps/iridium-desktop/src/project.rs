//! Where the file explorer starts, and whether searching may read past it.
//!
//! # The bug this module exists because of
//!
//! A macOS application launched from Finder or Spotlight inherits `/` as its
//! working directory — LaunchServices does not give it anything better. The
//! explorer used `current_dir()` when no file was open, so opening the app
//! from the Applications folder rooted it at the filesystem root, and the
//! search crawl then set off across the entire disk. Reported by Tom on
//! 6 Aug 2026, and it is entirely a consequence of trusting a working
//! directory that nobody chose.
//!
//! # The two questions, answered separately
//!
//! **Where to start** and **whether to search past what is open** are not the
//! same question, and conflating them is what made the failure so loud.
//!
//! The crawl is worth running when it is bounded by something real. Inside a
//! repository it is: the project's own `.gitignore` keeps it out of generated
//! trees, so it reads source and finishes. Pointed at a home directory it is
//! not bounded by anything — there are no ignore rules there, the first
//! twenty thousand directories are consumed by application support files, and
//! it finds nothing anybody wanted. A crawl that cannot work is not worth
//! starting, so it is not started.
//!
//! So: **search past what is open when the root is a project, or when it is
//! the folder of a file the user actually has open. Not when it is a
//! directory this code guessed.**

use std::path::{Path, PathBuf};

/// The directory git keeps its state in — the marker for "this is a project".
const GIT_DIRECTORY: &str = ".git";

/// Where the explorer opens, and how far a search there may reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerRoot {
    /// The directory to show.
    pub path: PathBuf,
    /// Whether a query may read directories nobody has opened.
    ///
    /// `false` is not a degraded mode so much as an honest one: the filter
    /// still narrows everything that has been read, and the panel says that
    /// is what it is doing.
    pub crawl: bool,
}

/// Chooses where the explorer opens.
///
/// Every input is passed in rather than read from the environment, so this is
/// testable — which matters, because the case that went wrong is one that
/// cannot be reproduced from a terminal at all.
///
/// - `active_file` — the path of the file in the active tab, if there is one.
/// - `working_directory` — `std::env::current_dir()`, which may be `/` and may
///   be nothing at all if the directory was deleted underneath the process.
/// - `home` — the user's home directory, the last resort.
#[must_use]
pub fn explorer_root(
    active_file: Option<&Path>,
    working_directory: Option<PathBuf>,
    home: Option<PathBuf>,
) -> ExplorerRoot {
    if let Some(directory) = active_file.and_then(Path::parent).filter(|path| !path.as_os_str().is_empty()) {
        // A file the user has open. Its project if it is in one, and its own
        // folder otherwise — either way a scope they chose by opening it.
        return ExplorerRoot {
            path: project_root(directory).unwrap_or_else(|| directory.to_path_buf()),
            crawl: true,
        };
    }

    // A working directory is worth using only when somebody plausibly chose
    // it — a shell, in other words. `/` means the launcher picked it, and a
    // path with no parent is exactly that test.
    if let Some(directory) = working_directory.filter(|path| !is_filesystem_root(path)) {
        return match project_root(&directory) {
            Some(project) => ExplorerRoot {
                path: project,
                crawl: true,
            },
            // A directory that is not a project: show it, but do not go
            // reading everything under it. Nothing here says how big it is.
            None => ExplorerRoot {
                path: directory,
                crawl: false,
            },
        };
    }

    ExplorerRoot {
        path: home.unwrap_or_else(|| PathBuf::from(".")),
        crawl: false,
    }
}

/// The nearest ancestor of `directory`, itself included, that holds a `.git`.
///
/// Walks up and stops at the first hit, which is what makes a file inside a
/// submodule root at the submodule rather than at the superproject.
///
/// This touches the filesystem — one `exists` per ancestor, bounded by the
/// path's depth — and it is called once, when the panel opens. That is a
/// different thing from the per-frame reads the whole crate is arranged to
/// avoid, and the depth of a path is not a quantity that can surprise anyone.
fn project_root(directory: &Path) -> Option<PathBuf> {
    let mut current = Some(directory);
    while let Some(candidate) = current {
        if candidate.join(GIT_DIRECTORY).exists() {
            return Some(candidate.to_path_buf());
        }
        current = candidate.parent();
    }
    None
}

/// Whether `path` is the top of a filesystem — `/`, or a Windows drive root.
///
/// Asked as "has no parent" rather than compared against `/`, so it is right
/// on every platform and right for a UNC path without knowing what one is.
fn is_filesystem_root(path: &Path) -> bool {
    path.parent().is_none()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use iridium_file::test_support::TempDir;

    use super::{ExplorerRoot, explorer_root};

    /// A directory holding a `.git`, so it reads as a project.
    fn repository(name: &str) -> TempDir {
        let directory = TempDir::new(name);
        std::fs::create_dir(directory.path().join(".git")).expect("the fixture .git was made");
        directory
    }

    #[test]
    fn a_launcher_working_directory_never_becomes_the_root() {
        // **The reported bug.** Finder and Spotlight hand a bundled app `/`,
        // and rooting there sends the search crawl across the whole disk.
        let home = PathBuf::from("/Users/someone");
        let chosen = explorer_root(None, Some(PathBuf::from("/")), Some(home.clone()));
        assert_eq!(
            chosen,
            ExplorerRoot {
                path: home,
                crawl: false
            },
            "the filesystem root is a launcher's answer, not a user's"
        );
    }

    #[test]
    fn a_home_directory_is_shown_but_never_searched_past() {
        // There are no ignore rules in a home directory, so the first twenty
        // thousand directories a crawl reads there are application support
        // files and none of them is what anybody wanted. A crawl that cannot
        // work is not worth starting.
        let chosen = explorer_root(None, None, Some(PathBuf::from("/Users/someone")));
        assert!(!chosen.crawl);
        assert_eq!(chosen.path, PathBuf::from("/Users/someone"));
    }

    #[test]
    fn a_file_in_a_repository_roots_at_the_repository() {
        let project = repository("project-root");
        let nested = project.path().join("crates/thing/src");
        std::fs::create_dir_all(&nested).expect("the fixture directories were made");
        let file = nested.join("lib.rs");
        std::fs::write(&file, "x").expect("the fixture was written");

        let chosen = explorer_root(Some(&file), None, None);
        assert_eq!(
            chosen.path,
            project.path(),
            "a file deep in a project shows the project, not the folder it happens to sit in"
        );
        assert!(chosen.crawl, "a project bounds its own crawl through .gitignore");
    }

    #[test]
    fn the_nearest_repository_wins_so_a_submodule_is_its_own_project() {
        let outer = repository("outer-project");
        let inner = outer.path().join("vendor/inner");
        std::fs::create_dir_all(inner.join(".git")).expect("the fixture directories were made");
        let file = inner.join("main.rs");
        std::fs::write(&file, "x").expect("the fixture was written");

        let chosen = explorer_root(Some(&file), None, None);
        assert_eq!(chosen.path, inner);
    }

    #[test]
    fn a_file_outside_any_repository_roots_at_its_own_folder() {
        // Not at home, and not at the filesystem root. A folder the user has
        // a file open in is a scope they chose, so it is searched — and it is
        // the folder itself, so the scope is small.
        let directory = TempDir::new("loose-file");
        let file = directory.path().join("notes.md");
        std::fs::write(&file, "x").expect("the fixture was written");

        let chosen = explorer_root(Some(&file), Some(PathBuf::from("/")), None);
        assert_eq!(chosen.path, directory.path());
        assert!(chosen.crawl);
    }

    #[test]
    fn a_shell_working_directory_inside_a_project_roots_at_the_project() {
        let project = repository("shell-project");
        let inside = project.path().join("src");
        std::fs::create_dir(&inside).expect("the fixture directory was made");

        let chosen = explorer_root(None, Some(inside), None);
        assert_eq!(chosen.path, project.path());
        assert!(chosen.crawl);
    }

    #[test]
    fn a_shell_working_directory_outside_a_project_is_shown_but_not_searched_past() {
        let directory = TempDir::new("shell-loose");
        let chosen = explorer_root(None, Some(directory.path().to_path_buf()), None);
        assert_eq!(chosen.path, directory.path());
        assert!(
            !chosen.crawl,
            "nothing here says how big this directory is, and nothing bounds a crawl of it"
        );
    }

    #[test]
    fn a_file_with_no_parent_falls_through_rather_than_rooting_on_nothing() {
        // `Path::parent` of a bare name is `Some("")`, which is not a
        // directory anyone meant. It must not become the root.
        let chosen = explorer_root(Some(Path::new("orphan.txt")), None, Some(PathBuf::from("/h")));
        assert_eq!(chosen.path, PathBuf::from("/h"));
        assert!(!chosen.crawl);
    }

    #[test]
    fn everything_missing_still_yields_a_root_rather_than_a_panic() {
        let chosen = explorer_root(None, None, None);
        assert_eq!(chosen.path, PathBuf::from("."));
        assert!(!chosen.crawl);
    }
}
