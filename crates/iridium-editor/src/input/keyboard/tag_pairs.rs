//! Named tags close only after a complete, unquoted opening delimiter.
use crate::document::{Document, Position};

pub(super) fn closing_edit(document: &Document, position: Position) -> Option<String> {
    let language = document.language_at(position)?;
    if !matches!(language.id(), "xml" | "html") {
        return None;
    }
    let end = document.position_to_offset(position)?;
    let mut start = end.saturating_sub(16_384);
    while start < end && !document.rope().is_char_boundary(start) {
        start += 1;
    }
    let prefix = document.rope().slice(start..end).to_string();
    let mut opening = None;
    let mut quote = None;
    for (byte, character) in prefix.char_indices() {
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            }
            continue;
        }
        match character {
            '<' => opening = Some(byte + 1),
            '>' => opening = None,
            '\'' | '"' if opening.is_some() => quote = Some(character),
            _ => {},
        }
    }
    if quote.is_some() {
        return None;
    }
    let body = &prefix[opening?..];
    if body.ends_with('/') {
        return None;
    }
    let name = body.split_whitespace().next()?;
    let mut characters = name.chars();
    let first = characters.next()?;
    if !(first.is_alphabetic() || first == '_' || first == ':')
        || !characters.all(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '-' | '.'))
    {
        return None;
    }
    if language.id() == "html" && is_void(name) {
        return None;
    }
    let closer = format!("</{name}>");
    let following_end = end.saturating_add(closer.len()).min(document.byte_count());
    if document.rope().is_char_boundary(following_end)
        && document.rope().slice(end..following_end) == closer
    {
        return None;
    }
    Some(format!(">{closer}"))
}

fn is_void(name: &str) -> bool {
    [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ]
    .iter()
    .any(|element| name.eq_ignore_ascii_case(element))
}
