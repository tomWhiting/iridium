//! What a menu row says, decided once for every menu this face draws.
//!
//! # Why this is its own module
//!
//! Two menus reach the same commands by pointing: the drawn context menu
//! ([`crate::context_menu`]) and the macOS menu bar ([`crate::menubar`]). Both
//! have to answer the same four questions about a row — does the command
//! exist, what is it called, what chord runs it, and can it run right now —
//! and an answer given twice is an answer that can differ. It has differed
//! before in this codebase; *one transcription, not two* is the rule that
//! came out of it.
//!
//! So the **ruled set** stays with each menu, because which verbs belong in a
//! right-click menu and which belong under *File* are different decisions.
//! The **resolution** lives here, because it is one decision wearing two hats.
//!
//! # Nothing here is typed out
//!
//! ⚠️ A row's title comes from [`CommandRegistry`], and its chord comes from
//! the [`KeyHintIndex`] built off the live keymap stack. Neither is spelled in
//! a literal anywhere, and that is deliberate: a menu that spells `⌘O` beside
//! *Open File…* is a second place for that fact to go stale, and rebinding the
//! key would leave the menu lying about what the keyboard does.
//!
//! The one thing a ruled set *does* spell is the **label**, which is not the
//! same string as the registry title: *Open Folder…* reads correctly under
//! *File* while the palette wants the fuller *Open Folder…* with its
//! description beside it. A test in each menu pins the labels that must agree
//! with the registry against drift.
//!
//! [`CommandRegistry`]: iridium_editor::commands::CommandRegistry
//! [`KeyHintIndex`]: iridium_editor::commands::KeyHintIndex

use iridium_editor::{CommandId, Editor, KeyLabelStyle};

/// One entry of a ruled verb set, before the registry is consulted.
///
/// The ids are the kernel's own constants at every call site, so a renamed
/// command is a compile error rather than a dead row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verb {
    /// The registered command the row runs.
    pub id: CommandId,
    /// The menu's label for it.
    pub label: &'static str,
}

/// One line of a ruled set: a verb, or the rule between two groups of them.
///
/// ⚠️ **Named rather than an `Option<Verb>`.** A ruled set is read far more
/// often than it is written — it is the table somebody scans to answer "why is
/// *Save As* not on the File menu" — and `Row::Rule` says what a `None` in the
/// middle of forty rows does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A verb, which becomes a row if the registry carries it.
    Verb(Verb),
    /// A hairline between two groups, kept only where it separates something.
    Rule,
}

impl Row {
    /// A verb row, spelled short so a ruled set reads as a table.
    #[must_use]
    pub const fn verb(id: CommandId, label: &'static str) -> Self {
        Self::Verb(Verb { id, label })
    }

    /// The verb this row carries, or `None` for a rule.
    ///
    /// What a ruled set's own tests walk: every assertion about the table is
    /// about its verbs, and a rule has nothing to assert.
    #[must_use]
    pub const fn as_verb(&self) -> Option<&Verb> {
        match self {
            Self::Verb(verb) => Some(verb),
            Self::Rule => None,
        }
    }
}

/// One entry of a ruled verb set after the registry has been consulted.
///
/// A resolved row is a row that is going to be shown: a verb the registry does
/// not carry never becomes one. See [`resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedVerb {
    /// The command the row runs.
    pub command: CommandId,
    /// The label, as the ruled set named it.
    pub label: &'static str,
    /// The chord that runs the same verb, when one is reachable.
    pub hint: Option<String>,
    /// Whether the row can run now: `false` greys it.
    pub enabled: bool,
}

/// Why a menu row might be unable to run.
///
/// ⭐ **A struct rather than a bare `bool` because the two reasons are not the
/// same reason**, and folding them together is how a face ends up greying a
/// row it should not. `read_only` is a property of the *document* and greys
/// only what would write to it; `inert` is a property of the *session* — a
/// modal panel is up, and nothing outside it may run at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Availability {
    /// The document refuses writes, so verbs that mutate it are greyed.
    pub read_only: bool,
    /// Something modal owns the session, so every verb is greyed.
    pub inert: bool,
}

