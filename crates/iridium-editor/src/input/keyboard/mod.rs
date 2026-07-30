//! Keyboard input handling.
//!
//! # Bindings are data, not branches
//!
//! A keypress is never matched against a `(KeyCode, Modifiers)` pattern here.
//! [`KeyboardHandler::handle_key`] feeds it to a [`KeymapResolver`] over a
//! [`KeymapStack`], which yields a [`CommandId`](crate::CommandId); the command
//! is then run by [`dispatch`] through the table in [`actions`]. Three
//! consequences follow, and they are the reason for the indirection:
//!
//! - **rebindable** — a host pushes a user [`Keymap`] onto the stack
//!   ([`KeyboardHandler::push_keymap`]) and every key it names changes meaning,
//!   including being unbound entirely, without touching this crate;
//! - **discoverable** — the same command ids populate a command palette and a
//!   keybinding-help view, because [`crate::commands::builtin`] describes each
//!   one;
//! - **modal-capable** — a Vim-grammar keymap is another [`Keymap`] whose
//!   bindings name modes, stacked on top of the non-modal default. The kernel
//!   grows no mode enum; see [`KeymapResolver`].
//!
//! Multi-key sequences work from the first keypress: `Ctrl+K` alone is a live
//! prefix of `Ctrl+K Ctrl+D`, so it is consumed and reported as
//! [`KeyResult::Handled`] while the sequence stays pending.
//!
//! # What the commands do
//!
//! Implementations are grouped by concern: caret motions and the sticky-column
//! machinery in [`navigation`], text/clipboard/comment/history commands in
//! [`edits`], multi-cursor verbs in [`multi_cursor_verbs`]. The per-cursor
//! primitives they build on live in [`editing`] and [`motions`] so other input
//! paths (e.g. paste handling in the editor core) can reuse them.
//!
//! All editing and navigation commands are multi-cursor aware: every cursor
//! (primary and secondary) is edited or moved independently, and cursors that
//! converge on the same position are merged.
//!
//! Configuration-dependent behaviors — tab width and spaces-vs-tabs,
//! indent/outdent, auto-indent on Enter (including bracket-block and code-fence
//! expansion), and auto-closing pairs — live in [`behaviors`] and are driven by
//! the [`EditorConfig`] passed to [`KeyboardHandler::handle_key`].
//!
//! Comment toggling lives in [`comments`]: the comment syntax is resolved from
//! the document's language identifier ([`Document::language`]), falling back to
//! [`EditorConfig::line_comment_token`]; when neither is available the toggle
//! commands are acknowledged without editing.

mod actions;
mod behaviors;
mod comments;
mod dispatch;
pub mod editing;
mod edits;
mod line_ops;
pub mod motions;
mod multi_cursor;
mod multi_cursor_verbs;
mod navigation;
mod types;

#[cfg(test)]
mod behavior_tests;
#[cfg(test)]
mod comment_tests;
#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod line_ops_tests;
#[cfg(test)]
mod multi_cursor_tests;
#[cfg(test)]
mod tests;

pub use types::{
    ClipboardOperation, CommandRunError, KeyCode, KeyEvent, KeyResult, Modifiers, SearchAction,
};

use crate::commands::{
    CommandArgs, CommandRegistry, KeyPress, Keymap, KeymapError, KeymapResolver, KeymapStack,
    ModeName, default_keymap_stack,
};
use crate::document::{CursorState, Document, Selection};
use crate::editor::EditorConfig;
use crate::history::{Command, UndoTree};

/// Re-exported for the `use super::*` in the module's test files, which build
/// cursor states from raw coordinates.
#[cfg(test)]
pub(super) use crate::document::Position;

/// Keyboard event handler for the editor.
///
/// Owns three things: the bindings ([`Self::keymap`]), the multi-key sequence
/// state machine ([`Self::resolver`]), and the transient per-cursor state that
/// only the keyboard needs (sticky columns and the multi-cursor addition-order
/// stack). It never mutates a document: a command that changes text is returned
/// as a reversible [`Command`] for the caller to apply.
///
/// One instance per input focus, because a half-typed key sequence belongs to the
/// surface it was typed into.
#[derive(Debug)]
pub struct KeyboardHandler {
    /// The binding layers consulted for every keypress, highest precedence last.
    ///
    /// Seeded with [`default_keymap_stack`]. A host layers its own keymap on top
    /// with [`Self::push_keymap`], which overrides the default without editing
    /// it.
    keymap: KeymapStack,

    /// The key-sequence state machine: the strokes typed so far in an incomplete
    /// sequence, plus the active mode.
    ///
    /// Held here rather than passed in because a pending sequence is per-focus
    /// state with the same lifetime as the sticky columns below, and must be
    /// abandoned by the same host events that invalidate them.
    resolver: KeymapResolver,

