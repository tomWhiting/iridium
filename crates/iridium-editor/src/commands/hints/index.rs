//! The reverse index: command id to the key sequences that run it.

use std::collections::HashMap;

use super::KeyHint;
use crate::commands::{CommandId, KeymapStack};

/// Every key sequence that runs each command, keyed by command id.
///
/// Built from a whole [`KeymapStack`], so it answers the question a palette
/// actually has — *what do I press for this command, given the user's own keymap
/// on top of the defaults?* — rather than the question one layer can answer.
///
/// # What is deliberately absent
///
/// A binding that cannot fire has no entry. Suppressed sequences, sequences a
/// higher layer took over, bindings outranked within their own layer, and chords
/// stranded behind a shorter binding on their prefix are all filtered out by
/// [`KeymapStack::binding_is_reachable`]. An empty result for a command means
/// *there is no key for this*, which is a true and useful answer; a wrong key is
/// worse than none.
#[derive(Debug, Clone, Default)]
pub struct KeyHintIndex {
    by_command: HashMap<CommandId, Vec<KeyHint>>,
}

impl KeyHintIndex {
    /// Builds the index for `stack`.
    ///
    /// Layers are walked lowest first so that, at equal rank, the earlier
    /// insertion wins and the order is stable across rebuilds. Each command's
    /// hints are then sorted best-first by [`KeyHint::rank`].
    #[must_use]
    pub fn build(stack: &KeymapStack) -> Self {
        let mut by_command: HashMap<CommandId, Vec<KeyHint>> = HashMap::new();

        for (layer_index, layer) in stack.layers().iter().enumerate() {
            for binding in layer.bindings() {
                // A suppression runs nothing, and a mode-entering binding with no
                // command has no command to be a hint *for*.
                let Some(command) = binding.command() else {
                    continue;
                };
                if !stack.binding_is_reachable(binding) {
                    continue;
                }
                by_command
                    .entry(command.clone())
                    .or_default()
                    .push(KeyHint::new(binding, layer_index));
            }
        }

        for hints in by_command.values_mut() {
            // Stable, so the lowest-layer-first insertion order breaks ties.
            hints.sort_by_key(KeyHint::rank);
        }

        Self { by_command }
    }

    /// Every sequence that runs `id`, best first.
    ///
    /// Returns an empty slice — allocating nothing — when the command has no
    /// reachable binding, which is the common case for a palette-only command.
    #[must_use]
    pub fn hints_for(&self, id: &str) -> &[KeyHint] {
        self.by_command
            .get(id)
            .map_or(&[], |hints| hints.as_slice())
    }

    /// The sequence a palette should show for `id`, or `None` if there is none.
    #[must_use]
    pub fn primary_hint(&self, id: &str) -> Option<&KeyHint> {
        self.hints_for(id).first()
    }

    /// The number of commands with at least one reachable binding.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_command.len()
    }

    /// Returns `true` when no command has a reachable binding.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_command.is_empty()
    }

    /// Every command id in the index, in unspecified order.
    pub fn command_ids(&self) -> impl ExactSizeIterator<Item = &CommandId> {
        self.by_command.keys()
    }
}
