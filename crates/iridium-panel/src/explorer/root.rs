//! Where the explorer opens when someone picks a directory, and how far a
//! search there may reach.
//!
//! # Why the choice is here and the *guess* is not
//!
//! ⭐ **Two different questions wear the word "root".** "Somebody walked into
//! this folder — may a search read past it?" is a property of the explorer,
//! answered the same way in every face, and it is what this module holds.
//! "Nobody said where to open; where should it be?" is a different question
//! entirely: it consults the file in the active tab, the process's working
//! directory and the user's home, none of which the terminal face and the
//! desktop face need answer alike. That guess stays in each face.
//!
//! The rule this module encodes is one sentence: **a chosen directory earns
//! the crawl, and the top of the filesystem does not.** A person who navigates
//! into a folder has bounded it by navigating into it; `/` is bounded by
//! nothing, no ignore file covers it, and nobody arrives there on purpose.

use std::path::{Path, PathBuf};

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

/// The root someone picked by hand, and how far a search there may reach.
///
/// **A chosen directory earns the crawl**, and that is the whole difference
/// between this and [`explorer_root`]. The rule there is that a directory
/// *this code guessed* is not worth reading past, because nothing bounds it
/// and nobody asked. A person walking into a folder has bounded it themselves
/// — that is what walking into it means — and a search that then refused to
/// look inside would be answering a question nobody asked instead of the one
/// they did.
///
/// The one exception is the top of the filesystem, which no `.gitignore`
/// bounds and which nobody navigates to on purpose. It is shown, and it is
/// not searched past.
#[must_use]
pub fn chosen_root(path: PathBuf) -> ExplorerRoot {
    ExplorerRoot {
        crawl: !is_filesystem_root(&path),
        path,
    }
}

/// Whether `path` is the top of a filesystem — `/`, or a Windows drive root.
///
/// Asked as "has no parent" rather than compared against `/`, so it is right
/// on every platform and right for a UNC path without knowing what one is.
///
/// Public because the *guess* needs it too: a face deciding where to open when
/// nobody said reaches the same conclusion about the top of a filesystem as
/// [`chosen_root`] does, and two spellings of "has no parent" would be two
/// places for that to stop being true.
#[must_use]
pub fn is_filesystem_root(path: &Path) -> bool {
    path.parent().is_none()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::chosen_root;

    #[test]
    fn a_directory_someone_walked_into_is_searched_past() {
        // The difference between a guess and a choice. `explorer_root` will
        // not read past a directory nothing bounds; a person who navigated
        // into one has bounded it by navigating into it, and a search that
        // then refused to look would be answering a question nobody asked.
        let chosen = chosen_root(PathBuf::from("/Users/someone/Documents"));
        assert!(chosen.crawl);
        assert_eq!(chosen.path, PathBuf::from("/Users/someone/Documents"));
    }

    #[test]
    fn walking_up_to_the_top_of_the_filesystem_still_does_not_search_it() {
        // Nobody navigates to `/` on purpose, no ignore file bounds it, and
        // the whole reason this module exists is that rooting there sends a
        // search across the disk. A choice does not make that a good idea.
        let chosen = chosen_root(PathBuf::from("/"));
        assert!(!chosen.crawl);
        assert_eq!(chosen.path, PathBuf::from("/"));
    }
}
