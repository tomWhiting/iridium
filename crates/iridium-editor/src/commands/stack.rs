//! Layered keymaps: a user keymap overriding a default without editing it.

use serde::{Deserialize, Serialize};

use super::keymap::shadows;
use super::{
    CommandRegistry, KeyBinding, KeyPress, Keymap, KeymapError, ModeName, ModifierPattern,
    ModifierState, StrokePattern,
};
use crate::input::{KeyCode, Modifiers};

/// Characters tried when deciding whether a wildcard binding can fire.
///
/// A wildcard is reachable as soon as *one* character resolves to it, so this
/// only has to be wide enough that a keymap cannot bind every entry literally.
/// The ASCII letters and digits cover the printable keys a modal grammar
/// realistically binds; `'~'` is the tail escape for the pathological keymap that
/// binds all of them.
const WILDCARD_WITNESS_CHARS: [char; 37] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '~',
];

/// The keypress that satisfies `stroke` while holding as little as possible.
///
/// [`ModifierState::Any`] renders as *released*: it accepts either, and a
/// released modifier is the one a key hint displays. See
/// [`KeymapStack::binding_is_reachable`] for why the minimal form is the correct
/// witness rather than a convenient one.
const fn minimal_press(stroke: &StrokePattern) -> KeyPress {
    KeyPress::new(stroke.key, minimal_modifiers(stroke.modifiers))
}

/// The modifier state that satisfies `pattern` while holding as little as
/// possible.
const fn minimal_modifiers(pattern: ModifierPattern) -> Modifiers {
    Modifiers {
        shift: is_required(pattern.shift),
        ctrl: is_required(pattern.ctrl),
        alt: is_required(pattern.alt),
        meta: is_required(pattern.meta),
        alt_graph: is_required(pattern.alt_graph),
    }
}

