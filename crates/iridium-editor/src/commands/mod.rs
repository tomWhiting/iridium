//! Command registry and keymap layer.
//!
//! Every editor action is a **named, described, addressable value** in a
//! [`CommandRegistry`] instead of a branch in a `match` statement. One structure
//! then serves four consumers that would otherwise each need their own source of
//! truth:
//!
//! - a **keymap** is *key sequence → command id* ([`Keymap`]);
//! - a **command palette** is fuzzy search over the same registry;
//! - **rebinding, macros and keybinding help** read the same two structures;
//! - a **scripting or AI host** discovers and drives the editor through this
//!   public surface, never from inside the kernel.
//!
//! # Naming
//!
//! [`crate::history::Command`] already means "a reversible document mutation".
//! Nothing here reuses that name. This module deals in *command identities* and
//! their metadata — [`CommandId`], [`CommandMeta`], [`CommandRegistry`] — which
//! say what an action *is*, not how a document changes. Running a command still
//! produces a [`crate::history::Command`]; that remains the only way document
//! state may change.
//!
//! # Layout
//!
//! | Type | Role |
//! |------|------|
//! | [`CommandId`] | stable machine id; the compatibility surface keymaps and hosts reference |
//! | [`CommandMeta`] | title, description, category, mutation hint |
//! | [`CommandCategory`] | palette grouping (open, string-ish) |
//! | [`CommandRegistry`] | register, look up, enumerate deterministically |
//! | [`KeyPress`] | one concrete keypress from the host |
//! | [`StrokePattern`] | one element of a binding: a key plus [`ModifierPattern`] |
//! | [`ModifierPattern`] / [`ModifierState`] | per-modifier `Required` / `Forbidden` / `Any` |
//! | [`KeyBinding`] | a key *sequence* plus what it does: a command id, a [`ModeName`] to enter, an optional count |
//! | [`StrokeCapture`] | widens one stroke to any character and captures it (`f{char}`) |
//! | [`CommandArgs`] / [`CommandInvocation`] | the count and captured characters a sequence carried |
//! | [`Keymap`] | one ordered layer of bindings |
//! | [`KeymapStack`] | layers, highest precedence last |
//! | [`KeymapResolver`] | the pending-sequence state machine; produces [`Resolution`] |
//! | [`builtin`] | ids and metadata for every command the kernel implements |
//! | [`default_non_modal_keymap`] | the non-modal default bindings, as data |
//!
//! # How the keyboard layer uses this
//!
//! `crate::input::keyboard` resolves every keypress through here; there is no
//! `match` on `(KeyCode, Modifiers)` left. The rules:
//!
//! 1. A [`KeyPress`] from [`KeyPress::from_event`] goes to
//!    [`KeymapResolver::resolve_repeat`], which normalizes it for *comparison*
//!    only. The original [`crate::input::KeyEvent`] is kept, because step 3 needs
//!    the character the user actually typed.
//! 2. [`Resolution::Matched`] runs the command with its [`CommandArgs`].
//!    [`Resolution::ModeEntered`], [`Resolution::Pending`] and
//!    [`Resolution::Aborted`] consume the key and return `KeyResult::Handled` — a
//!    pending chord must never reach the document. A matched command the kernel
//!    does not implement becomes `KeyResult::HostCommand`, so a host command is
//!    reported rather than silently dropped.
//! 3. [`Resolution::NoMatch`] falls through: if the key is a
//!    [`crate::input::KeyCode::Char`] and none of `ctrl`, `alt` or `meta` is held,
//!    [`builtin::EDIT_INSERT_CHARACTER`] runs with the **original,
//!    un-normalized** character; otherwise the key is
//!    `KeyResult::Ignored`.
//! 4. Sticky columns and the multi-cursor addition-order stack are invalidated
//!    after *every* key by `note_operation`, keyed on the resolved command. A
//!    selection that round-trips to a byte-identical state has twice revived a
//!    stale snapshot in this codebase, so any change to *where* that invalidation
//!    happens must be tested explicitly.
//! 5. A host that loads a keymap from configuration should use
//!    `KeyboardHandler::push_validated_keymap` (or [`Keymap::canonicalize`] plus
//!    [`KeymapStack::validate`] by hand). That turns an unknown command id, an
//!    unreachable binding and a cross-layer shadow into startup diagnostics, and
//!    restores allocation-free resolution.
//!
//! # Adding a command from outside the kernel
//!
//! The registry is open, so a host — including `iridium-bindings`, a terminal
//! face, or an AI host — contributes a command without editing this crate:
//!
//! 1. `Editor::register_command` adds a [`CommandMeta`], which makes the command
//!    appear in the palette ([`CommandRegistry::palette_order`]) and in
//!    keybinding help.
//! 2. A [`KeyBinding`] in a pushed [`Keymap`] gives it a key sequence, or
//!    `Editor::run_command` invokes it by id with no keystroke at all.
//! 3. When the binding fires, the editor reports
//!    `EditorKeyResult::HostCommand`, naming the id and its arguments; the host
//!    runs its own behaviour and applies any document change through
//!    `Editor::apply_command`, so the command-sourced invariant is preserved.
//!
//! The command implementations the kernel *compiles in* stay in a static table in
//! `crate::input::keyboard::actions`, matched exhaustively over an enum so a
//! registered built-in without an implementation is a compile error. That table is
//! the kernel's half of the contract; the registry remains the authority on what
//! exists, and the two are reconciled by tests.

pub mod builtin;

mod args;
mod binding;
mod default_keymap;
mod error;
mod hints;
mod id;
mod keymap;
mod keynames;
mod meta;
mod modifiers;
mod names;
mod registry;
mod resolver;
mod stack;
mod stroke;
mod stroke_text;

#[cfg(test)]
mod default_keymap_tests;
#[cfg(test)]
mod grammar_tests;
#[cfg(test)]
mod keymap_tests;
#[cfg(test)]
mod layer_tests;
#[cfg(test)]
mod reachability_tests;
#[cfg(test)]
mod registry_tests;
#[cfg(test)]
mod resolver_tests;
#[cfg(test)]
mod serde_tests;
#[cfg(test)]
mod text_form_tests;
#[cfg(test)]
mod validation_tests;

pub use args::{CommandArgs, CommandInvocation};
pub use binding::KeyBinding;
pub use default_keymap::{
    DEFAULT_KEYMAP_BINDING_COUNT, default_keymap_stack, default_non_modal_keymap,
};
pub use error::{KeymapError, RegistryError};
pub use hints::{KeyHint, KeyHintIndex, KeyLabelStyle};
pub use id::CommandId;
pub use keymap::Keymap;
pub use keynames::{key_from_name, name_of};
pub use meta::CommandMeta;
pub use modifiers::{ModifierPattern, ModifierState};
pub use names::{CommandCategory, ModeName};
pub use registry::CommandRegistry;
pub use resolver::{KeymapResolver, Resolution};
pub use stack::KeymapStack;
pub use stroke::{KeyPress, StrokeCapture, StrokePattern};
