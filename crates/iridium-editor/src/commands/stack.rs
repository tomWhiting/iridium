//! Layered keymaps: a user keymap overriding a default without editing it.

use serde::{Deserialize, Serialize};

use super::keymap::shadows;
use super::{CommandRegistry, KeyBinding, KeyPress, Keymap, KeymapError, ModeName};

/// An ordered stack of [`Keymap`] layers, highest precedence last.
///
/// Layering is what lets a user rebind one key without forking the default
/// keymap, and what lets a modal keymap sit on top of the non-modal defaults
/// while sharing every command. The base keymap is pushed first; each subsequent
/// layer overrides it.
///
/// # Resolution across layers
///
/// Layers are consulted **most specific (last pushed) first**, and precedence is
/// per *sequence*, not per layer:
///
/// - For a **complete** sequence, the first layer from the top that has an exact
///   match decides — including when that match is a
///   [`suppression`](KeyBinding::unbound), which makes the sequence unbound
///   without consulting lower layers. A user layer therefore overrides exactly
///   the sequences it mentions and nothing else.
/// - For a **pending prefix**, *every* layer is consulted, minus the sequences a
///   higher layer suppresses. Consulting every layer is what stops a user layer
///   that binds `Ctrl+K Ctrl+Z` from silently destroying the default's
///   `Ctrl+K Ctrl+D`; subtracting suppressions is what stops the reverse, where
///   unbinding `Ctrl+K Ctrl+D` would remove the leaf while leaving `Ctrl+K`
///   pending forever and eating the next keystroke.
///
/// # Cross-layer shadowing is a load-time diagnostic
///
/// Because an exact match fires the instant it completes, a *complete* binding in
/// one layer makes every longer sequence sharing its prefix unreachable — in
/// **any** layer. [`Self::validate`] therefore checks pairs across layers as well
/// as within them, and reports
/// [`KeymapError::CrossLayerShadowedSequence`]. Without that check the commonest
/// user customization there is — binding a bare `Ctrl+K` — silently strands every
/// `Ctrl+K …` chord in the default keymap.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeymapStack {
    /// Layers in increasing precedence order; the last entry wins.
    layers: Vec<Keymap>,
}

impl KeymapStack {
    /// Creates an empty stack.
    #[must_use]
    pub const fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Creates a stack containing `base` as its only, lowest-precedence layer.
    #[must_use]
    pub fn with_base(base: Keymap) -> Self {
        Self { layers: vec![base] }
    }

    /// Pushes `keymap` as the new highest-precedence layer.
    pub fn push(&mut self, keymap: Keymap) {
        self.layers.push(keymap);
    }

    /// Removes and returns the highest-precedence layer.
    pub fn pop(&mut self) -> Option<Keymap> {
        self.layers.pop()
    }

    /// The layers, in increasing precedence order.
    #[must_use]
    pub fn layers(&self) -> &[Keymap] {
        &self.layers
    }

    /// The number of layers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// Returns `true` when there are no layers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Removes every layer.
    pub fn clear(&mut self) {
        self.layers.clear();
    }

    /// Returns the winning binding for the complete sequence `presses`.
    ///
    /// Walks layers from highest precedence down and returns the first exact
    /// match, which may be a suppression (`is_suppression() == true`).
    #[must_use]
    pub fn exact_match(
        &self,
        presses: &[KeyPress],
        mode: Option<&ModeName>,
    ) -> Option<&KeyBinding> {
        self.layers
            .iter()
            .rev()
            .find_map(|layer| layer.exact_match(presses, mode))
    }

    /// Returns `true` when any layer can continue the sequence `presses` with a
    /// binding no higher layer has suppressed.
    #[must_use]
    pub fn has_continuation(&self, presses: &[KeyPress], mode: Option<&ModeName>) -> bool {
        self.layers.iter().enumerate().any(|(index, layer)| {
            layer
                .continuations(presses, mode)
                .any(|binding| !self.suppressed_above(index, binding))
        })
    }

    /// Returns `true` when some binding in reach of `presses` accepts a count.
    #[must_use]
    pub fn accepts_count_at(&self, presses: &[KeyPress], mode: Option<&ModeName>) -> bool {
        self.layers
            .iter()
            .any(|layer| layer.accepts_count_at(presses, mode))
    }

    /// Validates every layer against `registry`, then every layer against every
    /// other.
    ///
    /// # Errors
    ///
    /// - The first [`KeymapError`] any single layer reports; see
    ///   [`Keymap::validate`].
    /// - [`KeymapError::CrossLayerShadowedSequence`] when a complete sequence in
    ///   one layer makes a longer sequence in another unreachable.
    pub fn validate(&self, registry: &CommandRegistry) -> Result<(), KeymapError> {
        for layer in &self.layers {
            layer.validate(registry)?;
        }
        self.check_cross_layer_shadowing()
    }

    /// Canonicalizes the command ids of every layer against `registry`.
    ///
    /// # Errors
    ///
    /// The first [`KeymapError`] any layer reports; see
    /// [`Keymap::canonicalize`].
    pub fn canonicalize(&mut self, registry: &CommandRegistry) -> Result<(), KeymapError> {
        for layer in &mut self.layers {
            layer.canonicalize(registry)?;
        }
        Ok(())
    }

    /// Reports the first binding one layer renders unreachable in another.
    ///
    /// Only *cross*-layer pairs are examined; within one layer
    /// [`Keymap::validate`] has already reported the same defect with the
    /// single-keymap error. A binding either side of the pair is skipped when a
    /// higher layer suppresses it, because a suppressed sequence is not bound and
    /// so neither shadows nor is shadowed — which is what makes "bind the leader,
    /// unbind the chord" a valid customization rather than a rejected one.
    fn check_cross_layer_shadowing(&self) -> Result<(), KeymapError> {
        for (prefix_layer, prefix_map) in self.layers.iter().enumerate() {
            for shorter in prefix_map.bindings() {
                if self.suppressed_above(prefix_layer, shorter) {
                    continue;
                }
                for (long_layer, long_map) in self.layers.iter().enumerate() {
                    if long_layer == prefix_layer {
                        continue;
                    }
                    for longer in long_map.bindings() {
                        if !shadows(shorter, longer) || self.suppressed_above(long_layer, longer) {
                            continue;
                        }
                        return Err(KeymapError::CrossLayerShadowedSequence {
                            prefix_keymap: prefix_map.name().to_owned(),
                            prefix: shorter.display_sequence(),
                            shadowed_keymap: long_map.name().to_owned(),
                            shadowed: longer.display_sequence(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Returns `true` when a layer above `index` explicitly unbinds `binding`'s
    /// sequence.
    fn suppressed_above(&self, index: usize, binding: &KeyBinding) -> bool {
        self.layers
            .iter()
            .skip(index + 1)
            .any(|layer| layer.suppresses(binding))
    }
}
