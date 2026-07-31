//! Serde tests: keymaps must survive a round trip through configuration.

// `fn_params_excessive_bools` is allowed for the `mods` helper only: its five
// parameters mirror the five hardware modifier bits of `Modifiers` one for one,
// exactly as that struct's own documented allow does.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::fn_params_excessive_bools
)]

use super::builtin::builtin_registry;
use super::{
    CommandCategory, CommandId, CommandMeta, KeyBinding, KeyPress, Keymap, KeymapResolver,
    KeymapStack, ModeName, ModifierPattern, ModifierState, Resolution, StrokePattern,
    default_non_modal_keymap,
};
use crate::input::{KeyCode, Modifiers};

const fn mods(shift: bool, ctrl: bool, alt: bool, meta: bool, alt_graph: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        meta,
        alt_graph,
    }
}

fn ctrl_pattern(key: KeyCode) -> StrokePattern {
    StrokePattern::new(
        key,
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    )
}

#[test]
fn the_default_keymap_round_trips_byte_for_byte() {
    let original = default_non_modal_keymap();
    let json = serde_json::to_string(&original).unwrap();
    let restored: Keymap = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.name(), original.name());
    assert_eq!(restored.len(), original.len());
    // Equality covers the rebuilt first-stroke index as well as the bindings.
    assert_eq!(restored, original);
    assert_eq!(serde_json::to_string(&restored).unwrap(), json);
}

#[test]
fn a_round_tripped_keymap_resolves_identically() {
    let original = default_non_modal_keymap();
    let json = serde_json::to_string(&original).unwrap();
    let restored: Keymap = serde_json::from_str(&json).unwrap();

    let before = KeymapStack::with_base(original);
    let after = KeymapStack::with_base(restored);

    let probes = [
        KeyPress::new(KeyCode::Char('c'), mods(false, true, false, false, false)),
        KeyPress::new(KeyCode::Up, mods(false, false, true, false, false)),
        KeyPress::new(KeyCode::Up, mods(false, true, true, false, true)),
        KeyPress::new(KeyCode::Backspace, mods(true, false, false, false, false)),
        KeyPress::plain(KeyCode::Enter),
    ];
    for probe in probes {
        let mut left = KeymapResolver::new();
        let mut right = KeymapResolver::new();
        assert_eq!(
            left.resolve(&before, probe),
            right.resolve(&after, probe),
            "divergent resolution for {probe:?}"
        );
    }
}

#[test]
fn a_keymap_stack_round_trips_with_its_layer_order() {
    let mut stack = KeymapStack::with_base(default_non_modal_keymap());
    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::from_static("comment.toggleLine"),
    ));
    user.push(KeyBinding::unbound(ctrl_pattern(KeyCode::Char('j')), &[]));
    stack.push(user);

    let json = serde_json::to_string(&stack).unwrap();
    let restored: KeymapStack = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.len(), 2);
    assert_eq!(restored.layers()[0].name(), "default");
    assert_eq!(restored.layers()[1].name(), "user");

    // The override and the suppression both survive.
    let ctrl_c = [KeyPress::new(
        KeyCode::Char('c'),
        mods(false, true, false, false, false),
    )];
    assert_eq!(
        restored
            .exact_match(&ctrl_c, None)
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "comment.toggleLine"
    );
    let ctrl_j = [KeyPress::new(
        KeyCode::Char('j'),
        mods(false, true, false, false, false),
    )];
    assert!(
        restored
            .exact_match(&ctrl_j, None)
            .unwrap()
            .command()
            .is_none()
    );
}

