//! Which field names a type's own `Deserialize` implementation recognises.
//!
//! # Why this is not a list
//!
//! Reporting `tab_widht = 2` as a typo needs the set of settings that exist.
//! Writing that set out by hand would be a *proxy* for the real one: it would
//! agree with [`EditorConfig`](iridium_editor::EditorConfig) exactly until the
//! day a field was added to the struct and not to the list, at which point a
//! real setting would be reported as a typo and ignored — the loader
//! confidently wrong rather than merely silent.
//!
//! So the set is taken from the same derive that does the deserializing.
//! `serde`'s generated code hands its field names to
//! `Deserializer::deserialize_struct`, and that is where they are read from.
//! Adding a field to the struct updates this automatically because it is not a
//! second copy of anything.
//!
//! # The one case where it cannot answer
//!
//! A struct whose derive uses `#[serde(flatten)]` deserializes as a *map*, not
//! a struct, and has no field list to hand over. [`field_names`] answers `None`
//! there rather than guessing, and the caller reports nothing rather than
//! reporting every key as unknown. A test in [`crate::settings`] asserts the
//! answer for `EditorConfig` is `Some`, so introducing a flatten is a failing
//! test rather than a silent loss of the check.

use core::fmt;

use serde::Deserialize;
use serde::de::{self, Visitor};

/// The field names `T`'s derive asks for, or `None` when it does not ask.
///
/// Deserialization is deliberately abandoned the instant the names are in
/// hand: nothing is being deserialized here, and running a visitor over an
/// empty input to find out would answer a different question.
#[must_use]
pub fn field_names<'de, T: Deserialize<'de>>() -> Option<&'static [&'static str]> {
    let mut captured = None;
    drop(T::deserialize(Capture(&mut captured)));
    captured
}

/// The error every method of [`Capture`] returns.
///
/// A unit type because it is never reported: it exists only to unwind out of
/// `Deserialize` once the field names have been taken, and the caller discards
/// it. Carrying a message would imply somebody reads one.
#[derive(Debug)]
struct Halt;

impl fmt::Display for Halt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("field-name capture is not a deserializer")
    }
}

impl core::error::Error for Halt {}

impl de::Error for Halt {
    fn custom<T: fmt::Display>(_message: T) -> Self {
        Self
    }
}

/// A deserializer that records a struct's field names and then gives up.
struct Capture<'a>(&'a mut Option<&'static [&'static str]>);

impl<'de> de::Deserializer<'de> for Capture<'_> {
    type Error = Halt;

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(Halt)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        *self.0 = Some(fields);
        Err(Halt)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map enum identifier ignored_any
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::field_names;

    #[derive(Deserialize)]
    #[expect(dead_code, reason = "only the derive's field list is read")]
    struct Plain {
        first: u8,
        second: String,
    }

    #[derive(Deserialize)]
    #[expect(dead_code, reason = "only the derive's field list is read")]
    struct Renamed {
        #[serde(rename = "tab-width")]
        tab_width: u8,
    }

    #[test]
    fn a_struct_hands_over_its_field_names() {
        assert_eq!(field_names::<Plain>(), Some(&["first", "second"][..]));
    }

    #[test]
    fn the_names_are_the_ones_the_file_uses_not_the_ones_rust_uses() {
        // A renamed field is written in the file under its serde name, so that
        // is the name a typo has to be compared against. Reading the Rust
        // identifier would report every renamed setting as unknown.
        assert_eq!(field_names::<Renamed>(), Some(&["tab-width"][..]));
    }

    #[test]
    fn a_type_that_is_not_a_struct_answers_none_rather_than_guessing() {
        assert_eq!(field_names::<Vec<u8>>(), None);
        assert_eq!(field_names::<String>(), None);
    }
}