impl Availability {
    /// Nothing in the way: the ordinary state of an editable document.
    pub const OPEN: Self = Self {
        read_only: false,
        inert: false,
    };

    /// Whether a command that does or does not mutate the document can run.
    const fn allows(self, mutates_document: bool) -> bool {
        !(self.inert || (mutates_document && self.read_only))
    }
}

/// Resolves a ruled verb set against a live registry and keymap.
///
/// A [`Row::Rule`] becomes a `None` in the result, and only the rules that
/// still separate something survive: one is dropped where it would lead, trail
/// or double up once the verbs around it have been resolved.
///
/// ⚠️ **A verb the registry does not carry is dropped, not greyed.** A menu row
/// that could never run under any state is a lie about what the program can
/// do, and the registry is the only authority on what exists. Greying is for
/// a verb that exists and cannot run *now*.
#[must_use]
pub fn resolve(
    editor: &Editor,
    verbs: &[Row],
    availability: Availability,
) -> Vec<Option<ResolvedVerb>> {
    let mut rows: Vec<Option<ResolvedVerb>> = Vec::with_capacity(verbs.len());
    for entry in verbs {
        let Row::Verb(verb) = entry else {
            // A rule is kept only where a resolved verb precedes it; whether
            // it survives at the end is settled by the trim below.
            if rows.last().is_some_and(Option::is_some) {
                rows.push(None);
            }
            continue;
        };
        let Some(meta) = editor.commands().get(verb.id.as_str()) else {
            continue;
        };
        rows.push(Some(ResolvedVerb {
            command: verb.id.clone(),
            label: verb.label,
            hint: editor
                .key_hints()
                .primary_hint(verb.id.as_str())
                .map(|hint| hint.label(KeyLabelStyle::MacGlyphs).to_owned()),
            enabled: availability.allows(meta.mutates_document()),
        }));
    }
    while rows.last().is_some_and(Option::is_none) {
        rows.pop();
    }
    rows
}

#[cfg(test)]
mod tests {
    use iridium_editor::commands::builtin::{CLIPBOARD_PASTE, PALETTE_OPEN, SELECTION_SELECT_ALL};
    use iridium_editor::{CommandId, Editor};

    use super::{Availability, ResolvedVerb, Row, resolve};

    /// A kernel carrying this face's commands and keymap — the same session
    /// shape the menus are resolved against in a running window.
    fn editor() -> Editor {
        let mut editor = Editor::with_defaults();
        for meta in crate::commands::command_metas() {
            editor
                .register_command(meta)
                .expect("the kernel accepted the command");
        }
        editor
            .push_keymap(crate::commands::keymap())
            .expect("the keymap validates");
        editor
    }

    /// A verb row.
    const fn verb(id: CommandId, label: &'static str) -> Row {
        Row::verb(id, label)
    }

