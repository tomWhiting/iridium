//! The command-palette wire shape, and the logic that fills it.
//!
//! Deliberately **not** gated on `feature = "web"`. Everything here is plain
//! functions over borrowed kernel types, so `cargo test` exercises it on the host
//! target; only the three-line `#[wasm_bindgen]` adapters in `wasm.rs` are
//! browser-only. A conversion bug that can only be reproduced in a browser is a
//! conversion bug nobody reproduces.
//!
//! # What crosses the boundary
//!
//! One JSON payload per keystroke, over roughly fifty commands. The match itself
//! is microseconds and allocation-free; this serialization is the expensive half,
//! and both are far inside a frame.
//!
//! Two things are computed **here** rather than in TypeScript, because getting
//! either wrong is silent:
//!
//! - **Key labels**, in both the portable and the macOS spelling. The kernel owns
//!   the mapping from a binding to a label — including the fact that the web face
//!   forwards macOS `Cmd` as the kernel's `ctrl` — and a host re-deriving it would
//!   drift.
//! - **Match positions, converted to UTF-16 offsets.** The kernel indexes by
//!   character, which is right for Rust and wrong for a browser: JavaScript string
//!   indices are UTF-16 code units, so any title outside the basic plane would
//!   highlight the wrong characters. Converting at the boundary means TypeScript
//!   never has to know.

use iridium_editor::commands::palette::{CommandMru, PaletteEntry, search_text};
use iridium_editor::{CommandMeta, CommandRegistry, Editor, KeyHintIndex, KeyLabelStyle};
use serde::{Deserialize, Serialize};

/// One command as a palette renders it.
///
/// `camelCase` on the wire; the field names are the TypeScript surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteCommand {
    /// The stable id, and what a host passes back to run the command.
    pub id: String,
    /// The human-facing title.
    pub title: String,
    /// The longer explanation, when the command has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The palette grouping label.
    pub category: String,
    /// The key sequence that runs it, portable spelling (`Ctrl+K`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_hint: Option<String>,
    /// The same sequence in macOS glyphs (`⌘K`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_hint_mac: Option<String>,
    /// Advisory: running it can change document text.
    pub mutates_document: bool,
    /// Whether it can run *right now* — false for a mutating command in a
    /// read-only buffer, which a palette should grey out rather than hide.
    pub available: bool,
    /// Whether the kernel implements it, as opposed to reporting it to the host.
    pub implemented: bool,
    /// The rank score. Meaningless in absolute terms; only the order matters.
    pub score: i32,
    /// Which text matched: `title`, `alias`, `id`, `category` or `description`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_field: Option<String>,
    /// The exact text [`Self::matches`] indexes into.
    ///
    /// Carried because a match on an alias cannot be highlighted inside the
    /// title. A host renders this string when it wants to show *why* an entry
    /// matched, and falls back to the title when nothing matched.
    pub matched_text: String,
    /// The matched positions within [`Self::matched_text`], as **UTF-16 offsets**,
    /// ready to slice a JavaScript string.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<u32>,
}

/// Converts character positions into UTF-16 offsets within `text`.
///
/// One pass, positions ascending. A position past the end of `text` cannot arise
/// from a match against that same text; if one ever does it is dropped rather
/// than clamped, because a highlight in the wrong place is worse than none.
fn utf16_offsets(text: &str, positions: &[u32]) -> Vec<u32> {
    if positions.is_empty() {
        return Vec::new();
    }
    let mut offsets = Vec::with_capacity(positions.len());
    let mut next = 0;
    let mut utf16 = 0_u32;
    for (index, character) in text.chars().enumerate() {
        if next == positions.len() {
            break;
        }
        if positions[next] as usize == index {
            offsets.push(utf16);
            next += 1;
        }
        utf16 += u32::try_from(character.len_utf16()).unwrap_or(0);
    }
    offsets
}

/// Fills the fields every entry has, matched or not.
fn describe(meta: &CommandMeta, hints: &KeyHintIndex, read_only: bool) -> PaletteCommand {
    let id = meta.id().as_str().to_owned();
    let hint = hints.primary_hint(&id);
    let mutates_document = meta.mutates_document();

    PaletteCommand {
        title: meta.title().to_owned(),
        description: meta.description().map(str::to_owned),
        category: meta.category().as_str().to_owned(),
        key_hint: hint.map(|hint| hint.label(KeyLabelStyle::Portable).to_owned()),
        key_hint_mac: hint.map(|hint| hint.label(KeyLabelStyle::MacGlyphs).to_owned()),
        mutates_document,
        available: !(mutates_document && read_only),
        implemented: Editor::implements_command(&id),
        score: 0,
        matched_field: None,
        matched_text: meta.title().to_owned(),
        matches: Vec::new(),
        id,
    }
}

/// Converts one scored result into its wire shape.
fn convert(entry: &PaletteEntry<'_>, hints: &KeyHintIndex, read_only: bool) -> PaletteCommand {
    PaletteCommand {
        score: entry.score(),
        matched_field: entry.matched_field().map(|field| field.as_str().to_owned()),
        matched_text: entry.matched_text().to_owned(),
        matches: utf16_offsets(entry.matched_text(), entry.matches()),
        ..describe(entry.meta(), hints, read_only)
    }
}

/// Every registered command, in the registry's browse order.
///
/// This is [`CommandRegistry::palette_order`] — grouped by category — rather than
/// the ranked order [`search`] returns, because a palette opened with no query is
/// being *browsed*, and category grouping is what a reader wants.
#[must_use]
pub fn list(
    registry: &CommandRegistry,
    hints: &KeyHintIndex,
    read_only: bool,
) -> Vec<PaletteCommand> {
    registry
        .palette_order()
        .into_iter()
        .map(|meta| describe(meta, hints, read_only))
        .collect()
}

/// Searches for `query`, ranked, with recency breaking near ties.
///
/// `limit` of zero means no limit, matching the JavaScript convention where the
/// argument is optional and absent means "everything".
#[must_use]
pub fn search(
    registry: &CommandRegistry,
    hints: &KeyHintIndex,
    mru: &CommandMru,
    query: &str,
    limit: usize,
    read_only: bool,
) -> Vec<PaletteCommand> {
    let limit = (limit > 0).then_some(limit);
    search_text(registry, mru, query, limit)
        .iter()
        .map(|entry| convert(entry, hints, read_only))
        .collect()
}

/// The key sequence that runs `id`, or `None` when nothing is bound to it.
#[must_use]
pub fn key_hint(hints: &KeyHintIndex, id: &str, mac_glyphs: bool) -> Option<String> {
    let style = if mac_glyphs {
        KeyLabelStyle::MacGlyphs
    } else {
        KeyLabelStyle::Portable
    };
    hints
        .primary_hint(id)
        .map(|hint| hint.label(style).to_owned())
}

#[cfg(test)]
mod tests;
