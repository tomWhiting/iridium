//! String-ish label types: palette categories and keymap modes.
//!
//! Both are deliberately *open* newtypes over [`Cow<'static, str>`](Cow) rather
//! than closed enums. A closed enum would make the kernel the sole authority on
//! what groups exist and what modes exist, which contradicts two of Iridium's
//! laws: hosts (including AI hosts) register their own commands through the
//! public API, and a modal keymap defines its own modes as data. Neither may
//! require a kernel edit.

use std::borrow::{Borrow, Cow};
use std::fmt;

use serde::{Deserialize, Serialize};

/// A palette grouping label for a command.
///
/// Purely presentational: it decides where a command appears in the command
/// palette, and has no effect on dispatch. Independent of the namespace in a
/// [`CommandId`](crate::commands::CommandId), which is a stable machine
/// identity — a category may be renamed or re-worded freely, an id may not.
///
/// The built-in categories are exposed as associated constants so the kernel's
/// own commands group consistently; hosts may use any label.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandCategory(Cow<'static, str>);

impl CommandCategory {
    /// Cursor movement that does not extend the selection.
    pub const NAVIGATION: Self = Self::from_static("Navigation");
    /// Selection changes, including selection-extending motions.
    pub const SELECTION: Self = Self::from_static("Selection");
    /// Text insertion and deletion.
    pub const EDITING: Self = Self::from_static("Editing");
    /// Whole-line operations.
    pub const LINES: Self = Self::from_static("Lines");
    /// Case conversion and line-block rearrangement.
    pub const TRANSFORM: Self = Self::from_static("Transform");
    /// Comment toggling.
    pub const COMMENTS: Self = Self::from_static("Comments");
    /// Clipboard transfer.
    pub const CLIPBOARD: Self = Self::from_static("Clipboard");
    /// Undo history navigation.
    pub const HISTORY: Self = Self::from_static("History");
    /// Multi-cursor and multi-selection verbs.
    pub const MULTI_CURSOR: Self = Self::from_static("Multi-Cursor");
    /// Search and find-match navigation.
    pub const SEARCH: Self = Self::from_static("Search");
    /// Commands with no home of their own, such as the explicit no-op.
    pub const GENERAL: Self = Self::from_static("General");

    /// Creates a category from a string literal without allocating.
    #[must_use]
    pub const fn from_static(label: &'static str) -> Self {
        Self(Cow::Borrowed(label))
    }

    /// Creates a category from any string-like value.
    #[must_use]
    pub fn new(label: impl Into<Cow<'static, str>>) -> Self {
        Self(label.into())
    }

    /// Returns the label as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Borrow<str> for CommandCategory {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<&'static str> for CommandCategory {
    fn from(label: &'static str) -> Self {
        Self::from_static(label)
    }
}

impl From<String> for CommandCategory {
    fn from(label: String) -> Self {
        Self(Cow::Owned(label))
    }
}

/// The name of an editing mode a keymap can scope its bindings to.
///
/// A mode is **data owned by the keymap layer**, not a kernel concept: the
/// kernel knows only that a binding may name a mode and that a resolver has at
/// most one mode active. A Vim-grammar keymap therefore ships its own
/// `"normal"`, `"insert"`, `"visual"` and operator-pending modes as strings in
/// its own file, and the kernel never grows a `VimMode` enum.
///
/// A binding with no mode (`None`) applies in **every** mode, which is what
/// makes the non-modal default keymap mode-free. A binding naming a mode applies
/// only while [`KeymapResolver::mode`](crate::commands::KeymapResolver::mode)
/// equals it, and takes precedence over a mode-free binding for the same
/// sequence in the same layer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModeName(Cow<'static, str>);

impl ModeName {
    /// Creates a mode name from a string literal without allocating.
    #[must_use]
    pub const fn from_static(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    /// Creates a mode name from any string-like value.
    #[must_use]
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self(name.into())
    }

    /// Returns the mode name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Borrow<str> for ModeName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<&'static str> for ModeName {
    fn from(name: &'static str) -> Self {
        Self::from_static(name)
    }
}

impl From<String> for ModeName {
    fn from(name: String) -> Self {
        Self(Cow::Owned(name))
    }
}