    /// Per-cursor preferred ("sticky") columns for vertical movement.
    ///
    /// Aligned with [`CursorState::all_selections`] order (primary first,
    /// then secondary cursors in document order) as captured by the last
    /// vertical move or vertical add. The vector lives on the handler rather
    /// than on `Selection` so the persisted cursor state stays free of
    /// transient input concerns.
    ///
    /// The columns are valid *only* for the exact cursor state recorded in
    /// [`Self::sticky_state`]. A vertical operation reuses them when the
    /// incoming cursor state equals that snapshot and rebuilds them from the
    /// current head columns otherwise. Validating by full cursor-state
    /// identity (rather than by cursor count) is what makes any intervening
    /// path self-invalidate the sticky column without an explicit reset: a
    /// horizontal motion, an edit, or a select-all produces a different
    /// cursor state, so the stale columns are ignored — while undo/redo that
    /// restores the *exact* prior cursor state also restores its sticky
    /// columns (they were never destroyed, only left dormant).
    preferred_columns: Option<Vec<usize>>,

    /// The cursor state that [`Self::preferred_columns`] describes, or `None`
    /// when no sticky columns are held. A vertical move/add trusts
    /// `preferred_columns` only while this equals the incoming cursor state.
    sticky_state: Option<CursorState>,

    /// The document content revision ([`Document::revision`]) the sticky
    /// columns were captured at, or `None` when untracked.
    ///
    /// A host-driven bare content edit can shift text under cursors *without*
    /// moving them (leaving [`Self::sticky_state`] matching), so validating the
    /// sticky columns also against the content revision discards them whenever
    /// the document changed — a vertical move after such an edit re-seeds from
    /// the live head column instead of a stale preferred column.
    sticky_revision: Option<u64>,

    /// Set when a non-vertical cursor mutation has happened since the sticky
    /// columns were captured, marking them stale without destroying them.
    ///
    /// Validity by cursor-state identity alone is defeated by any action that
    /// returns the cursor to the same coordinates (a left-then-right round
    /// trip, an edit that lands the caret back where it started): the columns
    /// would be wrongly resurrected. A horizontal motion, an edit, a collapse to
    /// primary, or a line operation sets this flag (see
    /// [`Self::note_operation`]); a vertical move/add ignores dirty columns and
    /// re-seeds from the live head. The flag is cleared only when new columns are
    /// stored, on a full reset, or when undo/redo restores the exact prior state
    /// (see [`Self::revalidate_vertical_columns`]) — so genuine history replay
    /// still revives the columns while a live round trip does not.
    sticky_dirty: bool,

    /// Secondary cursors in the order they were added, most recent last.
    ///
    /// Populated by the multi-cursor add verbs (add-selection-to-next-match,
    /// add-cursor-above/below, select-all-occurrences) and consumed by
    /// remove-last-cursor and [`Self::skip_last_added_occurrence`], which pop the
    /// most recently added cursor. Each entry is the exact [`Selection`] of a
    /// secondary cursor.
    ///
    /// The stack is only meaningful while it describes the *current* cursor
    /// state; [`Self::add_order_anchor`] records the state it was last
    /// consistent with so any cursor change from another path (a motion, an
    /// edit, a collapse, a host-driven `set_cursor`/`set_selection`, undo/redo)
    /// self-invalidates the stack the next time a multi-cursor verb runs. See
    /// [`Self::sync_add_order`].
    cursor_add_order: Vec<Selection>,

    /// The cursor state that [`Self::cursor_add_order`] was last consistent
    /// with (the state each add verb produced), or `None` when the stack is
    /// empty and untracked.
    ///
    /// Before a multi-cursor verb trusts the stack it compares the incoming
    /// cursor state against this snapshot; any mismatch means something else
    /// moved, added, or removed a cursor since the last verb, so the stack is
    /// cleared. This is what makes edits and motions unable to corrupt the
    /// addition order: they never update the anchor, so the very next verb
    /// discards the now-stale stack instead of removing or skipping the wrong
    /// cursor.
    add_order_anchor: Option<CursorState>,

    /// The document content revision ([`Document::revision`]) the stack was
    /// last consistent with, or `None` when untracked.
    ///
    /// A host-driven bare `Insert`/`Delete`/`Replace` can move text under the
    /// cursors *without* changing any cursor position, leaving
    /// [`Self::add_order_anchor`] matching while the bytes each stack entry
    /// points at have changed. Recording the revision alongside the anchor
    /// means the next verb also compares content revisions and discards the
    /// stack whenever the document changed, so skip/remove-last-cursor never
    /// slice a stale range as the search term.
    add_order_revision: Option<u64>,
}

