//! Boundary tests: the conversions a browser cannot be asked to verify.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use iridium_editor::commands::builtin::{PALETTE_OPEN, default_registry};
use iridium_editor::commands::default_keymap_stack;
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::{
    CommandCategory, CommandId, CommandMeta, CommandRegistry, KeyHintIndex, KeymapStack,
};

use super::{PaletteCommand, key_hint, list, search, utf16_offsets};

fn hints() -> KeyHintIndex {
    KeyHintIndex::build(&default_keymap_stack())
}

fn registry() -> CommandRegistry {
    default_registry().expect("the kernel tables are consistent")
}

fn find<'a>(commands: &'a [PaletteCommand], id: &str) -> &'a PaletteCommand {
    commands
        .iter()
        .find(|command| command.id == id)
        .unwrap_or_else(|| panic!("`{id}` should be listed"))
}

// ===== The UTF-16 conversion =====

#[test]
fn ascii_positions_are_unchanged() {
    assert_eq!(utf16_offsets("Join Lines", &[0, 5, 7]), vec![0, 5, 7]);
    assert_eq!(utf16_offsets("Copy", &[0, 1, 2, 3]), vec![0, 1, 2, 3]);
    assert_eq!(utf16_offsets("Copy", &[]), Vec::<u32>::new());
}

#[test]
fn characters_outside_the_basic_plane_shift_later_offsets() {
    // The whole reason this conversion exists. `𝄞` is one Rust `char` and *two*
    // UTF-16 code units, so a browser slicing by the kernel's character index
    // would underline one position to the left of the intended glyph for every
    // astral character before it.
    let text = "𝄞ab";
    assert_eq!(text.chars().count(), 3);
    assert_eq!(text.encode_utf16().count(), 4);
    assert_eq!(utf16_offsets(text, &[0, 1, 2]), vec![0, 2, 3]);
}

#[test]
fn multi_byte_but_single_unit_characters_do_not_shift_anything() {
    // `ü` is two *bytes* and one UTF-16 unit: the byte length is irrelevant here,
    // and a conversion written against byte offsets would get this wrong in the
    // opposite direction.
    assert_eq!(utf16_offsets("über", &[0, 1, 3]), vec![0, 1, 3]);
}

#[test]
fn a_position_past_the_end_is_dropped_rather_than_guessed() {
    // Unreachable from a real match, since positions always come from the text
    // they index. Pinned because the alternative — clamping — would silently
    // highlight the wrong character instead of highlighting nothing.
    assert_eq!(utf16_offsets("ab", &[0, 9]), vec![0]);
    assert_eq!(utf16_offsets("", &[0]), Vec::<u32>::new());
}

// ===== Listing =====

#[test]
fn listing_covers_every_registered_command_in_browse_order() {
    let registry = registry();
    let commands = list(&registry, &hints(), false);
    // ⚠️ Against `palette_order`, not `len`: a mode-scoped command is
    // registered and deliberately not offered, because the panel it belongs to
    // is not open while a palette is. `registry.len()` counts both kinds.
    assert_eq!(commands.len(), registry.palette_order().len());

    let listed: Vec<&str> = commands.iter().map(|command| command.id.as_str()).collect();
    let expected: Vec<&str> = registry
        .palette_order()
        .iter()
        .map(|meta| meta.id().as_str())
        .collect();
    assert_eq!(listed, expected);

    // A listing has no query, so nothing claims a match.
    assert!(
        commands
            .iter()
            .all(|command| command.matched_field.is_none())
    );
    assert!(commands.iter().all(|command| command.matches.is_empty()));
    assert!(
        commands
            .iter()
            .all(|command| command.matched_text == command.title),
        "with nothing matched, the highlightable text is the title"
    );
}

