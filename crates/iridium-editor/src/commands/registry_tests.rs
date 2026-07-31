//! Registry tests: registration rules, lookup, and deterministic enumeration.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;

use super::builtin::{
    BUILTIN_COMMAND_COUNT, builtin_commands, builtin_registry, register_builtin_commands,
};
use super::{CommandCategory, CommandId, CommandMeta, CommandRegistry, RegistryError};

fn meta(id: &'static str, title: &'static str) -> CommandMeta {
    CommandMeta::new(CommandId::from_static(id), title, CommandCategory::EDITING)
}

#[test]
fn registers_and_looks_up_by_str() {
    let mut registry = CommandRegistry::new();
    registry.register(meta("edit.one", "One")).unwrap();

    let found = registry.get("edit.one").expect("registered command");
    assert_eq!(found.id().as_str(), "edit.one");
    assert_eq!(found.title(), "One");
    assert_eq!(found.category(), &CommandCategory::EDITING);
    assert!(!found.mutates_document());
    assert!(registry.contains("edit.one"));
    assert!(!registry.contains("edit.two"));
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
}

#[test]
fn duplicate_ids_are_rejected_and_the_first_registration_survives() {
    let mut registry = CommandRegistry::new();
    registry.register(meta("edit.one", "First")).unwrap();

    let error = registry.register(meta("edit.one", "Second")).unwrap_err();
    assert_eq!(
        error,
        RegistryError::DuplicateId {
            id: "edit.one".to_owned()
        }
    );
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.get("edit.one").unwrap().title(), "First");
}

#[test]
fn empty_ids_are_rejected() {
    let mut registry = CommandRegistry::new();
    let error = registry.register(meta("", "Nameless")).unwrap_err();
    assert_eq!(error, RegistryError::EmptyId);
    assert!(registry.is_empty());
}

#[test]
fn owned_and_static_ids_are_interchangeable_keys() {
    let mut registry = CommandRegistry::new();
    registry
        .register(CommandMeta::new(
            CommandId::new(String::from("host.action")),
            "Host Action",
            CommandCategory::new(String::from("Host")),
        ))
        .unwrap();

    assert!(registry.contains("host.action"));
    let canonical = registry.canonical_id("host.action").unwrap();
    assert_eq!(canonical.as_str(), "host.action");
    assert!(!canonical.is_static());
}

#[test]
fn enumeration_follows_registration_order() {
    let mut registry = CommandRegistry::new();
    for id in ["z.last", "a.first", "m.middle"] {
        registry.register(meta(id, id)).unwrap();
    }

    let ids: Vec<&str> = registry.commands().map(|m| m.id().as_str()).collect();
    assert_eq!(ids, vec!["z.last", "a.first", "m.middle"]);
    assert_eq!(registry.commands().len(), 3);
}

#[test]
fn palette_order_is_total_and_independent_of_registration_order() {
    let forward = {
        let mut registry = CommandRegistry::new();
        registry
            .register(CommandMeta::new(
                CommandId::from_static("b.beta"),
                "Beta",
                CommandCategory::EDITING,
            ))
            .unwrap();
        registry
            .register(CommandMeta::new(
                CommandId::from_static("a.alpha"),
                "Alpha",
                CommandCategory::EDITING,
            ))
            .unwrap();
        registry
            .register(CommandMeta::new(
                CommandId::from_static("c.gamma"),
                "Gamma",
                CommandCategory::CLIPBOARD,
            ))
            .unwrap();
        registry
    };
    let reversed = {
        let mut registry = CommandRegistry::new();
        registry
            .register(CommandMeta::new(
                CommandId::from_static("c.gamma"),
                "Gamma",
                CommandCategory::CLIPBOARD,
            ))
            .unwrap();
        registry
            .register(CommandMeta::new(
                CommandId::from_static("a.alpha"),
                "Alpha",
                CommandCategory::EDITING,
            ))
            .unwrap();
        registry
            .register(CommandMeta::new(
                CommandId::from_static("b.beta"),
                "Beta",
                CommandCategory::EDITING,
            ))
            .unwrap();
        registry
    };

    let order_of = |registry: &CommandRegistry| -> Vec<String> {
        registry
            .palette_order()
            .into_iter()
            .map(|m| m.id().as_str().to_owned())
            .collect()
    };

    // Clipboard sorts before Editing; within Editing, Alpha before Beta.
    assert_eq!(order_of(&forward), vec!["c.gamma", "a.alpha", "b.beta"]);
    assert_eq!(order_of(&forward), order_of(&reversed));
}

#[test]
fn palette_order_is_stable_across_repeated_calls() {
    let registry = builtin_registry().unwrap();
    let first: Vec<&str> = registry
        .palette_order()
        .into_iter()
        .map(|m| m.id().as_str())
        .collect();
    for _ in 0..8 {
        let again: Vec<&str> = registry
            .palette_order()
            .into_iter()
            .map(|m| m.id().as_str())
            .collect();
        assert_eq!(first, again);
    }
}

