//! Where the configuration file lives.
//!
//! One path, decided by the XDG Base Directory specification, because the
//! alternative is a search order — and a search order means a file that is
//! being read is a different question from the file the user just edited.

use std::path::{Path, PathBuf};

/// The directory Iridium keeps its configuration in, under the config home.
pub const CONFIG_DIRECTORY: &str = "iridium";

/// The configuration file's name.
pub const CONFIG_FILE: &str = "config.toml";

/// The directory the XDG specification falls back to under `$HOME`.
const XDG_FALLBACK: &str = ".config";

/// The environment variable naming the configuration home.
const XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";

/// Where the configuration file is, given the environment.
///
/// Every input is passed in rather than read from the environment, so the
/// decision is testable without a process that has one — and so a face that
/// wants to point the loader somewhere else (a test harness, a `--config`
/// flag) has a way in that does not involve setting variables globally.
///
/// - `config_home` — `$XDG_CONFIG_HOME`. **Ignored when it is relative**, as
///   the specification requires: a relative config home would resolve against
///   whatever working directory the process happened to inherit, which for an
///   application launched from Finder is `/`.
/// - `home` — `$HOME`, under which `.config` is the specified fallback.
///
/// Returns `None` when neither is usable, which is the honest answer: there is
/// nowhere to look, so nothing is read and the defaults stand.
#[must_use]
pub fn config_path(config_home: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    let base = config_home
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|home| home.join(XDG_FALLBACK)))?;
    Some(base.join(CONFIG_DIRECTORY).join(CONFIG_FILE))
}

/// Where the configuration file is, according to this process's environment.
///
/// The thin wrapper a face calls; [`config_path`] holds the decision.
#[must_use]
pub fn user_config_path() -> Option<PathBuf> {
    config_path(
        std::env::var_os(XDG_CONFIG_HOME)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        std::env::home_dir(),
    )
}

/// The directory the configuration file sits in, for a face that offers to
/// create one.
#[must_use]
pub fn config_directory(path: &Path) -> Option<&Path> {
    path.parent()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{config_directory, config_path};

    #[test]
    fn the_config_home_wins_when_it_is_set() {
        let path = config_path(
            Some(PathBuf::from("/opt/conf")),
            Some(PathBuf::from("/Users/someone")),
        );
        assert_eq!(path, Some(PathBuf::from("/opt/conf/iridium/config.toml")));
    }

    #[test]
    fn home_supplies_dot_config_when_the_config_home_is_unset() {
        let path = config_path(None, Some(PathBuf::from("/Users/someone")));
        assert_eq!(
            path,
            Some(PathBuf::from("/Users/someone/.config/iridium/config.toml"))
        );
    }

    #[test]
    fn a_relative_config_home_is_ignored_rather_than_resolved() {
        // The specification says to ignore it, and the reason bites here in
        // particular: a bundled application launched from Finder inherits `/`
        // as its working directory, so resolving `conf` against it would put
        // the user's configuration at `/conf` — somewhere they cannot write
        // and would never look.
        let path = config_path(
            Some(PathBuf::from("conf")),
            Some(PathBuf::from("/Users/someone")),
        );
        assert_eq!(
            path,
            Some(PathBuf::from("/Users/someone/.config/iridium/config.toml")),
            "a relative config home falls through to the home directory"
        );
    }

    #[test]
    fn nowhere_to_look_is_an_answer_rather_than_a_guess() {
        assert_eq!(config_path(None, None), None);
    }

    #[test]
    fn the_directory_is_the_files_parent() {
        let path = PathBuf::from("/Users/someone/.config/iridium/config.toml");
        assert_eq!(
            config_directory(&path),
            Some(PathBuf::from("/Users/someone/.config/iridium").as_path())
        );
    }
}