/// Returns `true` only for [`ModifierState::Required`].
const fn is_required(state: ModifierState) -> bool {
    matches!(state, ModifierState::Required)
}

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

    /// Swaps the layer named `keymap.name()` for `keymap`, returning the old
    /// one, or `None` when no layer bears that name.
    ///
    /// ⭐ **The operation a configuration reload needs, and the reason
    /// [`push`](Self::push) is not it.** Re-reading a file and pushing the
    /// result would leave the previous version of that layer underneath the new
    /// one: every binding the user *deleted* from the file would go on firing
    /// from the layer below, and the editor would disagree with its own
    /// configuration in a way nothing on screen could explain.
    ///
    /// [`pop`](Self::pop) followed by `push` is not it either. That is only
    /// correct while the layer being replaced happens to be the top one, which
    /// is a fact about the face's startup order rather than anything this stack
    /// guarantees — so it would keep working right up until a face pushed a
    /// third layer, and then quietly pop the wrong one.
    ///
    /// The name comes from the incoming keymap rather than a separate argument,
    /// so replacing the layer called `user` with a keymap called something else
    /// is not expressible.
    ///
    /// Precedence is preserved exactly: the replacement sits at the index the
    /// old layer held, not at the top.
    ///
    /// Layer names are expected to be unique within a stack. When they are not,
    /// the **highest-precedence** match is the one replaced, because that is the
    /// layer deciding what those chords do today.
    pub fn replace_named(&mut self, keymap: Keymap) -> Option<Keymap> {
        let index = self
            .layers
            .iter()
            .rposition(|layer| layer.name() == keymap.name())?;
        Some(core::mem::replace(&mut self.layers[index], keymap))
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

    /// Whether a printable key no binding claimed inserts itself in `mode`.
    ///
    /// **Any layer that silences `mode` silences it for the stack**, rather than
    /// the highest layer deciding as it does for bindings. Two layers
    /// disagreeing about whether a mode types is not a precedence question — a
    /// mode either is a typing mode or it is not — and of the two readings only
    /// one is safe: a key that does nothing is a key the user presses again,
    /// while a key that types is text in a document nobody asked to change.
    #[must_use]
    pub fn types_unclaimed_keys(&self, mode: Option<&ModeName>) -> bool {
        self.layers
            .iter()
            .all(|layer| layer.types_unclaimed_keys(mode))
    }

    /// The mode a session using this stack begins in, or `None`.
    ///
    /// The **highest** layer that names one wins, unlike
    /// [`Self::types_unclaimed_keys`]. This is a single value rather than a
    /// safety property, and a layer pushed on top of a modal keymap is the more
    /// specific statement about which mode the session is in — that is exactly
    /// what layering means everywhere else here.
    #[must_use]
    pub fn initial_mode(&self) -> Option<&ModeName> {
        self.layers.iter().rev().find_map(Keymap::initial_mode)
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

    /// Returns `true` when pressing `binding`'s own key sequence actually runs
    /// `binding`.
    ///
    /// `binding` must be borrowed from this stack; identity is compared by
    /// pointer, so a structurally equal binding from elsewhere always answers
    /// `false`.
    ///
    /// # Why this exists
    ///
    /// A binding being *present* in the stack does not mean it can ever fire.
    /// Four things can take it away, and all four are invisible from the binding
    /// alone:
    ///
    /// - a higher layer [`suppresses`](KeyBinding::unbound) its sequence;
    /// - a higher layer binds the same sequence and mode to something else;
    /// - a binding in the *same* layer outranks it (see [`Keymap::exact_match`]);
    /// - **a shorter binding claims one of its prefixes**, in any layer.
    ///
    /// Rather than re-derive those rules — and drift from them — this asks the
    /// resolver directly: it synthesizes the keypresses the binding describes and
    /// checks that [`Self::exact_match`] hands back this very binding.
    ///
    /// The prefix case needs its own check, because `exact_match` only ever
    /// compares whole sequences and so cannot see it. `KeymapResolver::resolve`
    /// tests for an exact match *before* it tests for a continuation, so a
    /// complete binding on `Ctrl+K` fires the moment `Ctrl+K` is pressed and
    /// `Ctrl+K Ctrl+D` never gets a second keystroke. This is the same defect
    /// [`Self::validate`] rejects at load time as
    /// [`KeymapError::CrossLayerShadowedSequence`]; a stack assembled without
    /// validation must still report the truth about it.
    ///
    /// # The synthesized keypress is the one a hint promises
    ///
    /// Each stroke becomes the *minimal* keypress that satisfies it: every
    /// [`ModifierState::Required`] modifier held,
    /// every other one released — including
    /// [`Any`](super::ModifierState::Any), which accepts either.
    ///
    /// That is deliberately not an approximation. A key hint shown in a palette
    /// is a promise about one exact keypress: *press these modifiers and this
    /// key, and this command runs*. So the binding that must win is the one that
    /// wins for exactly those modifiers and no others. A binding outranked at its
    /// own minimal keypress may still fire under some *additional* modifier, but
    /// displaying the minimal form for it would be a lie, and no hint is better
    /// than a wrong one.
    ///
    /// A [`StrokeCapture::AnyChar`](super::StrokeCapture::AnyChar) stroke has no
    /// single key to synthesize, so candidate characters are tried until one
    /// resolves to this binding. A literal binding on a character always outranks
    /// a wildcard there (`Keymap::exact_match` ranks non-capturing bindings
    /// higher), so testing one fixed character would report a live wildcard as
    /// unreachable whenever that character happened to be bound.
    #[must_use]
    pub fn binding_is_reachable(&self, binding: &KeyBinding) -> bool {
        let strokes = binding.sequence();
        // `KeyBinding` always carries a first stroke, so this is defensive only.
        if strokes.is_empty() {
            return false;
        }

        // Wildcards need a character no higher-ranked literal binding claims;
        // everything else has exactly one minimal witness.
        let capture_positions: Vec<usize> = strokes
            .iter()
            .enumerate()
            .filter(|(_, stroke)| stroke.captures())
            .map(|(index, _)| index)
            .collect();

        let mut presses: Vec<KeyPress> = strokes.iter().map(minimal_press).collect();
        if capture_positions.is_empty() {
            return self.resolves_to(&presses, binding.mode(), binding);
        }

        // One character is substituted into *every* capture stroke at once: a
        // sequence such as `f{char}{char}` is reachable iff some character makes
        // the whole sequence resolve here, and trying each independently would
        // multiply the search for no gain in coverage.
        WILDCARD_WITNESS_CHARS.iter().any(|&candidate| {
            for &index in &capture_positions {
                presses[index].key = KeyCode::Char(candidate);
            }
            self.resolves_to(&presses, binding.mode(), binding)
        })
    }

    /// Returns `true` when typing `presses` in `mode` runs exactly `binding`.
    ///
    /// Both halves of "runs" are checked: no proper prefix may resolve first —
    /// which would fire that shorter binding and swallow the rest — and the whole
    /// sequence must then resolve here.
    fn resolves_to(
        &self,
        presses: &[KeyPress],
        mode: Option<&ModeName>,
        binding: &KeyBinding,
    ) -> bool {
        if (1..presses.len()).any(|len| self.exact_match(&presses[..len], mode).is_some()) {
            return false;
        }

        self.exact_match(presses, mode)
            .is_some_and(|winner| std::ptr::eq(winner, binding))
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
