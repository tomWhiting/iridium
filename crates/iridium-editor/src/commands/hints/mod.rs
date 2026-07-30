//! Which key runs which command: the reverse of keymap resolution.
//!
//! Resolution answers *given these keypresses, what runs?* A command palette
//! needs the opposite — *given this command, what do I press?* — and no amount of
//! walking a [`Keymap`](crate::commands::Keymap) answers it correctly, because a
//! binding's presence in a layer says nothing about whether it can fire.
//!
//! # Two things this module refuses to get wrong
//!
//! **A hint that names a dead key.** Four separate mechanisms can take a binding
//! away — a higher layer suppressing it, a higher layer rebinding its sequence, a
//! sibling in the same layer outranking it, and a shorter binding claiming its
//! prefix — and none is visible from the binding alone. Rather than re-derive
//! those rules, the index asks
//! [`KeymapStack::binding_is_reachable`](crate::commands::KeymapStack::binding_is_reachable),
//! which puts the question to the resolver itself.
//!
//! **A hint that is unreadable.** [`KeyBinding::display_sequence`](crate::commands::KeyBinding::display_sequence)
//! renders the form that parses back, which spells every ignored modifier as
//! `~name`; the real default keymap ignores `AltGraph` on every `Ctrl` chord, so
//! the key for *Select All* round-trips as `ctrl+~shift+~altgraph+a`. Labels are
//! rendered separately, from the modifiers a person actually holds down, and
//! come out as `Ctrl+A`.
//!
//! # Cost
//!
//! [`KeyHintIndex::build`] allocates, so it runs when the keymap stack changes —
//! startup and configuration reload — and never on a keystroke. Lookups are a
//! hash probe returning a borrowed slice.

mod hint;
mod index;
mod label;

#[cfg(test)]
mod hint_tests;

pub use hint::KeyHint;
pub use index::KeyHintIndex;
pub use label::KeyLabelStyle;