impl Default for KeyboardHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardHandler {
    /// Creates a new keyboard handler bound to the default non-modal keymap.
    ///
    /// The keymap is the layer stack from [`default_keymap_stack`]; a host adds
    /// its own layer with [`Self::push_keymap`] rather than replacing this one, so
    /// bindings the user did not override keep working.
    ///
    /// # Compatibility
    ///
    /// This was a `const fn` before bindings became data. It cannot be one now,
    /// because it allocates the default binding table — so a downstream
    /// `static H: KeyboardHandler = KeyboardHandler::new();` no longer compiles and
    /// must become a `LazyLock` or a run-time construction. Nothing in this
    /// workspace constructed a handler in a `const` context; the narrowing is
    /// recorded here rather than left to be discovered.
    #[must_use]
    pub fn new() -> Self {
        Self {
            keymap: default_keymap_stack(),
            resolver: KeymapResolver::new(),
            preferred_columns: None,
            sticky_state: None,
            sticky_revision: None,
            sticky_dirty: false,
            cursor_add_order: Vec::new(),
            add_order_anchor: None,
            add_order_revision: None,
        }
    }

    /// Handles a keyboard event.
    ///
    /// Returns a `KeyResult` indicating what action should be taken.
    /// The caller is responsible for applying any resulting commands
    /// and handling clipboard operations.
    ///
    /// The event is resolved through [`Self::keymap`]; see [`dispatch`] for the
    /// resolution order and for what happens to a keypress no binding claims.
    ///
    /// `config` drives the editing behaviors: `tab_width`/`insert_spaces`
    /// for Tab, indent, and outdent; `auto_indent` for Enter (indent
    /// inheritance, bracket-block and code-fence expansion); `auto_pairs`
    /// for bracket/quote pairing; and `line_comment_token` as the comment
    /// syntax fallback for the comment-toggle commands when the document's
    /// language provides none.
    pub fn handle_key(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
        history: &UndoTree,
        config: &EditorConfig,
    ) -> KeyResult {
        let (result, action) = self.dispatch_key(event, document, cursor, history, config);
        self.note_operation(action, &result);
        result
    }

    /// Runs one command by id, with no keystroke involved.
    ///
    /// This is the other half of "every editor action is a named, addressable
    /// value": a command palette, a macro, a menu item and an AI host all need to
    /// invoke a command they found in the
    /// [`CommandRegistry`](crate::CommandRegistry) without fabricating a
    /// [`KeyEvent`]. The returned [`KeyResult`] is exactly what the same command
    /// produces from a keypress, and the post-command bookkeeping (sticky columns,
    /// the multi-cursor addition-order stack) runs identically — so invoking
    /// `cursor.lineDown` from a palette keeps the sticky column just as the
    /// `Down` key does.
    ///
    /// `args` carries a count and captured characters for commands that read them;
    /// pass [`CommandArgs::NONE`] otherwise.
    ///
    /// # Errors
    ///
    /// [`CommandRunError::Unimplemented`] when this kernel implements no command
    /// with that id. A host command registered in the registry lands here, and the
    /// caller — which owns the implementation — is the right place to notice.
    pub fn run_command(
        &mut self,
        id: &str,
        args: CommandArgs,
        document: &Document,
        cursor: &CursorState,
        history: &UndoTree,
        config: &EditorConfig,
    ) -> Result<KeyResult, CommandRunError> {
        let Some(action) = actions::action_for(id) else {
            return Err(CommandRunError::Unimplemented { id: id.to_owned() });
        };
        let ctx = actions::CommandContext {
            event: None,
            args,
            document,
            cursor,
            history,
            config,
        };
        let result = self.run_action(action, &ctx);
        self.note_operation(Some(action), &result);
        Ok(result)
    }

    /// Returns `true` when this kernel implements a command with `id`.
    ///
    /// A host uses this to decide whether a registry entry is the kernel's to run
    /// or its own, without invoking anything.
    #[must_use]
    pub fn implements_command(id: &str) -> bool {
        actions::action_for(id).is_some()
    }

    /// Handles pasting text from clipboard.
    ///
    /// This is called by the editor when clipboard content is available.
    /// The text is inserted at every cursor.
    pub fn handle_paste(&self, text: &str, document: &Document, cursor: &CursorState) -> KeyResult {
        Self::insert_text(text, document, cursor)
    }

    // ========== Keymap and sequence state ==========

    /// The binding layers this handler resolves against, lowest precedence first.
    pub const fn keymap(&self) -> &KeymapStack {
        &self.keymap
    }

