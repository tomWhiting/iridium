//! One key sequence that actually runs a command.

use serde::Serialize;

use super::label::{self, KeyLabelStyle};
use crate::commands::{KeyBinding, ModeName, StrokePattern};

/// A key sequence that runs a command, resolved against a whole keymap stack.
///
/// The guarantee that makes this worth having: a `KeyHint` exists only for a
/// binding that
/// [`KeymapStack::binding_is_reachable`](crate::commands::KeymapStack::binding_is_reachable)
/// confirmed will fire. A palette showing one is making a promise it can keep.
///
/// The sequence is owned rather than borrowed because the index is stored beside
/// the [`KeymapStack`](crate::commands::KeymapStack) it was built from, and a
/// struct cannot borrow from its sibling. Bindings are short and the index is
/// rebuilt only when a layer is pushed or popped, so the copy costs nothing that
/// matters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyHint {
    /// The strokes to type, in order.
    #[serde(skip)]
    sequence: Vec<StrokePattern>,
    /// The mode this sequence applies in, or `None` for every mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<ModeName>,
    /// Index of the layer the binding came from; higher wins.
    #[serde(skip)]
    layer: usize,
    /// Whether any stroke matches a whole class of keys rather than one.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    wildcard: bool,
    /// The human-readable label, e.g. `Ctrl+K`.
    label: String,
    /// The same sequence in macOS glyphs, e.g. `⌘K`.
    mac_label: String,
    /// The round-trippable text form, e.g. `ctrl+~altgraph+k`.
    binding_text: String,
}

impl KeyHint {
    /// Describes `binding`, which must have been found in layer `layer`.
    ///
    /// Both label forms are rendered once, here, rather than on demand: a
    /// palette re-reads them on every filter keystroke, and the host must never
    /// re-derive a chord's spelling for itself.
    pub(super) fn new(binding: &KeyBinding, layer: usize) -> Self {
        let sequence = binding.sequence().to_vec();
        Self {
            label: label::render(&sequence, KeyLabelStyle::Portable),
            mac_label: label::render(&sequence, KeyLabelStyle::MacGlyphs),
            binding_text: binding.display_sequence(),
            wildcard: binding.has_capture_stroke(),
            mode: binding.mode().cloned(),
            layer,
            sequence,
        }
    }

    /// The strokes to type, in order. Never empty.
    #[must_use]
    pub fn sequence(&self) -> &[StrokePattern] {
        &self.sequence
    }

    /// The mode this sequence applies in, or `None` for every mode.
    #[must_use]
    pub const fn mode(&self) -> Option<&ModeName> {
        self.mode.as_ref()
    }

    /// Index of the keymap layer the binding came from; higher takes precedence.
    #[must_use]
    pub const fn layer(&self) -> usize {
        self.layer
    }

    /// Whether some stroke matches a class of keys rather than one key.
    ///
    /// A wildcard sequence cannot be typed literally — its label carries a
    /// placeholder — so a host that wants to show only pressable chords filters
    /// on this.
    #[must_use]
    pub const fn is_wildcard(&self) -> bool {
        self.wildcard
    }

    /// The label in `style`, already rendered.
    #[must_use]
    pub fn label(&self, style: KeyLabelStyle) -> &str {
        match style {
            KeyLabelStyle::Portable => &self.label,
            KeyLabelStyle::MacGlyphs => &self.mac_label,
        }
    }

    /// The round-trippable text form, as
    /// [`KeyBinding::display_sequence`] renders it.
    ///
    /// This is the form a keymap file uses, **not** the one to show a user; see
    /// the [module documentation](super::super::hints) for why the two differ.
    #[must_use]
    pub fn binding_text(&self) -> &str {
        &self.binding_text
    }

    /// Orders two hints for the same command, best first.
    ///
    /// A command may be bound more than once — a higher layer adding an
    /// alternative, or the default keymap offering both `Ctrl+P` and `Ctrl+K` —
    /// and a palette shows one. The ranking, in order:
    ///
    /// 1. **Higher layer first.** A user's own binding is the one they expect to
    ///    see, even when the default still works.
    /// 2. **Literal before wildcard.** A wildcard cannot be shown as a chord.
    /// 3. **Shorter sequence first.** One chord beats two.
    /// 4. **Fewer modifiers first.** `Ctrl+P` beats `Ctrl+Shift+Alt+P`.
    ///
    /// Ties beyond that are broken by insertion order at the call site, which
    /// keeps the result stable across rebuilds.
    pub(super) fn rank(&self) -> (std::cmp::Reverse<usize>, bool, usize, usize) {
        (
            std::cmp::Reverse(self.layer),
            self.wildcard,
            self.sequence.len(),
            self.required_modifier_count(),
        )
    }

    /// How many modifiers the whole sequence requires a person to hold.
    fn required_modifier_count(&self) -> usize {
        self.sequence
            .iter()
            .map(|stroke| usize::from(stroke.modifiers.required_count()))
            .sum()
    }
}
