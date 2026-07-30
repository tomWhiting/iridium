//! The key-sequence resolution state machine.

use super::{
    CommandArgs, CommandId, CommandInvocation, KeyPress, KeymapStack, ModeName, ModifierPattern,
    StrokePattern,
};
use crate::input::KeyCode;

/// The outcome of feeding one keypress to a [`KeymapResolver`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A complete sequence matched; run this command with these arguments.
    ///
    /// The pending buffer has been cleared. The invocation carries the count and
    /// captured characters the sequence collected, which is what lets one binding
    /// serve `dw`, `3dw` and `f{char}`; a binding that declares neither yields
    /// [`CommandArgs::NONE`].
    Matched(CommandInvocation),

    /// A complete sequence matched and only switched the editing mode.
    ///
    /// Produced by a [`KeyBinding::enter_mode`](super::KeyBinding::enter_mode)
    /// binding. The mode has **already** been applied to the resolver; the name is
    /// reported so a host can render a mode indicator. The keypress was consumed.
    ModeEntered(ModeName),

    /// The keys so far are a live prefix of at least one binding, or a count is
    /// being typed.
    ///
    /// The keypress was **consumed**: the host must not treat it as text input,
    /// and should surface the pending sequence (see [`KeymapResolver::pending`])
    /// so the user can see the editor is waiting.
    Pending,

    /// A pending sequence was cancelled and the keypress consumed.
    ///
    /// Produced when the abort stroke arrives while a sequence is pending.
    /// Nothing runs; the buffer is cleared. Distinct from [`Self::NoMatch`]
    /// because the keypress must *not* fall through to text insertion — the user
    /// was mid-chord and asked to leave.
    ///
    /// A sequence that reaches a *dead end* does **not** produce this: see
    /// [`KeymapResolver::resolve`] for why the final keystroke is replayed
    /// instead of discarded.
    ///
    /// # Escape takes two presses when a chord is pending
    ///
    /// The abort stroke is tested *before* the keymap, and only while something is
    /// pending. So with the default keymap, `Escape` after `Ctrl+K` leaves the
    /// sequence and does **not** also collapse to the primary cursor; a user with
    /// multiple cursors who abandons a chord presses `Escape` twice. That is a
    /// recorded decision, not an oversight: making the abort stroke fall through to
    /// its own binding would mean the key that cancels a sequence also *acts*, and
    /// a user cancelling a mistyped chord has not asked for their cursors to be
    /// dropped.
    Aborted,

    /// No binding matched and nothing was pending; the keypress is untouched.
    ///
    /// The caller owns it. This is the fall-through that keeps typing working:
    /// the input layer inserts the character from the **original**
    /// [`KeyEvent`](crate::input::KeyEvent), not from the normalized
    /// [`KeyPress`], and ignores the key entirely when it carries no text.
    NoMatch,
}

impl Resolution {
    /// Builds an argument-free [`Self::Matched`], the common case in tests and in
    /// hosts that construct a resolution by hand.
    #[must_use]
    pub const fn matched(command: CommandId) -> Self {
        Self::Matched(CommandInvocation::bare(command))
    }

    /// Returns `true` when the resolver consumed the keypress.
    ///
    /// True for everything except [`Self::NoMatch`].
    #[must_use]
    pub const fn consumed(&self) -> bool {
        !matches!(self, Self::NoMatch)
    }

    /// Returns the matched command, if any.
    #[must_use]
    pub const fn command(&self) -> Option<&CommandId> {
        match self {
            Self::Matched(invocation) => Some(invocation.command()),
            Self::ModeEntered(_) | Self::Pending | Self::Aborted | Self::NoMatch => None,
        }
    }

    /// Returns the arguments the matched sequence carried, if any.
    #[must_use]
    pub const fn args(&self) -> Option<&CommandArgs> {
        match self {
            Self::Matched(invocation) => Some(invocation.args()),
            Self::ModeEntered(_) | Self::Pending | Self::Aborted | Self::NoMatch => None,
        }
    }
}

