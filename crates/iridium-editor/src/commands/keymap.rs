//! A single keymap layer: an ordered table of key sequences to command ids.

use std::borrow::Cow;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{CommandRegistry, KeyBinding, KeyPress, KeymapError, ModeName};
use crate::input::KeyCode;

/// The tie-break key for two bindings in one layer that both match a keypress, in
/// descending priority: mode-scoped, exact rather than wildcard, modifiers
/// required, modifiers constrained, insertion order. Compared as a tuple, so the
/// documented precedence rules are the field order and cannot drift from them.
type BindingRank = (bool, bool, u32, u32, usize);

/// One layer of bindings from key sequences to command ids.
///
/// A keymap is **data**: it holds no behaviour, only the mapping. The default
/// non-modal keymap ([`default_non_modal_keymap`](super::default_non_modal_keymap))
/// is one of these built in code; a user keymap is one of these deserialized from
/// configuration; a modal keymap is one of these whose bindings name modes. All
/// three are the same type and are composed by a
/// [`KeymapStack`](super::KeymapStack).
///
/// # Lookup cost
///
/// Bindings whose first stroke names an exact key are indexed by that
/// [`KeyCode`], so a keypress narrows to the handful of bindings that could
/// possibly involve that key (two for `Char('k')`, four for `Up`) before any
/// modifier pattern is tested. Resolution is a single hash probe plus a short
/// linear scan over `Copy` data, and allocates nothing. Bindings whose first
/// stroke is a [`capture wildcard`](super::StrokeCapture::AnyChar) cannot be
/// indexed by key and are held in a separate list that is scanned as well; the
/// default keymap has none, so the ordinary path is unchanged.
///
/// # Precedence inside one layer
///
/// When several bindings in one keymap match the same keypress sequence, the
/// winner is chosen deterministically:
///
/// 1. a binding scoped to the active mode beats a mode-free binding;
/// 2. then a binding matching its keys exactly beats one that matched through a
///    capture wildcard, so `dd` still wins over `d{char}`;
/// 3. then the binding requiring more modifiers wins
///    ([`KeyBinding::required_specificity`]) — this is what makes `Alt+Up` (move
///    the line) beat the looser `Up` (move the caret) without either binding
///    referring to the other;
/// 4. then the higher total [`constraint count`](KeyBinding::specificity) wins,
///    which separates two bindings that require the same modifiers but differ in
///    what they forbid;
/// 5. then the binding added **later** wins, so appending to a keymap overrides
///    what came before it, matching how the layer stack itself works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "KeymapData", into = "KeymapData")]
pub struct Keymap {
    /// A human-facing name for diagnostics (`"default"`, `"user"`, `"vim"`).
    name: Cow<'static, str>,
    /// Bindings in insertion order.
    bindings: Vec<KeyBinding>,
    /// First-stroke [`KeyCode`] to indices into [`Self::bindings`].
    index: HashMap<KeyCode, Vec<usize>>,
    /// Indices of bindings whose first stroke is a capture wildcard, which no
    /// [`KeyCode`] can index.
    wildcard_index: Vec<usize>,
    /// Modes in which a printable key no binding claimed must **not** insert
    /// itself. See [`Keymap::silence_typing_in`].
    silent_modes: Vec<ModeName>,
    /// The mode a session using this keymap begins in, if it names one. See
    /// [`Keymap::set_initial_mode`].
    initial_mode: Option<ModeName>,
}