    /// Replaces the whole layer stack, discarding any pending key sequence.
    ///
    /// The pending strokes were matched against the outgoing bindings, so
    /// carrying them across could complete a sequence the user never typed.
    ///
    /// A binding naming a command this handler does not implement is not an
    /// error: that key is reported as [`KeyResult::HostCommand`], naming the id,
    /// so a host command can own it. Use [`Self::push_validated_keymap`] (or
    /// [`KeymapStack::validate`] against a registry) at load time to turn a *typo*
    /// into a diagnostic instead.
    pub fn set_keymap(&mut self, keymap: KeymapStack) {
        self.keymap = keymap;
        self.resolver.abort_pending();
    }

    /// Pushes `keymap` as the new highest-precedence layer, discarding any
    /// pending key sequence.
    ///
    /// Prefer [`Self::push_validated_keymap`] for anything loaded from
    /// configuration: this method performs no checks, so a typo'd command id
    /// becomes a key that reports [`KeyResult::HostCommand`] for a command nobody
    /// implements, and every matched binding clones an owned id on the keystroke
    /// path.
    pub fn push_keymap(&mut self, keymap: Keymap) {
        self.keymap.push(keymap);
        self.resolver.abort_pending();
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
        self.keymap = candidate;
        self.resolver.abort_pending();
        Ok(())
    }

    /// Removes and returns the highest-precedence layer, discarding any pending
    /// key sequence.
    pub fn pop_keymap(&mut self) -> Option<Keymap> {
        self.resolver.abort_pending();
        self.keymap.pop()
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

    /// Clears the sticky-column state used for vertical movement, and any
    /// pending key sequence.
    ///
    /// Sticky columns are validated against the exact cursor state they were
    /// captured for (see [`Self::preferred_columns`]), so most cursor changes
    /// self-invalidate them. This explicit reset is for host-driven cursor
    /// jumps that bypass [`Self::handle_key`] — `set_cursor`/`set_selection`,
    /// mouse-applied commands, IME commits, paste — where the caller wants a
    /// fresh sticky column even if the new state happened to coincide with a
    /// dormant one. Undo/redo deliberately does **not** call this: restoring
    /// the exact prior cursor state must also restore its sticky columns.
    ///
    /// A half-typed key sequence is abandoned for the same reason the columns
    /// are: the context it was typed in is gone. Calls from
    /// [`Self::note_operation`] never abandon anything, because a keypress that
    /// left a sequence pending produces no cursor mutation and so never reaches
    /// here.
    pub fn reset_vertical_state(&mut self) {
        self.preferred_columns = None;
        self.sticky_state = None;
        self.sticky_revision = None;
        self.sticky_dirty = false;
        self.resolver.abort_pending();
    }

    /// Revives the sticky columns after an undo/redo replay that restored the
    /// exact cursor state they describe.
    ///
    /// History navigation that returns the cursor to the state the sticky
    /// columns were captured for should also revive them (the enclosing content
    /// edit had marked them dirty). This clears the dirty flag and, when the
    /// stored [`Self::sticky_state`] still equals the restored `cursor`,
    /// re-stamps [`Self::sticky_revision`] to the replayed document's
    /// `revision` — because undo/redo bump the revision even when they restore
    /// identical content, the columns' original revision would otherwise no
    /// longer match. When the stored state does not match the restored cursor,
    /// only the flag is cleared and the (now mismatched) columns are ignored by
    /// [`Self::take_sticky_columns_for`], so a wrong set can never be revived.
    pub fn revalidate_vertical_columns(&mut self, cursor: &CursorState, revision: u64) {
        self.sticky_dirty = false;
        if self.sticky_state.as_ref() == Some(cursor) {
            self.sticky_revision = Some(revision);
        }
    }

    /// Discards the multi-cursor addition-order stack, and any pending key
    /// sequence.
    ///
    /// Called by the editor from every path that mutates the cursor without
    /// going through an add verb — host `set_cursor`/`set_selection`, mouse
    /// clicks, IME commits, paste, search navigation, and undo/redo replay.
    /// Unlike the value-equality snapshot in [`Self::sync_add_order`] (which a
    /// sequence of mutations returning to the same state can defeat), this
    /// eager clear cannot be fooled by a round trip: once any such path runs,
    /// remove-last-cursor and skip find an empty stack and no-op rather than
    /// removing an arbitrarily-ordered cursor.
    pub fn invalidate_cursor_order(&mut self) {
        self.cursor_add_order.clear();
        self.add_order_anchor = None;
        self.add_order_revision = None;
        self.resolver.abort_pending();
    }

    // ========== Shared utility ==========

    /// Creates a selection command if the cursor changed.
    fn create_selection_command(old: &CursorState, new: &CursorState) -> KeyResult {
        if old == new {
            KeyResult::Handled
        } else {
            KeyResult::Command(Command::SetSelection {
                old_state: old.clone(),
                new_state: new.clone(),
            })
        }
    }
}