#[test]
fn a_listing_carries_both_key_labels() {
    let commands = list(&registry(), &hints(), false);
    let copy = find(&commands, "clipboard.copy");
    assert_eq!(copy.key_hint.as_deref(), Some("Ctrl+C"));
    assert_eq!(
        copy.key_hint_mac.as_deref(),
        Some("⌘C"),
        "the web face forwards macOS Cmd as the kernel's ctrl, so the mac label says ⌘"
    );

    // An unbound command has neither, rather than an empty string a host would
    // render as a blank shortcut chip.
    let skip = find(&commands, "multiCursor.skipLastOccurrence");
    assert_eq!(skip.key_hint, None);
    assert_eq!(skip.key_hint_mac, None);
}

#[test]
fn the_palette_command_reports_that_the_kernel_does_not_implement_it() {
    let commands = list(&registry(), &hints(), false);
    let palette = find(&commands, PALETTE_OPEN.as_str());

    assert!(
        !palette.implemented,
        "`palette.open` is the host's to run; a face that ignores this flag would \
         call `runCommand` and get an error it did not expect"
    );
    assert_eq!(palette.key_hint.as_deref(), Some("Ctrl+K"));
    assert!(find(&commands, "lines.join").implemented);
}

#[test]
fn read_only_marks_mutating_commands_unavailable_without_hiding_them() {
    let registry = registry();
    let writable = list(&registry, &hints(), false);
    let read_only = list(&registry, &hints(), true);

    assert_eq!(
        writable.len(),
        read_only.len(),
        "a read-only buffer greys entries out; it does not shorten the palette"
    );
    assert!(writable.iter().all(|command| command.available));

    let join = find(&read_only, "lines.join");
    assert!(join.mutates_document);
    assert!(!join.available);

    let copy = find(&read_only, "clipboard.copy");
    assert!(!copy.mutates_document);
    assert!(copy.available, "copying changes nothing, so it stays live");
}

// ===== Searching =====

#[test]
fn a_search_carries_the_text_and_offsets_a_host_highlights() {
    let results = search(&registry(), &hints(), &CommandMru::new(), "yank", 0, false);
    let copy = find(&results, "clipboard.copy");

    assert_eq!(copy.matched_field.as_deref(), Some("alias"));
    assert_eq!(copy.matched_text, "yank");
    assert_eq!(copy.matches, vec![0, 1, 2, 3]);
    assert_eq!(copy.title, "Copy", "the title is still what a row shows");
}

#[test]
fn every_offset_a_search_returns_indexes_inside_its_own_text() {
    // The invariant the host slices on. Checked over the whole command set for
    // several query shapes, in UTF-16 units rather than characters.
    let registry = registry();
    let hints = hints();
    for query in ["e", "li", "cur", "select", "up", "palette"] {
        for command in search(&registry, &hints, &CommandMru::new(), query, 0, false) {
            let units = u32::try_from(command.matched_text.encode_utf16().count()).unwrap();
            assert!(
                command.matches.iter().all(|&offset| offset < units),
                "`{query}` on `{}` produced {:?}, outside {units} UTF-16 units",
                command.matched_text,
                command.matches
            );
            assert!(
                command.matches.windows(2).all(|pair| pair[0] < pair[1]),
                "offsets must ascend: {:?}",
                command.matches
            );
        }
    }
}

#[test]
fn the_limit_caps_the_result_without_changing_the_order() {
    let registry = registry();
    let hints = hints();
    let all = search(&registry, &hints, &CommandMru::new(), "line", 0, false);
    assert!(all.len() > 3);

    let capped = search(&registry, &hints, &CommandMru::new(), "line", 3, false);
    assert_eq!(capped.len(), 3);
    assert_eq!(capped, all[..3].to_vec());
}

#[test]
fn an_empty_query_searches_everything_rather_than_nothing() {
    let registry = registry();
    let results = search(&registry, &hints(), &CommandMru::new(), "", 0, false);
    // "Everything" is everything a palette offers — see the listing test.
    assert_eq!(results.len(), registry.palette_order().len());
}