impl Keymap {
    /// Creates an empty keymap.
    #[must_use]
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            bindings: Vec::new(),
            index: HashMap::new(),
            wildcard_index: Vec::new(),
            silent_modes: Vec::new(),
            initial_mode: None,
        }
    }

    /// The keymap's name, used in diagnostics.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The bindings, in insertion order.
    #[must_use]
    pub fn bindings(&self) -> &[KeyBinding] {
        &self.bindings
    }

    /// The number of bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns `true` when the keymap has no bindings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Appends a binding.
    ///
    /// Later bindings win over earlier ones for the same sequence and mode; see
    /// the precedence rules on [`Keymap`].
    pub fn push(&mut self, binding: KeyBinding) {
        // `KeyBinding` guarantees a non-empty, normalized sequence.
        if let Some(first) = binding.sequence().first() {
            let index = self.bindings.len();
            if first.captures() {
                self.wildcard_index.push(index);
            } else {
                self.index.entry(first.key).or_default().push(index);
            }
            self.bindings.push(binding);
        }
    }

    /// Appends every binding in `bindings`.
    pub fn extend(&mut self, bindings: impl IntoIterator<Item = KeyBinding>) {
        for binding in bindings {
            self.push(binding);
        }
    }

    /// Declares that in `mode`, a printable key no binding claimed does nothing
    /// instead of inserting itself.
    ///
    /// # Why this is a keymap's declaration and not the kernel's rule
    ///
    /// Typing is **not a binding**: self-insert happens after resolution
    /// declines a key, so a normal mode cannot switch typing off by binding
    /// keys — and it cannot switch it off with
    /// [`suppressions`](KeyBinding::unbound) either, because a suppressed
    /// sequence is *unbound*, which is precisely the state that falls through to
    /// self-insert. Something outside the binding table has to say so.
    ///
    /// Saying it here rather than in the dispatcher keeps the promise the whole
    /// mode design rests on: a modal keymap is expressible **without the kernel
    /// knowing any mode**. The kernel asks a question; the keymap answers it.
    ///
    /// A mode named here is silent for as long as this layer is on the stack.
    /// Nothing is silent by default, so a keymap that never calls this — every
    /// keymap that exists today — types exactly as it always has.
    pub fn silence_typing_in(&mut self, mode: ModeName) {
        if !self.silent_modes.contains(&mode) {
            self.silent_modes.push(mode);
        }
    }

    /// Whether a printable key no binding claimed inserts itself in `mode`.
    ///
    /// `true` unless [`Self::silence_typing_in`] named `mode`. Always `true` for
    /// `None`: a keymap with no modes has no mode to silence, which is the
    /// invariant that keeps a non-modal keymap unable to stop typing by
    /// accident.
    #[must_use]
    pub fn types_unclaimed_keys(&self, mode: Option<&ModeName>) -> bool {
        mode.is_none_or(|mode| !self.silent_modes.contains(mode))
    }

    /// The modes this keymap has silenced, in the order they were named.
    #[must_use]
    pub fn silent_modes(&self) -> &[ModeName] {
        &self.silent_modes
    }

    /// Declares the mode a session using this keymap begins in.
    ///
    /// A modal keymap that silences typing in `normal` but does not say a
    /// session *starts* in `normal` is only half a keymap: it works if whoever
    /// installs it happens to remember the name, and fails silently — as a
    /// document that types normally and answers to no motion — if they do not.
    /// The two facts belong together, so they live together.
    ///
    /// Declaring it does not apply it. Which mode a face is in, and when, is the
    /// face's business; this is the keymap telling it what to ask for.
    pub fn set_initial_mode(&mut self, mode: Option<ModeName>) {
        self.initial_mode = mode;
    }

    /// The mode a session using this keymap begins in, or `None` for a keymap
    /// that names no mode at all.
    #[must_use]
    pub const fn initial_mode(&self) -> Option<&ModeName> {
        self.initial_mode.as_ref()
    }

    /// Returns the binding that matches `presses` exactly, or `None`.
    ///
    /// `presses` must be the complete pending sequence. Ties are broken by the
    /// precedence rules documented on [`Keymap`]. A returned binding may be an
    /// [`unbound`](KeyBinding::unbound) entry, whose
    /// [`command`](KeyBinding::command) is `None`; that is a *suppression*, and
    /// the caller must treat the sequence as unbound rather than falling through
    /// to a lower layer.
    #[must_use]
    pub fn exact_match<'s>(
        &'s self,
        presses: &[KeyPress],
        mode: Option<&ModeName>,
    ) -> Option<&'s KeyBinding> {
        let mut best: Option<(&'s KeyBinding, BindingRank)> = None;
        for (index, binding) in self.candidates(presses) {
            if binding.sequence().len() != presses.len() || !binding.applies_in(mode) {
                continue;
            }
            let rank: BindingRank = (
                binding.mode().is_some(),
                !binding.has_capture_stroke(),
                binding.required_specificity(),
                binding.specificity(),
                index,
            );
            if best.is_none_or(|(_, best_rank)| rank > best_rank) {
                best = Some((binding, rank));
            }
        }
        best.map(|(binding, _)| binding)
    }

    /// Returns `true` when some live binding continues past `presses`.
    ///
    /// This is what keeps a multi-key sequence pending: `Ctrl+K` is not a
    /// binding of its own, but `Ctrl+K Ctrl+D` is, so `Ctrl+K` is a live prefix.
    /// Suppressed ([`unbound`](KeyBinding::unbound)) entries do not hold a
    /// sequence open.
    #[must_use]
    pub fn has_continuation(&self, presses: &[KeyPress], mode: Option<&ModeName>) -> bool {
        self.continuations(presses, mode).next().is_some()
    }

    /// Yields every live binding that continues past `presses`.
    ///
    /// Exposed so a [`KeymapStack`](super::KeymapStack) can filter the
    /// continuations of a lower layer against the suppressions of a higher one:
    /// unbinding `Ctrl+K Ctrl+D` must stop `Ctrl+K` holding a sequence open, and
    /// only the stack can see both halves.
    pub fn continuations<'s, 'p>(
        &'s self,
        presses: &'p [KeyPress],
        mode: Option<&'p ModeName>,
    ) -> impl Iterator<Item = &'s KeyBinding> + use<'s, 'p> {
        self.candidates(presses).filter_map(move |(_, binding)| {
            (binding.sequence().len() > presses.len()
                && !binding.is_suppression()
                && binding.applies_in(mode))
            .then_some(binding)
        })
    }

    /// Returns `true` when some binding starting with `presses` accepts a count.
    ///
    /// Consulted before a digit is treated as a count digit: with no
    /// count-accepting binding in reach, a digit is an ordinary keypress and must
    /// fall through to typing, which is why the non-modal default keymap types
    /// `3` as `3`.
    #[must_use]
    pub fn accepts_count_at(&self, presses: &[KeyPress], mode: Option<&ModeName>) -> bool {
        if presses.is_empty() {
            return self
                .bindings
                .iter()
                .any(|binding| binding.accepts_count() && binding.applies_in(mode));
        }
        self.candidates(presses)
            .any(|(_, binding)| binding.accepts_count() && binding.applies_in(mode))
    }

    /// Returns `true` when this layer explicitly unbinds `binding`'s sequence.
    ///
    /// Sequence identity, not overlap: a suppression removes exactly the sequence
    /// it names from lower layers.
    #[must_use]
    pub fn suppresses(&self, binding: &KeyBinding) -> bool {
        self.bindings
            .iter()
            .any(|entry| entry.is_suppression() && entry.same_sequence_as(binding))
    }

    /// Yields `(insertion index, binding)` for every binding whose sequence
    /// starts with `presses`.
    ///
    /// Narrows by the first stroke's [`KeyCode`] through the index, then tests
    /// the modifier patterns; bindings led by a capture wildcard are appended
    /// from their own list because no key indexes them. Allocation free.
    fn candidates<'s, 'p>(
        &'s self,
        presses: &'p [KeyPress],
    ) -> impl Iterator<Item = (usize, &'s KeyBinding)> + use<'s, 'p> {
        let first = presses.first().map(|press| press.normalized().key);
        let exact = first
            .and_then(|key| self.index.get(&key))
            .into_iter()
            .flatten();
        exact
            .chain(self.wildcard_index.iter())
            .filter_map(move |&index| {
                let binding = self.bindings.get(index)?;
                binding.matches_prefix(presses).then_some((index, binding))
            })
    }

    /// Checks that every bound command exists and that no binding is unreachable.
    ///
    /// # Errors
    ///
    /// - [`KeymapError::UnknownCommand`] when a binding names a command that is
    ///   not registered. Catching this at load time is the whole point: a typo in
    ///   a user keymap must be a startup diagnostic, not a key that silently does
    ///   nothing.
    /// - [`KeymapError::ShadowedSequence`] when one binding's sequence can be
    ///   matched as a complete sequence while being a strict prefix of another
    ///   binding in the same mode, which would make the longer binding
    ///   unreachable.
    pub fn validate(&self, registry: &CommandRegistry) -> Result<(), KeymapError> {
        for binding in &self.bindings {
            if let Some(id) = binding.command() {
                if !registry.contains(id.as_str()) {
                    return Err(KeymapError::UnknownCommand {
                        keymap: self.name.to_string(),
                        id: id.as_str().to_owned(),
                    });
                }
            }
        }
        self.check_shadowing()
    }

    /// Reports the first binding rendered unreachable by a shorter binding.
    fn check_shadowing(&self) -> Result<(), KeymapError> {
        for shorter in &self.bindings {
            for longer in &self.bindings {
                if !shadows(shorter, longer) {
                    continue;
                }
                return Err(KeymapError::ShadowedSequence {
                    keymap: self.name.to_string(),
                    prefix: shorter.display_sequence(),
                    shadowed: longer.display_sequence(),
                });
            }
        }
        Ok(())
    }

    /// Replaces every bound id with the registry's own `'static` instance.
    ///
    /// Deserialized keymaps own their command id strings; cloning one on a match
    /// would allocate on the keystroke hot path. Canonicalizing after load swaps
    /// each owned id for the registry's borrowed one — which both removes the
    /// allocation and validates the id, so this is the single call a config
    /// loader needs.
    ///
    /// # Errors
    ///
    /// [`KeymapError::UnknownCommand`] if any bound id is not registered. On
    /// error the keymap is left partially canonicalized but semantically
    /// unchanged, since canonicalization only ever swaps equal strings.
    pub fn canonicalize(&mut self, registry: &CommandRegistry) -> Result<(), KeymapError> {
        let name = self.name.to_string();
        for binding in &mut self.bindings {
            let Some(id) = binding.command().cloned() else {
                continue;
            };
            let Some(canonical) = registry.canonical_id(id.as_str()) else {
                return Err(KeymapError::UnknownCommand {
                    keymap: name,
                    id: id.as_str().to_owned(),
                });
            };
            let canonical = canonical.clone();
            binding.set_command(canonical);
        }
        Ok(())
    }

    /// Rebuilds the first-stroke index from [`Self::bindings`].
    fn rebuild_index(&mut self) {
        self.index.clear();
        self.wildcard_index.clear();
        for (position, binding) in self.bindings.iter().enumerate() {
            if let Some(first) = binding.sequence().first() {
                if first.captures() {
                    self.wildcard_index.push(position);
                } else {
                    self.index.entry(first.key).or_default().push(position);
                }
            }
        }
    }
}

