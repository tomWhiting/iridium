//! The document's text at one revision, shared rather than copied.
//!
//! A rope clone shares every node with the original, so taking a snapshot is
//! O(1) whatever the document's size. The editor's next edit copies only the
//! path it changes, and only while a snapshot is still held.

use std::fmt;

use ropey::Rope;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Document;

/// The text of a document at one revision.
///
/// What [`EditorEvent::ContentChanged`](crate::EditorEvent::ContentChanged)
/// carries. A listener that wants a `String` pays for one with
/// [`Self::text`]; one that only wanted to know *that* the text changed pays
/// nothing.
#[derive(Clone)]
pub struct TextSnapshot {
    rope: Rope,
    revision: u64,
}

impl TextSnapshot {
    /// The text `document` holds now. O(1).
    #[must_use]
    pub fn of(document: &Document) -> Self {
        Self {
            rope: document.rope().clone(),
            revision: document.revision(),
        }
    }

    /// The document revision this text was taken at.
    ///
    /// Zero for a snapshot built from deserialized text, which has no document
    /// behind it.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// The length of the text in bytes. O(1).
    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.rope.len()
    }

    /// Whether the text is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rope.len() == 0
    }

    /// The rope itself, for reading by line or byte range without a copy.
    #[must_use]
    pub const fn rope(&self) -> &Rope {
        &self.rope
    }

    /// The whole text as a `String`. O(document): call it only when a copy is
    /// what you need.
    #[must_use]
    pub fn text(&self) -> String {
        self.rope.to_string()
    }
}

impl fmt::Display for TextSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.rope, f)
    }
}

/// A summary, not the text: a debug print of an event must not be O(document).
impl fmt::Debug for TextSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextSnapshot")
            .field("revision", &self.revision)
            .field("len_bytes", &self.rope.len())
            .finish()
    }
}

impl PartialEq<str> for TextSnapshot {
    fn eq(&self, other: &str) -> bool {
        if self.rope.len() != other.len() {
            return false;
        }
        let mut rest = other.as_bytes();
        for chunk in self.rope.chunks() {
            let Some((head, tail)) = rest.split_at_checked(chunk.len()) else {
                return false;
            };
            if head != chunk.as_bytes() {
                return false;
            }
            rest = tail;
        }
        rest.is_empty()
    }
}

impl PartialEq<&str> for TextSnapshot {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

/// Serialized as the plain text, so the wire form of `ContentChanged` is the
/// string it always was.
impl Serialize for TextSnapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.rope)
    }
}

impl<'de> Deserialize<'de> for TextSnapshot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(Self {
            rope: Rope::from(text),
            revision: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::document::Position;

    #[test]
    fn a_snapshot_reads_the_text_it_was_taken_at() {
        let mut document = Document::new("one\ntwo");
        let snapshot = TextSnapshot::of(&document);
        document
            .insert(Position::new(1, 3), "!")
            .expect("the end of the last line is a valid position");
        assert_eq!(snapshot, "one\ntwo");
        assert_eq!(snapshot.text(), "one\ntwo");
        assert_eq!(snapshot.len_bytes(), 7);
        assert_eq!(TextSnapshot::of(&document), "one\ntwo!");
        assert!(TextSnapshot::of(&document).revision() > snapshot.revision());
    }

    #[test]
    fn comparison_with_text_checks_every_byte() {
        let snapshot = TextSnapshot::of(&Document::new("abc"));
        assert_eq!(snapshot, "abc");
        assert_ne!(snapshot, "abd");
        assert_ne!(snapshot, "ab");
        assert_ne!(snapshot, "abcd");
        assert!(TextSnapshot::of(&Document::new("")).is_empty());
    }

    #[test]
    fn a_debug_print_does_not_carry_the_text() {
        let snapshot = TextSnapshot::of(&Document::new("secret"));
        let printed = format!("{snapshot:?}");
        assert!(!printed.contains("secret"), "{printed}");
        assert!(printed.contains("len_bytes: 6"), "{printed}");
    }

    #[test]
    fn the_wire_form_is_the_plain_text() {
        let snapshot = TextSnapshot::of(&Document::new("a \"quoted\"\nline"));
        let json = serde_json::to_string(&snapshot).expect("a snapshot serializes");
        assert_eq!(json, r#""a \"quoted\"\nline""#);
        let back: TextSnapshot = serde_json::from_str(&json).expect("and deserializes");
        assert_eq!(back, "a \"quoted\"\nline");
        assert_eq!(back.revision(), 0);
    }
}
