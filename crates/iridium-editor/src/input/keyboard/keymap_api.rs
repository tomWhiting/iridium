//! The host-facing keymap, mode and pending-sequence surface.
//!
//! Everything a host needs in order to make bindings *data*: read the layer
//! stack, push or pop a user layer, ask which mode the resolver is in, and see
//! or abandon a half-typed key sequence. Split from the module root because
//! swapping bindings at runtime is a distinct concern from turning a keypress
//! into a command, and the two together took the root past the module cap.

use std::mem;

use super::{KeyPress, KeyboardHandler, Keymap, KeymapError, KeymapStack, ModeName};
use crate::commands::{CommandRegistry, KeyHintIndex};

impl KeyboardHandler {
    /// The binding layers this handler resolves against, lowest precedence first.
    pub const fn keymap(&self) -> &KeymapStack {
        &self.keymap
    }

    /// Which key sequence runs each command, for the current layer stack.
    ///
    /// The reverse of resolution, and what a command palette shows beside each
    /// entry. Kept in step with [`Self::keymap`] automatically — see
    /// [`Self::install_keymap`].
    pub const fn key_hints(&self) -> &KeyHintIndex {
        &self.key_hints
    }

    /// Replaces the layer stack and everything derived from it.
    ///
    /// **Every** mutation of the keymap goes through here, and the field is
    /// private to this module so none can avoid it. That is deliberate: the hint
    /// index is derived state, and derived state updated by four separate call
    /// sites is derived state that will eventually be stale at one of them. A
    /// stale index means a palette showing a key that no longer runs the command
    /// beside it — a wrong answer, not a missing one.
    ///
    /// Rebuilding eagerly rather than on demand keeps [`Self::key_hints`] a
    /// `&self` read with no interior mutability. It costs an allocation per
    /// keymap change, which happens at startup and on configuration reload; the
    /// palette reads the result on every filter keystroke.
    fn install_keymap(&mut self, keymap: KeymapStack) {
        self.key_hints = KeyHintIndex::build(&keymap);
        self.keymap = keymap;
        self.resolver.abort_pending();
    }

    /// Replaces the whole layer stack, discarding any pending key sequence.
    ///
    /// The pending strokes were matched against the outgoing bindings, so
    /// carrying them across could complete a sequence the user never typed.
    ///
    /// A binding naming a command this handler does not implement is not an
    /// error: that key is reported as
    /// [`KeyResult::HostCommand`](crate::input::KeyResult::HostCommand), naming the id,
    /// so a host command can own it. Use [`Self::push_validated_keymap`] (or
    /// [`KeymapStack::validate`] against a registry) at load time to turn a *typo*
    /// into a diagnostic instead.
    pub fn set_keymap(&mut self, keymap: KeymapStack) {
        self.install_keymap(keymap);
    }

    /// Pushes `keymap` as the new highest-precedence layer, discarding any
    /// pending key sequence.
    ///
    /// Prefer [`Self::push_validated_keymap`] for anything loaded from
    /// configuration: this method performs no checks, so a typo'd command id
    /// becomes a key that reports
    /// [`KeyResult::HostCommand`](crate::input::KeyResult::HostCommand) for a command nobody
    /// implements, and every matched binding clones an owned id on the keystroke
    /// path.
    pub fn push_keymap(&mut self, keymap: Keymap) {
        let mut next = mem::take(&mut self.keymap);
        next.push(keymap);
        self.install_keymap(next);
    }

    /// Canonicalizes and validates `keymap` against `registry`, then pushes it.
    ///
    /// The path a host loading a keymap from configuration should take, and the
    /// reason it exists as one call: the two steps it performs are documented as
    /// the host's responsibility, and a documented responsibility with no
    /// convenient API is one that does not happen.
    ///
    /// - [`Keymap::canonicalize`] replaces every deserialized, heap-owned
    ///   [`CommandId`](crate::CommandId) with the registry's `'static` instance,
    ///   which restores allocation-free resolution on the keystroke hot path.
    /// - [`KeymapStack::validate`] then checks the whole stack, so an unknown
    ///   command id, an unreachable binding, and a binding whose bare prefix
    ///   strands a chord in a *lower* layer are all startup diagnostics rather
    ///   than keys that quietly misbehave.
    ///
    /// The stack is left untouched when validation fails, so a rejected user
    /// keymap cannot half-apply.
    ///
    /// # Errors
    ///
    /// The first [`KeymapError`] canonicalization or validation reports.
    pub fn push_validated_keymap(
        &mut self,
        mut keymap: Keymap,
        registry: &CommandRegistry,
    ) -> Result<(), KeymapError> {
        keymap.canonicalize(registry)?;
        let mut candidate = self.keymap.clone();
        candidate.push(keymap);
        candidate.validate(registry)?;
        self.install_keymap(candidate);
        Ok(())
    }

    /// Removes and returns the highest-precedence layer, discarding any pending
    /// key sequence.
    pub fn pop_keymap(&mut self) -> Option<Keymap> {
        let mut next = mem::take(&mut self.keymap);
        let popped = next.pop();
        self.install_keymap(next);
        popped
    }

    /// The strokes typed so far in an incomplete key sequence, normalized.
    ///
    /// Empty unless the last key was consumed as a live prefix. A host renders
    /// this as the "waiting for another key" indicator.
    pub fn pending_sequence(&self) -> &[KeyPress] {
        self.resolver.pending()
    }

    /// The count typed so far into an incomplete sequence, if any.
    ///
    /// Rendered alongside [`Self::pending_sequence`] so `2d` shows as `2d` while
    /// the user is mid-grammar.
    #[must_use]
    pub const fn pending_count(&self) -> Option<u32> {
        self.resolver.pending_count()
    }

    /// The active editing mode, or `None` for a non-modal keymap.
    pub const fn mode(&self) -> Option<&ModeName> {
        self.resolver.mode()
    }

    /// Sets the active editing mode, discarding any pending key sequence.
    ///
    /// A mode is data owned by the keymap layer; the handler only compares it
    /// against each binding's scope.
    ///
    /// A keymap can also switch modes on its own, without the host: a binding
    /// built with [`KeyBinding::enter_mode`](crate::KeyBinding::enter_mode) or
    /// [`KeyBinding::then_enter_mode`](crate::KeyBinding::then_enter_mode) applies
    /// its transition when it matches. That is what makes a modal keymap work
    /// end-to-end as data — `i` entering insert mode and `Escape` returning to
    /// normal are bindings, not kernel behaviour — and this method is for the host
    /// paths a keymap cannot see, such as restoring a mode after a focus change.
    pub fn set_mode(&mut self, mode: Option<ModeName>) {
        self.resolver.set_mode(mode);
    }

    /// Cancels any pending key sequence, returning `true` when one was cancelled.
    ///
    /// Call this from every host path that invalidates the input context without
    /// a keypress — focus loss especially — so a half-typed chord cannot complete
    /// much later against unrelated state. The cursor-invalidation entry points
    /// below already do it, which covers mouse clicks, host-driven cursor jumps,
    /// paste, and history replay.
    pub fn abort_pending_sequence(&mut self) -> bool {
        self.resolver.abort_pending()
    }

    // ========== Transient state invalidation ==========
}
