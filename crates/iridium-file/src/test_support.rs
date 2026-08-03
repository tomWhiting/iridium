//! Fixtures for tests that need real files on a real disk.
//!
//! Public behind the `test-support` feature so the faces' own test suites can
//! build their fixtures in the same self-removing directory this crate's tests
//! use, rather than each keeping a private copy that drifts. Nothing outside a
//! test should ever depend on it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes directories made by different tests in one run.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    ///
    /// # Panics
    ///
    /// Panics when the directory cannot be created — this is test fixture
    /// code, and a fixture that cannot exist should fail the test loudly.
    #[must_use]
    pub fn new(label: &str) -> Self {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("iridium-{}-{label}-{sequence}", std::process::id()));
        let created = fs::create_dir_all(&path);
        // An assertion rather than `expect`: this module is compiled as
        // ordinary library code under its feature, where the workspace's
        // unwrap and expect lints rightly apply, and a fixture that cannot
        // exist should fail the test loudly either way.
        assert!(
            created.is_ok(),
            "the test directory could not be created: {created:?}"
        );
        Self { path }
    }

    /// Where it is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