/// Feeds keypresses through a [`KeymapStack`], holding the pending-sequence state.
///
/// One resolver instance per input focus. It owns the keys pressed so far in an
/// incomplete sequence, the count being typed into it, and the active mode.
///
/// # Multi-key sequences
///
/// Sequences work from the first keypress, because retrofitting them into
/// single-chord dispatch later is the expensive mistake this layer exists to
/// avoid. Both shapes the editor needs are carried by the same machine:
///
/// - **prefix chords** — `Ctrl+K Ctrl+D`: `Ctrl+K` returns [`Resolution::Pending`],
///   `Ctrl+D` completes it;
/// - **modal grammar** — a Vim keymap expresses `d2w` through *modes*, *counts*
///   and *captures*, not through timing. `d` in mode `normal` starts a sequence;
///   digits typed where no binding consumes them accumulate into the count of a
///   binding that declared [`with_count_prefix`](super::KeyBinding::with_count_prefix);
///   a [`capture wildcard`](super::StrokeCapture::AnyChar) stroke matches any
///   character and passes it to the command, which is what makes `f{char}` and
///   `di{delim}` one binding each; and a binding may switch the mode
///   ([`then_enter_mode`](super::KeyBinding::then_enter_mode)), which is how the
///   grammar advances without the kernel growing a mode enum.
///
/// # Timeout policy: there is none, deliberately
///
/// A pending sequence waits indefinitely. An exact match fires the instant it
/// completes rather than pausing to see whether a longer sequence follows, so no
/// keystroke is ever delayed by a timer — which is the whole point of an editor
/// whose stated target is sub-8ms input latency and "the keys appear before I hit
/// them". The cost of that choice is that a binding which is a strict prefix of
/// another binding would make the longer one unreachable; rather than paper over
/// that with a timeout, [`Keymap::validate`](super::Keymap::validate) and
/// [`KeymapStack::validate`](super::KeymapStack::validate) detect it — within a
/// layer and across layers — and report
/// [`KeymapError::ShadowedSequence`](super::KeymapError::ShadowedSequence) at
/// load time.
///
/// A sequence is left by pressing the abort stroke (`Escape` by default, in any
/// modifier combination), by typing a key that cannot continue it (which
/// *replays* that key rather than eating it, see [`Self::resolve`]), or by an
/// explicit [`Self::abort_pending`] / [`Self::reset`] from the host — for
/// instance when focus is lost, since a chord must not survive the user clicking
/// somewhere else.
///
/// # Hot path
///
/// [`Self::resolve`] allocates nothing for a binding that captures nothing: the
/// pending buffer is reused across keystrokes, the returned [`CommandId`] is
/// cloned from the binding (a pointer copy for the compiled default keymap and
/// for any keymap that has been [`canonicalized`](super::Keymap::canonicalize)),
/// and [`CommandArgs`] holds an empty `Vec`, which does not allocate.
#[derive(Debug, Clone)]
pub struct KeymapResolver {
    /// Keys pressed so far in an incomplete sequence, exactly as the host
    /// reported them.
    pending: Vec<KeyPress>,
    /// The count committed so far, as a product of the digit groups typed.
    count: Option<u32>,
    /// The digit group currently being typed, not yet folded into
    /// [`Self::count`].
    digits: Option<u32>,
    /// The active mode, or `None` for a non-modal keymap.
    mode: Option<ModeName>,
    /// The stroke that cancels a pending sequence.
    abort: StrokePattern,
}

impl Default for KeymapResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapResolver {
    /// The pending-buffer capacity reserved up front.
    ///
    /// Long enough for every sequence a keymap realistically uses, so the buffer
    /// never reallocates during a chord.
    const PENDING_CAPACITY: usize = 8;

