//! The search-and-replace panel's vocabulary — one screen, thirteen verbs.
//!
//! Two text fields, a set of options and a way out. One mode, because the two
//! fields are one screen: `Tab` moves the focus between them and every other key
//! means the same thing in both, so a second mode would double the table to say
//! nothing new.
//!
//! # ⚠️ Next and previous match are **not** here, and that is deliberate
//!
//! [`SEARCH_NEXT_MATCH`](super::super::SEARCH_NEXT_MATCH) and
//! [`SEARCH_PREVIOUS_MATCH`](super::super::SEARCH_PREVIOUS_MATCH) already exist
//! as mode-free document commands, and the panel's `Enter` *is* that command —
//! it calls [`Editor::goto_next_match`](crate::Editor::goto_next_match), the
//! same verb the document's own binding calls. Minting a scoped twin would give
//! a user two ids for one behaviour and put both in the palette.
//!
//! The `palette.open` / `palette.dismiss` split points the other way and does
//! not apply here: those are two *different* actions that happen to share a
//! chord, which is exactly why they need separate ids. This is one action
//! reached from two places.
//!
//! The consequence is that a face's search panel answers a vocabulary made of
//! this table **plus two borrowed document ids**, and its own tests are what
//! keep that seam honest — see the terminal panel's `keymap_tests`.
//!
//! # ⚠️ `search.open` is not here either, for the palette's reason
//!
//! Opening the panel is something the editor can be asked from anywhere, so it
//! stays mode-free. [`SEARCH_DISMISS`] is the panel's own way out, and the two
//! share a chord without sharing an id — which is what lets a user rebind
//! either one alone.

use crate::commands::{CommandCategory, CommandId, CommandMeta, ModeName};

/// The mode the search-and-replace panel is in whenever it is open.
pub const SEARCH_MODE: ModeName = ModeName::from_static("search");

/// Close the search panel, ending the search.
pub const SEARCH_DISMISS: CommandId = CommandId::from_static("search.dismiss");

/// Move the focus between the query field and the replacement field.
pub const SEARCH_TOGGLE_FIELD: CommandId = CommandId::from_static("search.toggleField");

/// Move the caret one character left within the focused field.
pub const SEARCH_CARET_LEFT: CommandId = CommandId::from_static("search.caretLeft");

/// Move the caret one character right within the focused field.
pub const SEARCH_CARET_RIGHT: CommandId = CommandId::from_static("search.caretRight");

/// Move the caret to the start of the focused field.
pub const SEARCH_CARET_HOME: CommandId = CommandId::from_static("search.caretHome");

/// Move the caret to the end of the focused field.
pub const SEARCH_CARET_END: CommandId = CommandId::from_static("search.caretEnd");

/// Delete the character before the caret in the focused field.
pub const SEARCH_FIELD_BACKSPACE: CommandId = CommandId::from_static("search.fieldBackspace");

/// Delete the character after the caret in the focused field.
pub const SEARCH_FIELD_DELETE: CommandId = CommandId::from_static("search.fieldDelete");

/// Replace the match the document is currently sitting on.
pub const SEARCH_REPLACE_CURRENT: CommandId = CommandId::from_static("search.replaceCurrent");

/// Replace every match in the document.
pub const SEARCH_REPLACE_ALL: CommandId = CommandId::from_static("search.replaceAll");

/// Turn case-sensitive matching on or off and re-run the search.
pub const SEARCH_TOGGLE_CASE_SENSITIVE: CommandId =
    CommandId::from_static("search.toggleCaseSensitive");

/// Turn whole-word matching on or off and re-run the search.
pub const SEARCH_TOGGLE_WHOLE_WORD: CommandId = CommandId::from_static("search.toggleWholeWord");

/// Turn regular-expression matching on or off and re-run the search.
pub const SEARCH_TOGGLE_REGEX: CommandId = CommandId::from_static("search.toggleRegex");

/// Every search-panel command, in declaration order.
///
/// Ordered as the panel's own documentation table lists them — the way out
/// first, then the fields, then the replacements, then the options — so a reader
/// comparing the two is comparing like with like.
pub(super) static SEARCH: &[CommandMeta] = &[
    CommandMeta::scoped(
        SEARCH_DISMISS,
        "Close Search",
        "Closes the search panel and ends the search.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_TOGGLE_FIELD,
        "Switch Search Field",
        "Moves the focus between the search query and the replacement text.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_CARET_LEFT,
        "Search Caret Left",
        "Moves the caret one character left within the focused field.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_CARET_RIGHT,
        "Search Caret Right",
        "Moves the caret one character right within the focused field.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_CARET_HOME,
        "Search Caret To Start",
        "Moves the caret to the start of the focused field.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_CARET_END,
        "Search Caret To End",
        "Moves the caret to the end of the focused field.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_FIELD_BACKSPACE,
        "Search Delete Backwards",
        "Deletes the character before the caret in the focused field.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_FIELD_DELETE,
        "Search Delete Forwards",
        "Deletes the character after the caret in the focused field.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_REPLACE_CURRENT,
        "Replace Current Match",
        "Replaces the match the document is currently sitting on.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_REPLACE_ALL,
        "Replace All Matches",
        "Replaces every match in the document.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_TOGGLE_CASE_SENSITIVE,
        "Toggle Case Sensitive",
        "Turns case-sensitive matching on or off and re-runs the search.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_TOGGLE_WHOLE_WORD,
        "Toggle Whole Word",
        "Turns whole-word matching on or off and re-runs the search.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
    CommandMeta::scoped(
        SEARCH_TOGGLE_REGEX,
        "Toggle Regular Expression",
        "Turns regular-expression matching on or off and re-runs the search.",
        CommandCategory::SEARCH,
        SEARCH_MODE,
    ),
];
