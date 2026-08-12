//! Key bindings: what a key sequence is bound *to*.
//!
//! Split out of `stroke` because a binding is a different concept from the
//! pattern that matches a keypress — it carries a command, a mode, a mode to
//! enter and a count-prefix flag, none of which a [`StrokePattern`] knows about.

use serde::{Deserialize, Serialize};

use super::{CommandId, KeyPress, KeymapError, ModeName, StrokePattern};

/// A single entry in a [`Keymap`](super::Keymap): a key sequence and what it does.
///
/// The sequence is guaranteed non-empty. In-process construction takes the first
/// stroke as its own parameter, so the guarantee is structural rather than
/// checked; deserialization is the only path that can violate it, and it is
/// rejected there with [`KeymapError::EmptySequence`].
///
/// A binding does up to three things when it matches, any of which may be absent:
///
/// - run a **command** ([`Self::new`]);
/// - switch the resolver's **mode** ([`Self::then_enter_mode`],
///   [`Self::enter_mode`]), which is what makes a modal keymap expressible
///   without the kernel knowing any mode;
/// - accept a numeric **count prefix** ([`Self::with_count_prefix`]), which is
///   what makes `3dw` one binding rather than one binding per count.
///
/// A binding that does none of them is a [`suppression`](Self::unbound): the
/// sequence is treated as unbound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(into = "BindingData")]
pub struct KeyBinding {
    /// The key sequence, at least one stroke long.
    sequence: Vec<StrokePattern>,
    /// The command to run, or `None` when the binding runs nothing.
    command: Option<CommandId>,
    /// The mode the binding is scoped to, or `None` for every mode.
    mode: Option<ModeName>,
    /// The mode to switch to when the binding matches.
    enters_mode: Option<ModeName>,
    /// Whether a decimal count may be typed into this sequence.
    accepts_count: bool,
}

impl KeyBinding {
    /// Binds a sequence starting with `first`, continuing with `rest`, to
    /// `command`.
    ///
    /// `rest` is empty for a single-chord binding such as `Ctrl+C`, and carries
    /// the continuation for a multi-key sequence such as `Ctrl+K Ctrl+D`.
    #[must_use]
    pub fn new(first: StrokePattern, rest: &[StrokePattern], command: CommandId) -> Self {
        Self {
            sequence: Self::build_sequence(first, rest),
            command: Some(command),
            mode: None,
            enters_mode: None,
            accepts_count: false,
        }
    }

    /// Explicitly unbinds a sequence.
    ///
    /// An unbound entry in a higher layer suppresses the same sequence in every
    /// lower layer, which is how a user keymap removes a default binding without
    /// editing the default keymap. Resolution treats the suppressed sequence as
    /// unbound — a single suppressed chord falls through to the host, exactly as
    /// if it had never been bound — and, for a multi-stroke sequence, the
    /// suppression also stops the *prefix* from holding a sequence open, so the
    /// leader no longer waits for a key it can never complete.
    #[must_use]
    pub fn unbound(first: StrokePattern, rest: &[StrokePattern]) -> Self {
        Self {
            sequence: Self::build_sequence(first, rest),
            command: None,
            mode: None,
            enters_mode: None,
            accepts_count: false,
        }
    }

    /// Binds a sequence that only switches the resolver into `mode`.
    ///
    /// The mode transition is data in the keymap, not a command in the kernel:
    /// `i` in mode `normal` entering mode `insert`, and `Escape` in mode `insert`
    /// returning to `normal`, are both this. See
    /// [`KeymapResolver`](super::KeymapResolver) for why the kernel grows no mode
    /// enum.
    #[must_use]
    pub fn enter_mode(first: StrokePattern, rest: &[StrokePattern], mode: ModeName) -> Self {
        Self {
            sequence: Self::build_sequence(first, rest),
            command: None,
            mode: None,
            enters_mode: Some(mode),
            accepts_count: false,
        }
    }

    /// Scopes the binding to `mode`.
    ///
    /// A mode-scoped binding matches only while the resolver's mode equals
    /// `mode`, and outranks a mode-free binding for the same sequence in the same
    /// keymap.
    #[must_use]
    pub fn in_mode(mut self, mode: ModeName) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Switches the resolver to `mode` when this binding matches.
    ///
    /// Combined with [`Self::new`] this is "run the command, then change mode" —
    /// `c` in operator-pending mode, or `v` entering visual mode while also
    /// collapsing the selection.
    #[must_use]
    pub fn then_enter_mode(mut self, mode: ModeName) -> Self {
        self.enters_mode = Some(mode);
        self
    }