    /// Creates a resolver with no pending sequence, no mode, and `Escape` as the
    /// abort stroke.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: Vec::with_capacity(Self::PENDING_CAPACITY),
            count: None,
            digits: None,
            mode: None,
            abort: StrokePattern::new(KeyCode::Escape, ModifierPattern::ANY),
        }
    }

    /// Returns a resolver that uses `abort` to cancel pending sequences.
    ///
    /// Override the default when a keymap needs `Escape` itself to be bindable
    /// inside a sequence: the abort stroke is tested *before* the keymap, so
    /// whatever it is cannot appear as a continuation.
    #[must_use]
    pub const fn with_abort_stroke(mut self, abort: StrokePattern) -> Self {
        self.abort = abort;
        self
    }

    /// The stroke that cancels a pending sequence.
    #[must_use]
    pub const fn abort_stroke(&self) -> StrokePattern {
        self.abort
    }

    /// Sets the stroke that cancels a pending sequence.
    pub const fn set_abort_stroke(&mut self, abort: StrokePattern) {
        self.abort = abort;
    }

    /// The active mode, or `None` when no mode is set.
    #[must_use]
    pub const fn mode(&self) -> Option<&ModeName> {
        self.mode.as_ref()
    }

    /// Sets the active mode, discarding any pending sequence and count.
    ///
    /// The pending prefix was resolved against the *previous* mode's bindings, so
    /// carrying it across a mode change could complete a sequence the user never
    /// typed.
    pub fn set_mode(&mut self, mode: Option<ModeName>) {
        self.mode = mode;
        self.clear_sequence();
    }

    /// The keys pressed so far in an incomplete sequence, as the host reported
    /// them.
    ///
    /// Empty unless the last [`Self::resolve`] returned [`Resolution::Pending`].
    /// A host renders this as the "waiting for another key" indicator. The presses
    /// are **not** normalized, because the capture strokes that read them need the
    /// character the user actually typed.
    #[must_use]
    pub fn pending(&self) -> &[KeyPress] {
        &self.pending
    }

    /// The count typed so far, including a digit group still being entered.
    ///
    /// A host renders this next to [`Self::pending`] so `2d` shows as `2d`.
    #[must_use]
    pub const fn pending_count(&self) -> Option<u32> {
        match (self.count, self.digits) {
            (None, None) => None,
            (count, None) => count,
            (None, digits) => digits,
            (Some(count), Some(digits)) => Some(count.saturating_mul(digits)),
        }
    }

    /// Returns `true` when a sequence or a count is in progress.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty() || self.digits.is_some() || self.count.is_some()
    }

    /// Cancels any pending sequence and count.
    ///
    /// Returns `true` when something was actually cancelled. Call this from
    /// every host path that can invalidate the input context without a keypress —
    /// focus loss, a mouse click, a host-driven cursor jump — so a half-typed
    /// chord cannot complete much later against unrelated state.
    pub fn abort_pending(&mut self) -> bool {
        let was_pending = self.is_pending();
        self.clear_sequence();
        was_pending
    }

    /// Clears the pending sequence, the count, and the active mode.
    pub fn reset(&mut self) {
        self.clear_sequence();
        self.mode = None;
    }

    /// Feeds one keypress to the state machine.
    ///
    /// Equivalent to [`Self::resolve_repeat`] with `is_repeat` false; use that
    /// overload when the host can tell an auto-repeat from a deliberate second
    /// press.
    ///
    /// Resolution order, in full:
    ///
    /// 1. While a sequence is pending, the abort stroke cancels it →
    ///    [`Resolution::Aborted`].
    /// 2. An auto-repeat of the stroke already at the head of the pending buffer
    ///    is swallowed → [`Resolution::Pending`], unchanged. Holding a chord
    ///    leader down must not turn into a second, different chord.
    /// 3. A decimal digit that no binding can consume, where some reachable
    ///    binding declared [`with_count_prefix`](super::KeyBinding::with_count_prefix),
    ///    is folded into the count → [`Resolution::Pending`].
    /// 4. The press is appended to the pending buffer.
    /// 5. If some layer has an exact match for the whole buffer, the
    ///    highest-precedence one decides: a bound command →
    ///    [`Resolution::Matched`]; a mode-only binding →
    ///    [`Resolution::ModeEntered`]; a
    ///    [`suppression`](super::KeyBinding::unbound) means the sequence is
    ///    unbound and resolution continues at step 6, so unbinding a chord does
    ///    not strand a longer sequence that shares it.
    /// 6. Otherwise, if any layer can continue the buffer →
    ///    [`Resolution::Pending`].
    /// 7. Otherwise the sequence is dead. The buffer and count are cleared and
    ///    the keypress is **retried from scratch**, so a mistyped chord costs the
    ///    user the leader only: `Ctrl+K` then `h` inserts `h`, and `Ctrl+K` then
    ///    `Ctrl+D`-that-is-really-`Ctrl+Shift+D` runs the `Ctrl+Shift+D` binding
    ///    instead of vanishing. Discarding it instead would silently destroy a
    ///    keystroke the user can see no reason to have lost — which in a document
    ///    editor is data loss, not a minor infelicity.
    pub fn resolve(&mut self, keymap: &KeymapStack, press: KeyPress) -> Resolution {
        self.resolve_repeat(keymap, press, false)
    }

    /// Feeds one keypress, distinguishing an auto-repeat from a real press.
    ///
    /// See [`Self::resolve`] for the resolution order. `is_repeat` comes from
    /// [`KeyEvent::is_repeat`](crate::input::KeyEvent); hosts that cannot observe
    /// it pass `false`, which is exactly [`Self::resolve`].
    pub fn resolve_repeat(
        &mut self,
        keymap: &KeymapStack,
        press: KeyPress,
        is_repeat: bool,
    ) -> Resolution {
        if self.is_pending() && self.abort.matches(press) {
            self.clear_sequence();
            return Resolution::Aborted;
        }

        if is_repeat && self.repeats_head(press) {
            return Resolution::Pending;
        }

        if self.absorb_count_digit(keymap, press) {
            return Resolution::Pending;
        }
        self.commit_digits();

        self.step(keymap, press)
    }

    /// Appends `press` and resolves the buffer, retrying once from empty when the
    /// sequence turns out to be dead.
    ///
    /// The loop runs at most twice: the retry starts from an empty buffer, where a
    /// failure is a single unmatched chord and returns
    /// [`Resolution::NoMatch`] rather than dead-ending again.
    fn step(&mut self, keymap: &KeymapStack, press: KeyPress) -> Resolution {
        loop {
            self.pending.push(press);

            if let Some(binding) = keymap.exact_match(&self.pending, self.mode.as_ref()) {
                if !binding.is_suppression() {
                    let command = binding.command().cloned();
                    let captures = binding.captures_from(&self.pending);
                    let entered = binding.enters_mode().cloned();
                    let count = self.count;
                    self.clear_sequence();
                    if entered.is_some() {
                        self.mode.clone_from(&entered);
                    }
                    return command.map_or_else(
                        // `is_suppression` ruled out "neither", so a binding with
                        // no command has a mode transition.
                        || entered.map_or(Resolution::NoMatch, Resolution::ModeEntered),
                        |id| {
                            Resolution::Matched(CommandInvocation::new(
                                id,
                                CommandArgs::new(count, captures),
                            ))
                        },
                    );
                }
                // A suppression means the sequence is unbound, not that resolution
                // stops: a longer sequence sharing this prefix may still be live.
            }

            if keymap.has_continuation(&self.pending, self.mode.as_ref()) {
                return Resolution::Pending;
            }

            let depth = self.pending.len();
            self.clear_sequence();
            if depth == 1 {
                return Resolution::NoMatch;
            }
            // Dead end mid-sequence: retry this keystroke as a fresh sequence so
            // it is never silently destroyed.
        }
    }

    /// Returns `true` when `press` is the stroke already at the head of the
    /// pending buffer, compared in normalized form.
    fn repeats_head(&self, press: KeyPress) -> bool {
        self.pending
            .last()
            .is_some_and(|last| last.normalized() == press.normalized())
    }

    /// Folds `press` into the count when it is a count digit here, returning
    /// `true` when it was absorbed.
    ///
    /// Guards, in order, keep this from stealing ordinary keystrokes:
    ///
    /// - the press must be a bare decimal digit (no `Ctrl`/`Alt`/`Meta`);
    /// - a digit typed while a digit group is already open **always** continues it,
    ///   ahead of any binding. This is what makes `10dw` a count of ten in a keymap
    ///   that also binds a bare `0` to line-start, as Vim's does;
    /// - otherwise `0` cannot *start* a count, leaving it bindable;
    /// - otherwise a binding that can consume the digit as its next stroke wins,
    ///   and some reachable binding must have declared that it accepts a count. The
    ///   non-modal default keymap declares none, so digits there always type.
    fn absorb_count_digit(&mut self, keymap: &KeymapStack, press: KeyPress) -> bool {
        let Some(digit) = decimal_digit(press) else {
            return false;
        };

        if self.digits.is_none() {
            if digit == 0 {
                return false;
            }
            // Test the digit as a real stroke without allocating a second buffer.
            self.pending.push(press);
            let bound = keymap
                .exact_match(&self.pending, self.mode.as_ref())
                .is_some()
                || keymap.has_continuation(&self.pending, self.mode.as_ref());
            self.pending.pop();
            if bound {
                return false;
            }
            if !keymap.accepts_count_at(&self.pending, self.mode.as_ref()) {
                return false;
            }
        }

        self.digits = Some(
            self.digits
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(digit),
        );
        true
    }

    /// Folds the digit group being typed into the committed count.
    ///
    /// Groups **multiply**, so `2d2w` is four words and `22dw` is twenty-two, both
    /// matching Vim.
    fn commit_digits(&mut self) {
        if let Some(digits) = self.digits.take() {
            self.count = Some(self.count.unwrap_or(1).saturating_mul(digits));
        }
    }

    /// Clears the pending strokes and both halves of the count, leaving the mode.
    fn clear_sequence(&mut self) {
        self.pending.clear();
        self.count = None;
        self.digits = None;
    }
}

/// The decimal value of a bare digit keypress, or `None`.
///
/// `Shift` is not consulted (the host already resolved it into the character);
/// `Ctrl`, `Alt` and `Meta` are, because `Ctrl+2` is a chord, not a count.
const fn decimal_digit(press: KeyPress) -> Option<u32> {
    if press.modifiers.ctrl || press.modifiers.alt || press.modifiers.meta {
        return None;
    }
    match press.key {
        KeyCode::Char(c) => c.to_digit(10),
        _ => None,
    }
}