#[test]
fn a_binding_deserializes_from_a_minimal_json_shape() {
    let json = r#"{"keys":[{"key":{"Char":"s"},"modifiers":{"ctrl":"required"}}],"command":"clipboard.copy"}"#;
    let binding: KeyBinding = serde_json::from_str(json).unwrap();

    assert_eq!(binding.sequence().len(), 1);
    assert_eq!(binding.sequence()[0].key, KeyCode::Char('s'));
    assert_eq!(binding.command().unwrap().as_str(), "clipboard.copy");
    assert!(binding.mode().is_none());

    // Unmentioned modifiers default to Forbidden, so the chord is exact and the
    // AltGr guard is present without the author writing it.
    let pattern = binding.sequence()[0].modifiers;
    assert_eq!(pattern.ctrl, ModifierState::Required);
    assert_eq!(pattern.shift, ModifierState::Forbidden);
    assert_eq!(pattern.alt, ModifierState::Forbidden);
    assert_eq!(pattern.meta, ModifierState::Forbidden);
    assert_eq!(pattern.alt_graph, ModifierState::Forbidden);
    assert!(pattern.matches(mods(false, true, false, false, false)));
    assert!(!pattern.matches(mods(false, true, true, false, true)));
}

#[test]
fn a_multi_stroke_binding_and_a_mode_survive_json() {
    let json = r#"{
        "keys": [
            {"key": {"Char": "k"}, "modifiers": {"ctrl": "required"}},
            {"key": {"Char": "d"}, "modifiers": {"ctrl": "required"}}
        ],
        "command": "multiCursor.skipLastOccurrence",
        "mode": "normal"
    }"#;
    let binding: KeyBinding = serde_json::from_str(json).unwrap();
    assert_eq!(binding.sequence().len(), 2);
    assert_eq!(binding.mode(), Some(&ModeName::from_static("normal")));
    assert_eq!(binding.display_sequence(), "ctrl+k ctrl+d");
}

#[test]
fn an_omitted_command_is_a_suppression() {
    let json = r#"{"keys":[{"key":{"Char":"c"},"modifiers":{"ctrl":"required"}}]}"#;
    let binding: KeyBinding = serde_json::from_str(json).unwrap();
    assert!(binding.command().is_none());

    let explicit =
        r#"{"keys":[{"key":{"Char":"c"},"modifiers":{"ctrl":"required"}}],"command":null}"#;
    let binding: KeyBinding = serde_json::from_str(explicit).unwrap();
    assert!(binding.command().is_none());
}

#[test]
fn an_empty_key_sequence_is_rejected() {
    let json = r#"{"keys":[],"command":"clipboard.copy"}"#;
    let error = serde_json::from_str::<KeyBinding>(json).unwrap_err();
    assert!(
        error.to_string().contains("empty key sequence"),
        "unexpected error: {error}"
    );
}

#[test]
fn deserialized_ids_are_owned_until_canonicalized() {
    let json = r#"{
        "name": "user",
        "bindings": [
            {"keys":[{"key":{"Char":"c"},"modifiers":{"ctrl":"required"}}],"command":"clipboard.copy"}
        ]
    }"#;
    let mut keymap: Keymap = serde_json::from_str(json).unwrap();
    assert!(
        !keymap.bindings()[0].command().unwrap().is_static(),
        "a deserialized id owns its storage"
    );

    let registry = builtin_registry().unwrap();
    keymap.validate(&registry).unwrap();
    keymap.canonicalize(&registry).unwrap();
    assert!(keymap.bindings()[0].command().unwrap().is_static());

    // And it still resolves after canonicalization.
    let stack = KeymapStack::with_base(keymap);
    let mut resolver = KeymapResolver::new();
    assert_eq!(
        resolver.resolve(
            &stack,
            KeyPress::new(KeyCode::Char('c'), mods(false, true, false, false, false))
        ),
        Resolution::matched(CommandId::from_static("clipboard.copy"))
    );
}

#[test]
fn a_keymap_referencing_an_unknown_command_is_caught_at_load_time() {
    let json = r#"{
        "name": "typo",
        "bindings": [
            {"keys":[{"key":{"Char":"c"},"modifiers":{"ctrl":"required"}}],"command":"clipbord.copy"}
        ]
    }"#;
    let keymap: Keymap = serde_json::from_str(json).unwrap();
    let registry = builtin_registry().unwrap();
    let error = keymap.validate(&registry).unwrap_err();
    assert!(error.to_string().contains("clipbord.copy"), "{error}");
}