    /// The labels of the resolved rows, with `None` for a surviving rule.
    fn labels(rows: &[Option<ResolvedVerb>]) -> Vec<Option<&'static str>> {
        rows.iter()
            .map(|row| row.as_ref().map(|row| row.label))
            .collect()
    }

    #[test]
    fn a_verb_the_registry_does_not_carry_is_dropped() {
        let rows = resolve(
            &editor(),
            &[
                verb(SELECTION_SELECT_ALL, "Select All"),
                verb(CommandId::from_static("nothing.at.all"), "Nowhere"),
            ],
            Availability::OPEN,
        );
        assert_eq!(labels(&rows), vec![Some("Select All")]);
    }

    /// ⭐ The rule that keeps a dropped verb from leaving a scar. Before the
    /// trim, a set whose last group resolved to nothing ended in a hairline
    /// with nothing under it.
    #[test]
    fn separators_never_lead_trail_or_double_up() {
        let rows = resolve(
            &editor(),
            &[
                Row::Rule,
                Row::Rule,
                verb(SELECTION_SELECT_ALL, "Select All"),
                Row::Rule,
                Row::Rule,
                verb(PALETTE_OPEN, "Command Palette…"),
                Row::Rule,
                verb(CommandId::from_static("nothing.at.all"), "Nowhere"),
                Row::Rule,
            ],
            Availability::OPEN,
        );
        assert_eq!(
            labels(&rows),
            vec![Some("Select All"), None, Some("Command Palette…")]
        );
    }

    /// The chord is read from the keymap, never typed here, and it is read in
    /// the style a mac hand reads.
    ///
    /// ⚠️ **This deliberately does not assert which glyph.** The first draft
    /// asserted `⌘`, on the assumption that this face maps the command key
    /// onto the kernel's ctrl — and it failed, because Select All is `⌃A`
    /// here: since #104 both keys are reachable and which one a verb wears is
    /// the keymap's business, not this module's. A test that named the glyph
    /// would be pinning a binding from the wrong file.
    ///
    /// What is asserted is the property that actually separates the two label
    /// styles, and so is the thing that could regress: [`KeyLabelStyle::
    /// MacGlyphs`] spells every modifier as a glyph, while `Portable` spells
    /// them as words joined by `+` and the round-trippable form spells an
    /// ignored modifier as `~name`. Any of the three would satisfy "is a
    /// non-empty string"; only one satisfies this.
    ///
    /// [`KeyLabelStyle::MacGlyphs`]: iridium_editor::KeyLabelStyle::MacGlyphs
    #[test]
    fn the_chord_comes_from_the_keymap_and_wears_mac_glyphs() {
        let rows = resolve(
            &editor(),
            &[verb(SELECTION_SELECT_ALL, "Select All")],
            Availability::OPEN,
        );
        let hint = rows[0]
            .as_ref()
            .and_then(|row| row.hint.clone())
            .expect("select all is bound in this face");
        assert!(
            !hint.contains('~'),
            "`{hint}` is the round-trippable form, not the presentable one"
        );
        for word in ["Ctrl", "Shift", "Alt", "Cmd", "Meta", "+"] {
            assert!(
                !hint.contains(word),
                "`{hint}` is the portable form, not the mac one"
            );
        }
        assert!(
            hint.chars().any(|glyph| "⌘⌃⌥⇧".contains(glyph)),
            "`{hint}` carries no modifier glyph at all"
        );
    }

    /// A read-only document greys what would write to it and nothing else.
    #[test]
    fn read_only_greys_the_writers_and_leaves_the_readers_alone() {
        let rows = resolve(
            &editor(),
            &[
                verb(CLIPBOARD_PASTE, "Paste"),
                verb(SELECTION_SELECT_ALL, "Select All"),
            ],
            Availability {
                read_only: true,
                inert: false,
            },
        );
        let enabled: Vec<bool> = rows
            .iter()
            .filter_map(|row| row.as_ref().map(|row| row.enabled))
            .collect();
        assert_eq!(
            enabled,
            vec![false, true],
            "paste writes and select-all does not"
        );
    }

    /// ⭐ The reason [`Availability`] is not a bool. An inert session greys
    /// everything, including the verbs a read-only document leaves alone —
    /// and if the two were one flag, one of these two cases would be wrong.
    #[test]
    fn an_inert_session_greys_even_what_does_not_write() {
        let rows = resolve(
            &editor(),
            &[verb(SELECTION_SELECT_ALL, "Select All")],
            Availability {
                read_only: false,
                inert: true,
            },
        );
        assert_eq!(
            rows[0].as_ref().map(|row| row.enabled),
            Some(false),
            "a modal panel owns the session, so nothing outside it runs"
        );
    }
}
