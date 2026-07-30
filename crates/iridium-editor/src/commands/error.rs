//! Error types for command registration and keymap construction.

use thiserror::Error;

/// A command could not be registered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegistryError {
    /// A command with this id is already registered.
    ///
    /// Registration never silently overwrites: two commands claiming one id is a
    /// programming error (or a host colliding with the kernel), and the correct
    /// response is to reject the second, not to shadow the first.
    #[error("a command with id `{id}` is already registered")]
    DuplicateId {
        /// The id that was already taken.
        id: String,
    },

    /// The command id was empty.
    #[error("command ids must not be empty")]
    EmptyId,
}

/// A keymap was structurally invalid, or referenced something unknown.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KeymapError {
    /// A binding carried an empty key sequence.
    ///
    /// Only reachable through deserialization: the in-process constructors take
    /// the first stroke as a separate parameter, so non-emptiness is a
    /// type-level guarantee for code-built keymaps.
    #[error("keymap `{keymap}` contains a binding with an empty key sequence")]
    EmptySequence {
        /// The name of the offending keymap.
        keymap: String,
    },

    /// A binding referenced a command id that is not in the registry.
    #[error("keymap `{keymap}` binds unknown command `{id}`")]
    UnknownCommand {
        /// The name of the offending keymap.
        keymap: String,
        /// The unresolvable command id.
        id: String,
    },

    /// One binding's sequence is a strict prefix of another's in the same
    /// keymap and mode, which makes the longer binding unreachable.
    ///
    /// Resolution deliberately fires an exact match the instant it completes
    /// rather than waiting to see whether a longer sequence follows (see
    /// [`KeymapResolver`](crate::commands::KeymapResolver)), so the longer
    /// binding could never run. Reporting it is the only honest outcome; a
    /// timeout would trade a silent bug for input latency.
    #[error(
        "keymap `{keymap}`: binding for `{shadowed}` is unreachable because `{prefix}` is bound as a complete sequence"
    )]
    ShadowedSequence {
        /// The name of the offending keymap.
        keymap: String,
        /// Display form of the complete binding acting as a prefix.
        prefix: String,
        /// Display form of the binding rendered unreachable.
        shadowed: String,
    },

    /// One layer's binding is a complete sequence that is a strict prefix of a
    /// binding in *another* layer, making the longer one unreachable.
    ///
    /// The same defect as [`Self::ShadowedSequence`], across a
    /// [`KeymapStack`](crate::commands::KeymapStack). It has its own variant
    /// because the two layers must both be named for the diagnostic to be
    /// actionable: the usual case is a user keymap binding a bare `Ctrl+K`,
    /// which strands every `Ctrl+K …` sequence in the default keymap. The fix is
    /// to also [`unbind`](crate::commands::KeyBinding::unbound) the stranded
    /// sequences, which suppresses them and clears the diagnostic.
    ///
    /// A suppression matches a sequence *exactly*, so the fix must name the
    /// stranded binding's own modifier pattern rather than an approximation of it.
    /// The `shadowed` field is the round-trippable display form for precisely that
    /// reason: feeding it to
    /// [`KeyBinding::parse_sequence`](crate::commands::KeyBinding::parse_sequence)
    /// reproduces the sequence to unbind, so the diagnostic is quotable rather than
    /// merely descriptive.
    #[error(
        "keymap `{shadowed_keymap}`: binding for `{shadowed}` is unreachable because keymap `{prefix_keymap}` binds `{prefix}` as a complete sequence"
    )]
    CrossLayerShadowedSequence {
        /// The name of the layer holding the prefix binding.
        prefix_keymap: String,
        /// Display form of the complete binding acting as a prefix.
        prefix: String,
        /// The name of the layer holding the unreachable binding.
        shadowed_keymap: String,
        /// Display form of the binding rendered unreachable.
        shadowed: String,
    },

    /// A textual key sequence could not be parsed.
    ///
    /// Produced by [`StrokePattern::from_str`](crate::commands::StrokePattern) and
    /// [`KeyBinding::parse_sequence`](crate::commands::KeyBinding::parse_sequence),
    /// so a hand-written `"ctrl-k ctrl-c"` in a configuration file is a startup
    /// diagnostic naming the offending chord rather than a silently dropped
    /// binding.
    #[error("`{stroke}` is not a valid key chord")]
    UnparsableStroke {
        /// The text that could not be parsed.
        stroke: String,
    },
}