#[test]
fn in_category_filters_in_palette_order() {
    let registry = builtin_registry().unwrap();
    let history = registry.in_category(CommandCategory::HISTORY.as_str());
    let titles: Vec<&str> = history.iter().map(|m| m.title()).collect();
    assert_eq!(titles, vec!["Redo", "Undo"]);
}

#[test]
fn builtin_registry_holds_the_documented_command_count() {
    let registry = builtin_registry().unwrap();
    assert_eq!(registry.len(), BUILTIN_COMMAND_COUNT);
    assert_eq!(builtin_commands().len(), BUILTIN_COMMAND_COUNT);
}

#[test]
fn builtin_ids_are_unique_non_empty_and_static() {
    let mut seen = HashSet::new();
    for meta in builtin_commands() {
        let id = meta.id();
        assert!(!id.is_empty(), "empty builtin id");
        assert!(id.is_static(), "builtin id `{id}` should be interned");
        assert!(!meta.title().is_empty(), "builtin `{id}` has no title");
        assert!(
            seen.insert(id.as_str().to_owned()),
            "duplicate builtin id `{id}`"
        );
        assert!(
            id.as_str().contains('.'),
            "builtin id `{id}` is missing its namespace"
        );
    }
}

#[test]
fn builtin_aliases_are_well_formed() {
    let ids: HashSet<String> = builtin_commands()
        .iter()
        .map(|meta| meta.id().as_str().to_owned())
        .collect();

    for meta in builtin_commands() {
        let id = meta.id();
        let mut seen = HashSet::new();
        for alias in meta.aliases() {
            assert!(!alias.is_empty(), "`{id}` has an empty alias");
            assert_eq!(
                *alias,
                alias.trim(),
                "alias `{alias}` on `{id}` has surrounding whitespace"
            );
            // Lowercase because the matcher awards an exact-case bonus: a
            // capitalized alias would score differently from the same word typed
            // in lower case, which is not a distinction the author intended.
            assert_eq!(
                *alias,
                alias.to_lowercase(),
                "alias `{alias}` on `{id}` is not lower case"
            );
            assert!(seen.insert(*alias), "alias `{alias}` is repeated on `{id}`");
            assert!(
                !alias.eq_ignore_ascii_case(meta.title()),
                "alias `{alias}` on `{id}` only restates its title"
            );
            // An alias that *is* another command's id would make an exact-id
            // query rank the wrong command first.
            assert!(
                !ids.contains(*alias),
                "alias `{alias}` on `{id}` collides with a command id"
            );
        }
    }
}

#[test]
fn aliases_are_absent_by_default_and_carried_when_declared() {
    assert!(meta("edit.plain", "Plain").aliases().is_empty());

    let aliased = meta("edit.aliased", "Aliased").with_aliases(&["synonym", "other"]);
    assert_eq!(aliased.aliases(), ["synonym", "other"]);

    // The same synonym on two commands is legitimate — both deletions really are
    // what a user means by "erase" — so the table must not be deduplicated
    // globally. Anchored here because a well-meaning uniqueness check would break
    // it.
    let registry = builtin_registry().unwrap();
    let backward = registry.get("edit.deleteBackward").unwrap().aliases();
    let forward = registry.get("edit.deleteForward").unwrap().aliases();
    assert!(backward.contains(&"erase"));
    assert!(forward.contains(&"erase"));

    assert!(
        registry
            .get("clipboard.copy")
            .unwrap()
            .aliases()
            .contains(&"yank")
    );
    assert!(
        registry
            .get("cursor.lineEnd")
            .unwrap()
            .aliases()
            .contains(&"eol")
    );
    assert!(
        registry
            .get("selection.selectAll")
            .unwrap()
            .aliases()
            .is_empty()
    );
}

#[test]
fn registering_the_builtins_twice_is_rejected_rather_than_shadowing() {
    let mut registry = builtin_registry().unwrap();
    let error = register_builtin_commands(&mut registry).unwrap_err();
    assert!(matches!(error, RegistryError::DuplicateId { .. }));
}

#[test]
fn descriptions_are_optional() {
    let plain = meta("edit.plain", "Plain");
    assert!(plain.description().is_none());
    let described = meta("edit.described", "Described").with_description("Longer text.");
    assert_eq!(described.description(), Some("Longer text."));
}

#[test]
fn mutating_flag_is_carried() {
    let command = meta("edit.mutates", "Mutates").mutating();
    assert!(command.mutates_document());
    let registry = builtin_registry().unwrap();
    assert!(
        registry
            .get("edit.deleteBackward")
            .unwrap()
            .mutates_document()
    );
    assert!(!registry.get("cursor.charLeft").unwrap().mutates_document());
    assert!(!registry.get("clipboard.copy").unwrap().mutates_document());
    assert!(registry.get("clipboard.cut").unwrap().mutates_document());
}