/// Returns `true` when `shorter`, bound as a complete sequence, makes `longer`
/// unreachable.
///
/// The rule resolution imposes: an exact match fires the instant it completes, so
/// a live shorter binding whose strokes all overlap the head of a live longer
/// binding wins forever. Suppressions on either side are ignored — a suppressed
/// prefix is not bound, and a suppressed longer sequence was already removed.
///
/// Shared by the per-layer check and the cross-layer check in
/// [`KeymapStack::validate`](super::KeymapStack::validate) so the two cannot
/// diverge.
pub(super) fn shadows(shorter: &KeyBinding, longer: &KeyBinding) -> bool {
    if shorter.is_suppression()
        || longer.is_suppression()
        || longer.sequence().len() <= shorter.sequence().len()
    {
        return false;
    }
    // A mode-free prefix shadows in every mode; a mode-scoped prefix shadows only
    // bindings in that same mode.
    if shorter.mode().is_some() && shorter.mode() != longer.mode() {
        return false;
    }
    shorter
        .sequence()
        .iter()
        .zip(longer.sequence())
        .all(|(short, long)| short.overlaps(long))
}

/// The serialized shape of a [`Keymap`]: name plus bindings, no index.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeymapData {
    /// The keymap name.
    name: String,
    /// The bindings, in precedence order (later wins).
    bindings: Vec<KeyBinding>,
    /// The modes in which an unclaimed printable key does not self-insert.
    ///
    /// Defaulted so that every keymap serialized before modes existed still
    /// deserializes, and so a keymap that silences nothing writes nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    silent_modes: Vec<ModeName>,
    /// The mode a session using this keymap begins in.
    ///
    /// Defaulted and skipped when absent for the same reason as
    /// [`Self::silent_modes`]: a keymap with no modes writes nothing about them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    initial_mode: Option<ModeName>,
}

impl From<Keymap> for KeymapData {
    fn from(keymap: Keymap) -> Self {
        Self {
            name: keymap.name.into_owned(),
            bindings: keymap.bindings,
            silent_modes: keymap.silent_modes,
            initial_mode: keymap.initial_mode,
        }
    }
}

impl From<KeymapData> for Keymap {
    fn from(data: KeymapData) -> Self {
        let mut keymap = Self {
            name: Cow::Owned(data.name),
            bindings: data.bindings,
            index: HashMap::new(),
            wildcard_index: Vec::new(),
            silent_modes: data.silent_modes,
            initial_mode: data.initial_mode,
        };
        keymap.rebuild_index();
        keymap
    }
}