#[test]
fn recency_reaches_the_wire() {
    let registry = registry();
    let hints = hints();
    let neutral = search(&registry, &hints, &CommandMru::new(), "cursor", 0, false);
    let candidate = neutral[1].id.clone();

    let mut mru = CommandMru::new();
    mru.record(&CommandId::new(candidate.clone()));
    let biased = search(&registry, &hints, &mru, "cursor", 0, false);

    assert_eq!(biased[0].id, candidate);
    assert!(biased[0].score > neutral[1].score);
}

#[test]
fn a_host_registered_command_is_searchable_and_labelled() {
    // A face contributes commands of its own; they must be indistinguishable from
    // kernel commands in the palette apart from the `implemented` flag.
    let mut registry = registry();
    registry
        .register(
            CommandMeta::new(
                CommandId::from_static("host.formatDocument"),
                "Format Document",
                CommandCategory::new("Host"),
            )
            .with_description("Runs the host's formatter."),
        )
        .expect("a fresh id registers");

    let results = search(&registry, &hints(), &CommandMru::new(), "format", 0, false);
    let formatter = find(&results, "host.formatDocument");
    assert_eq!(formatter.category, "Host");
    assert!(!formatter.implemented);
    assert_eq!(formatter.key_hint, None, "the host bound no key to it");
}

// ===== Key hints on their own =====

#[test]
fn a_key_hint_can_be_asked_for_by_id_in_either_style() {
    let hints = hints();
    assert_eq!(
        key_hint(&hints, "clipboard.copy", false).as_deref(),
        Some("Ctrl+C")
    );
    assert_eq!(
        key_hint(&hints, "clipboard.copy", true).as_deref(),
        Some("⌘C")
    );
    assert_eq!(
        key_hint(&hints, "multiCursor.skipLastOccurrence", false),
        None
    );
    assert_eq!(key_hint(&hints, "no.such.command", false), None);
}

#[test]
fn hints_follow_the_keymap_they_were_built_from() {
    // The reason `KeyHintIndex` is passed in rather than rebuilt here: a face that
    // pushed a user keymap must see the user's keys in its palette.
    let empty = KeyHintIndex::build(&KeymapStack::new());
    assert_eq!(key_hint(&empty, "clipboard.copy", false), None);

    let commands = list(&registry(), &empty, false);
    assert!(
        commands.iter().all(|command| command.key_hint.is_none()),
        "with no keymap, no entry may claim a shortcut"
    );
}

// ===== The wire format itself =====

#[test]
fn the_wire_format_is_camel_case_and_omits_what_is_absent() {
    let commands = list(&registry(), &hints(), false);
    let json = serde_json::to_string(find(&commands, "lines.join")).unwrap();

    assert!(json.contains(r#""mutatesDocument":true"#), "{json}");
    assert!(json.contains(r#""keyHint":"Ctrl+J""#), "{json}");
    assert!(json.contains(r#""keyHintMac":"⌘J""#), "{json}");
    assert!(json.contains(r#""matchedText":"Join Lines""#), "{json}");
    assert!(
        !json.contains("matchedField"),
        "an unmatched entry must not carry an empty match: {json}"
    );
    assert!(!json.contains(r#""matches""#), "{json}");

    // And it round-trips, so a host may cache a payload and replay it.
    let restored: PaletteCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(&restored, find(&commands, "lines.join"));
}

#[test]
fn the_whole_command_set_serializes() {
    // The wasm adapters fall back to `[]` when serialization fails, which a host
    // cannot distinguish from an editor with no commands. Nothing in
    // `PaletteCommand` *can* fail — it is strings, booleans and integers — and this
    // is what keeps that true if a field is ever added.
    let registry = registry();
    let hints = hints();

    let listed = serde_json::to_string(&list(&registry, &hints, false)).unwrap();
    assert!(listed.starts_with('['));
    assert!(listed.contains(PALETTE_OPEN.as_str()));

    for query in ["", "e", "join", "yank", "qqqq"] {
        serde_json::to_string(&search(
            &registry,
            &hints,
            &CommandMru::new(),
            query,
            0,
            false,
        ))
        .unwrap_or_else(|error| panic!("`{query}` failed to serialize: {error}"));
    }
}