    /// Declares that a decimal count may be typed into this sequence.
    ///
    /// With this set, digits typed where no binding consumes them are collected
    /// into [`CommandArgs::count`](super::CommandArgs::count) instead of falling
    /// through to text insertion — before the sequence (`3dw`), inside it
    /// (`d3w`), or both (`2d2w`, whose counts multiply, as in Vim). Without it,
    /// digits are never treated as counts, which is why the non-modal default
    /// keymap types `3` as the character `3`.
    #[must_use]
    pub const fn with_count_prefix(mut self) -> Self {
        self.accepts_count = true;
        self
    }

    /// The key sequence, normalized, never empty.
    #[must_use]
    pub fn sequence(&self) -> &[StrokePattern] {
        &self.sequence
    }

    /// The command this sequence runs, or `None` when it runs none.
    #[must_use]
    pub const fn command(&self) -> Option<&CommandId> {
        self.command.as_ref()
    }

    /// The mode this binding is scoped to, or `None` when it applies to all
    /// modes.
    #[must_use]
    pub const fn mode(&self) -> Option<&ModeName> {
        self.mode.as_ref()
    }

    /// The mode this binding switches to, or `None` when it changes no mode.
    #[must_use]
    pub const fn enters_mode(&self) -> Option<&ModeName> {
        self.enters_mode.as_ref()
    }

    /// Whether a decimal count may be typed into this sequence.
    #[must_use]
    pub const fn accepts_count(&self) -> bool {
        self.accepts_count
    }

    /// Returns `true` when the binding does nothing at all, i.e. it exists only
    /// to unbind its sequence.
    ///
    /// This — not `command().is_none()` — is the test resolution and validation
    /// use, because a binding that only switches mode runs no command and is
    /// nevertheless live.
    #[must_use]
    pub const fn is_suppression(&self) -> bool {
        self.command.is_none() && self.enters_mode.is_none()
    }

    /// Returns `true` when the binding is active for `mode`.
    #[must_use]
    pub fn applies_in(&self, mode: Option<&ModeName>) -> bool {
        self.mode
            .as_ref()
            .is_none_or(|required| mode == Some(required))
    }

    /// Returns `true` when `presses` matches the whole sequence exactly.
    #[must_use]
    pub fn matches_exactly(&self, presses: &[KeyPress]) -> bool {
        self.sequence.len() == presses.len() && self.matches_prefix(presses)
    }

    /// Returns `true` when `presses` matches a leading portion of the sequence.
    ///
    /// A sequence equal in length to `presses` counts as a prefix of itself.
    #[must_use]
    pub fn matches_prefix(&self, presses: &[KeyPress]) -> bool {
        presses.len() <= self.sequence.len()
            && self
                .sequence
                .iter()
                .zip(presses)
                .all(|(pattern, press)| pattern.matches(*press))
    }

    /// Collects the characters `presses` captured, in sequence order.
    ///
    /// Only strokes declaring
    /// [`StrokeCapture::AnyChar`](super::StrokeCapture::AnyChar) contribute, so a
    /// binding
    /// with no wildcard stroke returns an empty vector without allocating — which
    /// is every binding in the default keymap.
    #[must_use]
    pub fn captures_from(&self, presses: &[KeyPress]) -> Vec<char> {
        if !self.has_capture_stroke() {
            return Vec::new();
        }
        self.sequence
            .iter()
            .zip(presses)
            .filter_map(|(pattern, press)| pattern.capture_of(*press))
            .collect()
    }

    /// Returns `true` when a wildcard stroke widens any element of the sequence.
    #[must_use]
    pub fn has_capture_stroke(&self) -> bool {
        self.sequence.iter().any(StrokePattern::captures)
    }

    /// Total modifier specificity of the sequence, used to break ties between
    /// two bindings in one keymap that both match a keypress.
    #[must_use]
    pub fn specificity(&self) -> u32 {
        self.sequence
            .iter()
            .map(|stroke| u32::from(stroke.modifiers.specificity()))
            .sum()
    }

    /// Total number of modifiers the sequence *requires*, the primary tie-break.
    ///
    /// See [`ModifierPattern::required_count`](super::ModifierPattern::required_count)
    /// for why this is ranked above
    /// [`Self::specificity`].
    #[must_use]
    pub fn required_specificity(&self) -> u32 {
        self.sequence
            .iter()
            .map(|stroke| u32::from(stroke.modifiers.required_count()))
            .sum()
    }

