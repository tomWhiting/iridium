//! Fixtures for harnesses that need the themes this repository ships.
//!
//! `themes/` holds the light faces D-1 ruled *out* of the binary — Variant B
//! "Paper" and Variant C "Monochrome" of `docs/design/LIGHT-THEME-MAP.md`
//! §2.3. Nothing links them; they are read from disk exactly as a user's
//! `--theme ./themes/paper.json` reads them, through [`crate::theme::load`].
//!
//! Two kinds of test still need all three light faces together. The desktop
//! overlay's derivation tests assert that a panel, a hairline and a tab card
//! stay distinguishable *whatever* light theme is underneath, and the
//! screenshot harness renders the map's §4.1 frames for each. Both would
//! otherwise have to carry their own copy of the path from a crate directory
//! to the repository root, and the copies would be right until the day one of
//! them moved.
//!
//! Behind the `test-support` feature, on the same terms as
//! [`iridium_file::test_support`]: this reads the working tree, so nothing
//! outside a test should ever want it.
//!
//! # Why these return `Result`
//!
//! Compiled under a feature, this is ordinary library code — the workspace's
//! `panic`, `unwrap_used` and `expect_used` lints apply to it and rightly so,
//! and a fixture that answered a missing file with `panic!` would not build.
//! Reporting the failure lets each caller say what it was looking for, which
//! is a better message than any this module could write.

use std::path::PathBuf;

use iridium_editor::Theme;

use crate::theme::{ThemeChoice, ThemeError, load};

/// The repository's `themes/` directory.
///
/// Derived from this crate's own manifest directory, which is the only
/// location Cargo guarantees a test can find itself from — a relative path
/// would depend on which directory `cargo test` was invoked in.
#[must_use]
pub fn themes_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../themes"))
}

/// One shipped theme, by file stem.
///
/// # Errors
///
/// [`ThemeError`] when the file is missing or does not parse. For the files
/// this repository ships, either is a repository defect rather than a
/// condition a caller can recover from.
pub fn shipped_theme(slug: &str) -> Result<Theme, ThemeError> {
    load(&ThemeChoice::File(
        themes_dir().join(format!("{slug}.json")),
    ))
}

/// The three light faces of the map's §2.3, in the map's own order, each with
/// the slug its screenshots and failure messages are named by.
///
/// Variant A comes from [`Theme::light`] rather than from a file, because that
/// is where it lives: D-1 ruled it the shipped preset. The two that follow
/// come off disk. A caller sweeping this list is therefore asking its question
/// of both halves of the ruling at once, which is the point.
///
/// # Errors
///
/// [`ThemeError`] from the first file that cannot be loaded.
pub fn classic_light_faces() -> Result<Vec<(&'static str, Theme)>, ThemeError> {
    Ok(vec![
        ("platinum", Theme::light()),
        ("paper", shipped_theme("paper")?),
        ("monochrome", shipped_theme("monochrome")?),
    ])
}

#[cfg(test)]
mod tests {
    use super::{classic_light_faces, shipped_theme, themes_dir};

    #[test]
    fn the_shipped_themes_are_where_this_says_they_are() {
        let directory = themes_dir();
        assert!(
            directory.is_dir(),
            "`{}` must exist — it is where D-1's two files live",
            directory.display()
        );
    }

    #[test]
    fn each_shipped_theme_loads_and_names_itself() {
        assert_eq!(
            shipped_theme("paper")
                .expect("themes/paper.json loads")
                .name,
            "Iridium Paper"
        );
        assert_eq!(
            shipped_theme("monochrome")
                .expect("themes/monochrome.json loads")
                .name,
            "Iridium Monochrome"
        );
    }

    /// A file that is not there reports itself rather than falling back to a
    /// default — the failure that would otherwise make a sweep pass while
    /// checking the same theme three times.
    #[test]
    fn a_theme_that_is_not_shipped_is_an_error() {
        let error = shipped_theme("no-such-theme").expect_err("the file does not exist");
        assert!(
            error.to_string().contains("no-such-theme"),
            "the missing file is named: {error}"
        );
    }

    /// The list is three distinct light faces, not the same one three times.
    #[test]
    fn the_three_faces_are_three() {
        let faces = classic_light_faces().expect("the shipped themes load");
        assert_eq!(faces.len(), 3);
        for (slug, theme) in &faces {
            assert!(!theme.is_dark, "{slug} is a light face");
        }
        for (index, (first_slug, first)) in faces.iter().enumerate() {
            for (second_slug, second) in faces.iter().skip(index + 1) {
                assert_ne!(
                    first.editor, second.editor,
                    "{first_slug} and {second_slug} are the same face"
                );
            }
        }
    }
}