#[test]
fn modifier_states_use_stable_lowercase_names() {
    assert_eq!(
        serde_json::to_string(&ModifierState::Required).unwrap(),
        "\"required\""
    );
    assert_eq!(
        serde_json::to_string(&ModifierState::Forbidden).unwrap(),
        "\"forbidden\""
    );
    assert_eq!(
        serde_json::to_string(&ModifierState::Any).unwrap(),
        "\"any\""
    );
    assert_eq!(
        serde_json::from_str::<ModifierState>("\"any\"").unwrap(),
        ModifierState::Any
    );
}

#[test]
fn forbidden_modifiers_are_omitted_from_the_serialized_form() {
    let pattern = ModifierPattern::NONE.with_ctrl(ModifierState::Required);
    let json = serde_json::to_string(&pattern).unwrap();
    assert_eq!(json, r#"{"ctrl":"required"}"#);

    let all_any = serde_json::to_string(&ModifierPattern::ANY).unwrap();
    assert_eq!(
        all_any,
        r#"{"shift":"any","ctrl":"any","alt":"any","meta":"any","alt_graph":"any"}"#
    );
}

#[test]
fn command_metadata_round_trips_for_host_export() {
    let original = CommandMeta::new(
        CommandId::from_static("lines.join"),
        "Join Lines",
        CommandCategory::LINES,
    )
    .with_description("Joins each cursor's line with the next.")
    .mutating();

    let json = serde_json::to_string(&original).unwrap();
    let restored: CommandMeta = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, original);
    assert_eq!(restored.id().as_str(), "lines.join");
    assert!(restored.mutates_document());
}

#[test]
fn aliases_are_exported_but_never_accepted_back() {
    let aliased = CommandMeta::from_static(
        CommandId::from_static("clipboard.copy"),
        "Copy",
        CommandCategory::CLIPBOARD,
    )
    .with_aliases(&["yank"]);

    let json = serde_json::to_string(&aliased).unwrap();
    assert!(json.contains(r#""aliases":["yank"]"#), "{json}");

    // Aliases must be `'static`, so there is nothing a deserialized string can
    // become. Rejecting loudly is the point: a host manifest naming a synonym
    // that silently never matched would be undiagnosable from the palette.
    let error = serde_json::from_str::<CommandMeta>(&json).unwrap_err();
    assert!(error.to_string().contains("with_aliases"), "{error}");

    let plain = CommandMeta::from_static(
        CommandId::from_static("lines.join"),
        "Join Lines",
        CommandCategory::LINES,
    );
    let json = serde_json::to_string(&plain).unwrap();
    assert!(!json.contains("aliases"), "{json}");
    assert_eq!(serde_json::from_str::<CommandMeta>(&json).unwrap(), plain);

    // An explicitly empty list is accepted, so a consumer that always writes the
    // field still round-trips.
    let empty = r#"{"id":"lines.join","title":"Join Lines","category":"Lines","mutates_document":false,"aliases":[]}"#;
    assert_eq!(serde_json::from_str::<CommandMeta>(empty).unwrap(), plain);
}

#[test]
fn command_ids_and_labels_serialize_transparently() {
    assert_eq!(
        serde_json::to_string(&CommandId::from_static("edit.tab")).unwrap(),
        "\"edit.tab\""
    );
    assert_eq!(
        serde_json::to_string(&CommandCategory::LINES).unwrap(),
        "\"Lines\""
    );
    assert_eq!(
        serde_json::to_string(&ModeName::from_static("normal")).unwrap(),
        "\"normal\""
    );
    assert_eq!(
        serde_json::from_str::<CommandId>("\"edit.tab\"").unwrap(),
        CommandId::from_static("edit.tab")
    );
}
