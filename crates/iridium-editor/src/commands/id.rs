//! The stable machine identity of a registered command.

use std::borrow::{Borrow, Cow};
use std::fmt;

use serde::{Deserialize, Serialize};

/// A stable, machine-facing command identifier.
///
/// # Compatibility surface
///
/// A `CommandId` is part of Iridium's **public compatibility surface**: keymaps
/// on disk, macros, and AI hosts driving the editor through the public API all
/// reference commands by this string. Ids are therefore treated like a wire
/// format — they may be *added* freely, but renaming or removing one is a
/// breaking change and must go through a deprecation alias, never a silent edit.
///
/// The canonical form is `namespace.lowerCamelCase`, e.g. `cursor.wordLeft`,
/// `lines.duplicateUp`, `multiCursor.addCursorAbove`. The namespace groups a
/// family of related commands and is independent of the palette
/// [`category`](crate::commands::CommandMeta::category), which is presentation.
///
/// # Representation
///
/// The id wraps a [`Cow<'static, str>`](Cow):
///
/// - Commands compiled into the kernel use [`CommandId::from_static`], a `const`
///   constructor yielding the borrowed variant. Cloning one copies a pointer and
///   a length — no allocation, which is what keeps keymap resolution allocation
///   free on the keystroke hot path.
/// - Ids arriving from a deserialized keymap or from a host registering its own
///   command own their storage. Such an id still compares and hashes
///   identically to the static form, so it works everywhere; but a keymap loaded
///   from configuration should be passed through
///   [`Keymap::canonicalize`](crate::commands::Keymap::canonicalize), which
///   swaps every owned id for the registry's static one. That both validates
///   the ids against the registry and restores the allocation-free hot path.
///
/// Equality, ordering and hashing are all delegated to the underlying string, so
/// borrowed and owned ids with the same text are interchangeable as map keys, and
/// a [`std::collections::HashMap<CommandId, _>`] can be probed with a plain
/// `&str`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(Cow<'static, str>);

impl CommandId {
    /// Creates an id from a string literal without allocating.
    ///
    /// This is the constructor used for every command compiled into the kernel;
    /// see [`crate::commands::builtin`].
    #[must_use]
    pub const fn from_static(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    /// Creates an id from any string-like value.
    ///
    /// Accepts both `&'static str` (borrowed, allocation free) and [`String`]
    /// (owned), so hosts registering commands at run time and deserialized
    /// keymaps share one type with the compiled-in ids.
    #[must_use]
    pub fn new(id: impl Into<Cow<'static, str>>) -> Self {
        Self(id.into())
    }

    /// Returns the id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` when this id borrows `'static` storage.
    ///
    /// A keymap whose bindings all report `true` resolves without allocating.
    /// [`Keymap::canonicalize`](crate::commands::Keymap::canonicalize) is what
    /// converts owned ids into this form.
    #[must_use]
    pub const fn is_static(&self) -> bool {
        matches!(self.0, Cow::Borrowed(_))
    }

    /// Returns `true` when the id is empty, which the registry rejects.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Borrow<str> for CommandId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CommandId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&'static str> for CommandId {
    fn from(id: &'static str) -> Self {
        Self::from_static(id)
    }
}

impl From<String> for CommandId {
    fn from(id: String) -> Self {
        Self(Cow::Owned(id))
    }
}

impl PartialEq<str> for CommandId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for CommandId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}
