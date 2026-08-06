//! Which paths the crawl skips.
//!
//! # Only the crawl
//!
//! Nothing here filters a listing. A directory someone opens by hand shows
//! everything in it, generated or not — a file tree that hid `target/` would
//! be lying about the disk, and opening a build artefact is a legitimate
//! thing to want. What these rules decide is narrower and only that: **which
//! directories the crawl walks into on its own**. Typing a query will not
//! dredge up ten thousand object files; expanding the folder by hand still
//! will.
//!
//! # Why `.gitignore` and not a list of well-known names
//!
//! A hardcoded set of `target`, `node_modules`, `dist` is a guess about
//! somebody else's project, and it is wrong the moment a project builds
//! somewhere else. A repository's `.gitignore` is that project's own
//! statement of what is not source, written by the people who would know.
//!
//! # Nested files are honoured, deepest first
//!
//! A `.gitignore` in a subdirectory overrides the one above it, which is what
//! makes a negation like `!keep.log` inside one folder work without
//! un-ignoring every `*.log` in the repository. Each directory's file is
//! compiled against *its own* base, because an anchored pattern (`/build`)
//! means "in this directory" and a matcher rooted somewhere else would
//! resolve it against the wrong place.
//!
//! # This is I/O and belongs on the reader thread
//!
//! Loading a `.gitignore` reads a file. That happens here, which is called
//! only from the worker — never from a frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The directory git keeps its own state in. Never crawled, and it is not in
/// anybody's `.gitignore` because git does not need telling.
const GIT_DIRECTORY: &str = ".git";

/// The ignore rules in force under one root, loaded as they are needed.
pub struct Ignores {
    /// Where the walk up stops. A path outside it is not this tree's problem.
    root: PathBuf,
    /// Directory to its own compiled `.gitignore`, or `None` when it has none
    /// worth consulting.
    ///
    /// Cached because the crawl asks about every entry of every directory it
    /// reads, and each miss would otherwise re-read and re-compile the same
    /// file once per sibling.
    loaded: HashMap<PathBuf, Option<Gitignore>>,
}

impl Ignores {
    /// Rules for a hierarchy rooted at `root`.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            loaded: HashMap::new(),
        }
    }

    /// Whether the crawl should leave `path` alone.
    ///
    /// `is_dir` is the entry's own kind, which git needs: a pattern ending in
    /// `/` matches directories only, and answering with the wrong kind ignores
    /// a file that should have been searched.
    pub fn is_ignored(&mut self, path: &Path, is_dir: bool) -> bool {
        if path.file_name().is_some_and(|name| name == GIT_DIRECTORY) {
            return true;
        }

        // Deepest first: the nearest `.gitignore` to a path is the one whose
        // author knew most about it, and a negation there is meant to win.
        let mut directory = path.parent();
        while let Some(current) = directory {
            if let Some(rules) = self.rules_in(current) {
                let verdict = rules.matched(path, is_dir);
                if verdict.is_ignore() {
                    return true;
                }
                if verdict.is_whitelist() {
                    return false;
                }
            }
            if current == self.root {
                break;
            }
            directory = current.parent();
        }
        false
    }

    /// The compiled rules `directory` declares, loading them on first ask.
    ///
    /// A file that cannot be read or cannot be parsed yields no rules rather
    /// than an error: the consequence of getting this wrong in the cautious
    /// direction is a crawl that walks a little more than it needed to, and
    /// the consequence in the other is a file the user can never find.
    fn rules_in(&mut self, directory: &Path) -> Option<&Gitignore> {
        if !self.loaded.contains_key(directory) {
            let compiled = compile(directory, directory == self.root);
            self.loaded.insert(directory.to_path_buf(), compiled);
        }
        self.loaded.get(directory).and_then(Option::as_ref)
    }
}

impl std::fmt::Debug for Ignores {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Ignores")
            .field("root", &self.root)
            .field("directories_loaded", &self.loaded.len())
            // The compiled globs have no `Debug` worth printing, and there can
            // be thousands of them.
            .finish_non_exhaustive()
    }
}

/// Compiles one directory's `.gitignore`, and at the root also the private
/// exclude file git keeps out of the working tree.
///
/// `None` when there is nothing to match against, so the common case — a
/// directory with no ignore file at all, which is nearly all of them — costs
/// one cache hit and no glob evaluation.
fn compile(directory: &Path, is_root: bool) -> Option<Gitignore> {
    let mut builder = GitignoreBuilder::new(directory);
    // `add` returns the error rather than raising it, and "no such file" is
    // the ordinary answer for almost every directory.
    drop(builder.add(directory.join(".gitignore")));
    if is_root {
        // `.git/info/exclude` is the same syntax and the same precedence as a
        // root `.gitignore`; a project that puts its editor droppings there
        // rather than in a tracked file means exactly what it says.
        drop(builder.add(directory.join(GIT_DIRECTORY).join("info").join("exclude")));
    }
    match builder.build() {
        Ok(rules) if !rules.is_empty() => Some(rules),
        _ => None,
    }
}