    /// Renders the sequence as space-separated chords, e.g. `ctrl+k ctrl+d`.
    ///
    /// The inverse of [`Self::parse_sequence`].
    #[must_use]
    pub fn display_sequence(&self) -> String {
        let mut out = String::new();
        for (index, stroke) in self.sequence.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            out.push_str(&stroke.to_string());
        }
        out
    }

    /// Parses a space-separated key sequence, e.g. `"ctrl-k ctrl-c"`.
    ///
    /// The text form a configuration file or a rebinding UI works in; see
    /// [`StrokePattern`]'s `FromStr` impl for the accepted chord grammar. The
    /// result is
    /// the `(first, rest)` pair the constructors take.
    ///
    /// # Errors
    ///
    /// - [`KeymapError::EmptySequence`] when `text` contains no chord.
    /// - [`KeymapError::UnparsableStroke`] when a chord cannot be parsed.
    pub fn parse_sequence(text: &str) -> Result<(StrokePattern, Vec<StrokePattern>), KeymapError> {
        let mut strokes = Vec::new();
        for chord in text.split_whitespace() {
            strokes.push(chord.parse::<StrokePattern>()?);
        }
        let mut drain = strokes.into_iter();
        let first = drain.next().ok_or_else(|| KeymapError::EmptySequence {
            keymap: "<parsed>".to_owned(),
        })?;
        Ok((first, drain.collect()))
    }

    /// Parses `sequence` and binds it to `command`.
    ///
    /// # Errors
    ///
    /// As [`Self::parse_sequence`].
    pub fn parse(sequence: &str, command: CommandId) -> Result<Self, KeymapError> {
        let (first, rest) = Self::parse_sequence(sequence)?;
        Ok(Self::new(first, &rest, command))
    }

    /// Replaces the command id, used by keymap canonicalization.
    pub(super) fn set_command(&mut self, command: CommandId) {
        self.command = Some(command);
    }

    /// Scopes this binding to `mode`, used by keymap canonicalization.
    ///
    /// Distinct from [`Self::in_mode`] only in taking `&mut self`, because
    /// canonicalization walks bindings in place. The caller decides whether to
    /// call it — [`Keymap::canonicalize`](super::Keymap::canonicalize) does so
    /// only when the binding names no mode of its own, so an author's scoping is
    /// never overwritten.
    pub(super) fn set_mode(&mut self, mode: ModeName) {
        self.mode = Some(mode);
    }

    /// Returns `true` when both bindings name the same sequence and mode scope.
    ///
    /// Sequence *identity* — not overlap — is what a
    /// [`suppression`](Self::unbound) matches, so unbinding `Ctrl+K Ctrl+D`
    /// removes exactly that sequence from lower layers and nothing else.
    pub(super) fn same_sequence_as(&self, other: &Self) -> bool {
        self.sequence == other.sequence && self.mode == other.mode
    }

    /// Builds the normalized, non-empty sequence vector.
    fn build_sequence(first: StrokePattern, rest: &[StrokePattern]) -> Vec<StrokePattern> {
        let mut sequence = Vec::with_capacity(1 + rest.len());
        sequence.push(first.normalized());
        sequence.extend(rest.iter().map(|stroke| stroke.normalized()));
        sequence
    }
}

/// The serialized shape of a [`KeyBinding`].
///
/// A separate type so deserialization can reject an empty sequence instead of
/// admitting a binding that can never match. Every field beyond `keys` is
/// optional and omitted when unset, so configuration written before mode
/// transitions and counts existed still deserializes unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BindingData {
    /// The key sequence.
    keys: Vec<StrokePattern>,
    /// The command, `null` to unbind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    command: Option<CommandId>,
    /// The mode scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<ModeName>,
    /// The mode to switch to on match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enters_mode: Option<ModeName>,
    /// Whether a decimal count may be typed into the sequence.
    // `Not::not` is implemented for `&bool`, which is the shape serde passes.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    count_prefix: bool,
}

impl From<KeyBinding> for BindingData {
    fn from(binding: KeyBinding) -> Self {
        Self {
            keys: binding.sequence,
            command: binding.command,
            mode: binding.mode,
            enters_mode: binding.enters_mode,
            count_prefix: binding.accepts_count,
        }
    }
}

impl<'de> Deserialize<'de> for KeyBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        let data = BindingData::deserialize(deserializer)?;
        let Some((first, rest)) = data.keys.split_first() else {
            return Err(D::Error::custom(
                KeymapError::EmptySequence {
                    keymap: "<deserialized>".to_owned(),
                }
                .to_string(),
            ));
        };
        Ok(Self {
            sequence: Self::build_sequence(*first, rest),
            command: data.command,
            mode: data.mode,
            enters_mode: data.enters_mode,
            accepts_count: data.count_prefix,
        })
    }
}
