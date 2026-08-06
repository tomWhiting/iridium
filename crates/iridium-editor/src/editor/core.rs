//! Core editor state and main editor instance.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::ast::SyntaxState;
use super::config::EditorConfig;
use super::fold_state::{FoldInfo, FoldState};
use crate::commands::{
    CommandArgs, CommandId, CommandMeta, CommandRegistry, KeyHintIndex, KeyPress, Keymap,
    KeymapError, KeymapStack, ModeName, RegistryError, builtin,
};
use crate::document::{CursorState, Document, Position, Selection, compute_edit_span};
use crate::history::{Command, UndoTree};
use crate::input::keyboard::editing;
use crate::input::{
    AstRequest, ClipboardOperation, CommandRunError, ImeEvent, ImeHandler, ImeResult, ImeState,
    KeyEvent, KeyResult, KeyboardHandler, MouseEvent, MouseHandler, MouseResult, SearchAction,
};
use crate::render::Viewport;
use crate::search::{SearchOptions, SearchState, replace_all, replace_current};
use crate::theme::Theme;

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::{FoldKind, Language};
#[cfg(feature = "syntax")]
use iridium_syntax::{FoldKind, Language};

/// Events emitted by the editor to the host application.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EditorEvent {
    /// Document content changed.
    ContentChanged {
        /// New full content
        content: String,
    },

    /// Cursor or selection changed.
    SelectionChanged {
        /// All current selections
        selections: Vec<Selection>,
    },

    /// Scroll position changed.
    ScrollChanged {
        /// First visible line
        first_line: usize,
    },

    /// Search results updated.
    SearchUpdated {
        /// Number of matches
        match_count: usize,
        /// Current match index (if any)
        current_index: Option<usize>,
    },

    /// Theme changed (T147).
    ///
    /// Emitted when the theme is updated at runtime.
    ThemeChanged {
        /// Name of the new theme
        theme_name: String,
        /// Whether the new theme is dark
        is_dark: bool,
    },

    /// Fold state changed (T130).
    ///
    /// Emitted when a fold is toggled, or regions are updated.
    FoldChanged {
        /// Number of currently folded regions
        folded_count: usize,
        /// Total number of foldable regions
        total_regions: usize,
    },

    /// Which branch a redo would take has changed, without the document or the
    /// cursor moving.
    ///
    /// The only signal an undo-tree panel has that its highlight is stale: a
    /// branch cycle applies nothing, so neither `ContentChanged` nor
    /// `SelectionChanged` fires.
    HistoryBranchChanged,

    /// An error occurred.
    Error {
        /// Error message
        message: String,
        /// Error code for programmatic handling
        code: String,
    },
}

/// Result of handling a keyboard event in the editor.
///
/// Used to communicate what type of action was requested by the keyboard input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorKeyResult {
    /// No special action requested.
    None,
    /// A clipboard operation was requested.
    Clipboard(ClipboardOperation),
    /// A search action was requested.
    Search(SearchAction),
    /// A binding resolved to a command the kernel does not implement.
    ///
    /// The keypress was consumed and the command named, so a host command is
    /// reported to the host that owns it instead of vanishing. This is how a
    /// command contributed from outside the kernel is *reached*: register its
    /// [`CommandMeta`] with [`Editor::register_command`], bind it in a keymap
    /// pushed with [`Editor::push_keymap`], and run it when this arrives. Any
    /// document change the host then makes must go through
    /// [`Editor::apply_command`], which keeps the command-sourced invariant and
    /// the undo tree intact.
    HostCommand {
        /// The resolved command id.
        command: CommandId,
        /// The count and captured characters the key sequence carried.
        args: CommandArgs,
    },
}

/// Complete state of an editor instance.
///
/// This struct holds all the state needed to render and interact with
/// the editor. It is the source of truth for the editor's current state.
#[derive(Debug)]
pub struct EditorState {
    /// The document being edited
    pub document: Document,

    /// Cursor and selection state
    pub cursor: CursorState,

    /// Undo/redo history
    pub history: UndoTree,

    /// Current theme
    pub theme: Theme,

    /// Editor configuration
    pub config: EditorConfig,

    /// Viewport for visible region tracking
    pub viewport: Viewport,

    /// Current scroll position (first visible line)
    pub scroll_line: usize,

    /// Horizontal scroll offset in pixels
    pub scroll_x: f32,

    /// Whether the editor has focus
    pub has_focus: bool,

    /// Whether the editor is read-only
    pub read_only: bool,

    /// Code folding state (T130)
    pub fold_state: FoldState,

    /// The document's parse tree, and how current it is.
    ///
    /// The one owner. Folds read it, and structural navigation will; nothing
    /// parses this document a second time.
    pub syntax: SyntaxState,

    /// Search state (T126)
    pub search: SearchState,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            document: Document::default(),
            cursor: CursorState::at(Position::zero()),
            history: UndoTree::new(),
            theme: Theme::default(),
            config: EditorConfig::default(),
            viewport: Viewport::default(),
            scroll_line: 0,
            scroll_x: 0.0,
            has_focus: false,
            read_only: false,
            fold_state: FoldState::new(),
            syntax: SyntaxState::new(),
            search: SearchState::new(),
        }
    }
}

impl EditorState {
    /// Creates a new editor state with the given content.
    #[must_use]
    pub fn new(content: &str) -> Self {
        Self {
            document: Document::new(content),
            ..Self::default()
        }
    }

    /// Creates an empty editor state.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns the current content as a string.
    #[must_use]
    pub fn content(&self) -> String {
        self.document.text()
    }

    /// Sets the content, resetting cursor and history.
    ///
    /// The current language (if any) carries over to the new document, so
    /// language-aware editing keeps working after a full content swap.
    pub fn set_content(&mut self, content: &str) {
        let language_id = self
            .language()
            .map(|language| language.id().to_owned())
            .filter(|id| !id.is_empty());
        self.document = Document::new(content);
        self.document.set_language(language_id);
        self.cursor = CursorState::at(Position::zero());
        // Replacing the content replaces the history — the old tree describes
        // a document that no longer exists — but the *configured* grouping
        // timeout must survive, or opening a file would silently revert the
        // host's `undo_group_timeout_ms` to the built-in default.
        self.history = UndoTree::with_timeout(self.config.undo_group_timeout_ms);
        self.scroll_line = 0;
        self.scroll_x = 0.0;
        // The old tree describes a document that no longer exists, and the
        // replacement's revision counter starts again — so say so outright
        // rather than leaving `sync` to infer it from a number that repeats.
        self.syntax.invalidate();
        self.refresh_syntax();
    }

    /// Sets the language for syntax-aware folding and language-aware editing.
    ///
    /// The language id is also recorded on the document itself so editing
    /// paths that only see the document (e.g. comment toggling in the
    /// keyboard handler) can resolve the language's comment syntax.
    pub fn set_language(&mut self, language: Language) {
        self.fold_state.set_language(language);
        self.syntax.set_language(language);
        let id = language.id();
        if !id.is_empty() {
            self.document.set_language(Some(id.to_owned()));
        }
        self.refresh_syntax();
    }

    /// Brings the parse tree up to date and refreshes the fold regions from it.
    ///
    /// Returns true if the fold regions changed.
    ///
    /// This is the only place folds are recomputed, and it is deliberately the
    /// only place that parses: [`SyntaxState::sync`] does nothing when the
    /// document has not moved, so calling this more often than necessary costs
    /// a revision comparison rather than a parse.
    pub fn refresh_syntax(&mut self) -> bool {
        if self.syntax.sync(&self.document).is_none() {
            return false;
        }
        // What `sync` just did, taken before the tree is borrowed again. This is
        // what keeps a keystroke off the O(document) path: with it, fold
        // detection re-reads only the part of the tree that moved.
        let delta = self.syntax.take_delta();
        let Some(tree) = self.syntax.tree() else {
            return false;
        };
        // The tree and the text must be one document's worth. `sync` has just
        // parsed this exact revision, so they are by construction. The text is
        // passed as a closure and, with the `syntax` feature on, is never
        // built: nothing in tree-sitter fold detection reads it.
        let document = &self.document;
        self.fold_state
            .update_regions(tree, &delta, || Cow::Owned(document.text()))
    }

    /// Returns the current language, if any.
    #[must_use]
    pub const fn language(&self) -> Option<Language> {
        self.fold_state.language()
    }

    /// Re-synchronizes the active search with the current document content.
    ///
    /// Recorded match ranges point into the document as it was when the
    /// search last ran; any content mutation can silently invalidate them
    /// (and literal-mode replace trusts them verbatim), so this must be
    /// called after every content-modifying command, including undo/redo.
    ///
    /// When a search with a non-empty query is active, the search is re-run
    /// against the mutated document with the current query and options, and
    /// the current match is preserved by nearest position (the match at or
    /// after the previous current match's start). With an empty query this
    /// is a no-op. Re-running the full search on every edit is the honest
    /// baseline; incremental match maintenance is a planned optimization
    /// (PLAN.md Phase 2 performance work).
    ///
    /// Returns `true` when an active search was re-run (matches and current
    /// index may have changed), `false` when there was nothing to refresh.
    pub fn refresh_search(&mut self) -> bool {
        if self.search.query.is_empty() {
            return false;
        }

        let previous_position = self.search.current_range().map(|range| range.start);
        let query = self.search.query.clone();
        let options = self.search.options.clone();

        // The query and options were validated when the search was created
        // and are unchanged here, so this can only fail if the public search
        // fields were mutated into an invalid state. `find_all` clears the
        // recorded matches before validating, which is exactly the safe
        // outcome for that case: no stale range survives.
        let _ = self.search.find_all(&query, &options, &self.document);

        if let Some(position) = previous_position {
            self.search.goto_nearest_match(position);
        }

        true
    }
}

/// The main Iridium editor instance.
///
/// This is the primary interface for embedding the editor. It wraps
/// [`EditorState`] and provides methods for interaction.
///
/// # Example
///
/// ```ignore
/// use iridium_editor::{Editor, EditorConfig};
///
/// let editor = Editor::new(EditorConfig::default())?;
/// editor.set_content("Hello, world!");
/// ```
#[allow(clippy::type_complexity)]
pub struct Editor {
    /// The editor state.
    ///
    /// Visible to sibling modules of `editor` (see `history_nav`) so they can
    /// mutate it without going through [`Editor::state_mut`], which eagerly
    /// discards the keyboard handler's transient state. History replay must
    /// *preserve* that state — see [`Editor::finish_history_replay`].
    pub(super) state: EditorState,

    /// Event listeners
    listeners: Vec<Box<dyn Fn(&EditorEvent) + Send + Sync>>,

    /// Keyboard input handler
    keyboard_handler: KeyboardHandler,

    /// Every command this editor exposes, kernel and host alike.
    ///
    /// The authority on *what exists*: a command palette enumerates this, a
    /// keymap is validated against it, and an AI host discovers the editor's
    /// vocabulary from it. Seeded with
    /// [`register_builtin_commands`](crate::commands::builtin::register_builtin_commands);
    /// a host adds its own with [`Editor::register_command`].
    commands: CommandRegistry,

    /// Mouse input handler
    mouse_handler: MouseHandler,

    /// IME composition handler
    ime_handler: ImeHandler,
}

impl Editor {
    /// Creates a new editor with the given configuration.
    ///
    /// [`EditorConfig::undo_group_timeout_ms`] is applied to the undo history
    /// here; it is the only configuration value the history reads, and it
    /// cannot be honoured after the fact without discarding the tree, so
    /// changing it later goes through
    /// [`Editor::set_undo_group_timeout_ms`].
    #[must_use]
    pub fn new(config: EditorConfig) -> Self {
        let history = UndoTree::with_timeout(config.undo_group_timeout_ms);

        Self {
            state: EditorState {
                history,
                config,
                ..EditorState::default()
            },
            listeners: Vec::new(),
            keyboard_handler: KeyboardHandler::new(),
            commands: Self::seeded_command_registry(),
            mouse_handler: MouseHandler::new(),
            ime_handler: ImeHandler::new(),
        }
    }

    /// Builds the command registry every editor starts with.
    ///
    /// [`default_registry`](crate::commands::builtin::default_registry) holds the
    /// built-ins *and* the host commands the kernel names, such as
    /// [`PALETTE_OPEN`](crate::commands::builtin::PALETTE_OPEN). Both belong here:
    /// the default keymap binds the palette, so a registry without it would make
    /// the editor's own keymap fail validation.
    ///
    /// It is fallible only for a duplicated id *inside the two static tables*,
    /// which the registry tests rule out by asserting the registry holds exactly
    /// [`BUILTIN_COMMAND_COUNT`](crate::commands::builtin::BUILTIN_COMMAND_COUNT)
    /// plus [`HOST_COMMAND_COUNT`](crate::commands::builtin::HOST_COMMAND_COUNT)
    /// commands. Degrading to an empty registry rather than panicking keeps
    /// [`Editor::new`] infallible; the fallback is unreachable except through a
    /// kernel edit the tests reject.
    fn seeded_command_registry() -> CommandRegistry {
        builtin::default_registry().unwrap_or_default()
    }

    /// Creates an editor with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(EditorConfig::default())
    }

    /// Returns a reference to the editor state.
    #[must_use]
    pub const fn state(&self) -> &EditorState {
        &self.state
    }

    /// Sets how long consecutive edits keep merging into one undo step.
    ///
    /// Kept in sync with [`EditorConfig::undo_group_timeout_ms`] so the
    /// configured value and the history's behaviour cannot disagree. Zero
    /// disables grouping, making every edit individually undoable. Existing
    /// history is untouched; the new timeout applies to subsequent edits.
    pub const fn set_undo_group_timeout_ms(&mut self, timeout_ms: u64) {
        self.state.config.undo_group_timeout_ms = timeout_ms;
        self.state.history.set_group_timeout_ms(timeout_ms);
    }

    /// Returns a mutable reference to the editor state.
    ///
    /// This exposes the raw [`EditorState`] for out-of-band mutation, so the
    /// keyboard handler's transient state — the sticky vertical column and the
    /// multi-cursor addition-order stack — is invalidated up front. Both are
    /// validated by exact cursor-state (and revision) identity, which value
    /// equality alone cannot protect: a caller could remove and re-add a cursor
    /// through `state.cursor`, landing on a byte-identical `CursorState`, and a
    /// later Ctrl+U would then pop against a stale stack. Eagerly clearing here
    /// means any mutation via this accessor is treated like every other
    /// host-driven cursor jump (`set_cursor`, mouse, IME, paste).
    pub fn state_mut(&mut self) -> &mut EditorState {
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        &mut self.state
    }

    /// Sets the content, replacing everything.
    ///
    /// The cursor is reset (so the keyboard handler's sticky vertical column
    /// is dropped) and any active search is re-run against the new content.
    pub fn set_content(&mut self, content: &str) {
        self.state.set_content(content);
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        if self.state.refresh_search() {
            self.emit_search_updated();
        }
        self.emit_content_changed();
    }

    /// Returns the current content.
    #[must_use]
    pub fn content(&self) -> String {
        self.state.content()
    }

    /// Returns the current cursor position.
    #[must_use]
    pub const fn cursor(&self) -> Position {
        self.state.cursor.primary.head
    }

    /// Sets the cursor position.
    ///
    /// The position is clamped to the document. This bypasses the keyboard
    /// handler, so its sticky vertical column is reset.
    pub fn set_cursor(&mut self, position: Position) {
        let clamped = self.state.document.clamp_position(position);
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        self.state.cursor = CursorState::at(clamped);
        self.emit_selection_changed();
    }

    /// Sets the selection, replacing all cursors with a single selection.
    ///
    /// Both endpoints are clamped to the document. This bypasses the
    /// keyboard handler, so its sticky vertical column is reset.
    pub fn set_selection(&mut self, anchor: Position, head: Position) {
        let anchor = self.state.document.clamp_position(anchor);
        let head = self.state.document.clamp_position(head);
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        self.state.cursor = CursorState::new(Selection::new(anchor, head));
        self.emit_selection_changed();
    }

    /// Handles a keyboard event.
    ///
    /// Returns an `EditorKeyResult` indicating if a clipboard or search action
    /// was requested. The caller is responsible for handling these operations.
    pub fn handle_key(&mut self, event: &KeyEvent) -> EditorKeyResult {
        // The page-motion hop is the viewport height, which is this editor's
        // state and not the handler's; synced at every dispatch so a resize —
        // or a host writing `state_mut().viewport` directly — can never leave
        // a page key hopping yesterday's layout.
        self.keyboard_handler
            .set_page_rows(self.state.viewport.visible_lines);
        let result = self.keyboard_handler.handle_key(
            event,
            &self.state.document,
            &self.state.cursor,
            &self.state.history,
            &self.state.config,
        );
        self.consume_key_result(result)
    }

    // ========== Commands, keymaps and modes ==========

    /// Every command this editor exposes, kernel and host alike.
    ///
    /// This is what a command palette enumerates
    /// ([`CommandRegistry::palette_order`]) and what an AI or scripting host reads
    /// to discover the editor's vocabulary. Pair it with [`Self::run_command`],
    /// which invokes any entry by id.
    #[must_use]
    pub const fn commands(&self) -> &CommandRegistry {
        &self.commands
    }

    /// Registers a command contributed from outside the kernel.
    ///
    /// The command immediately appears in [`Self::commands`], so a palette lists
    /// it and [`Self::push_keymap`] will accept a binding naming it. Running it is
    /// the host's: [`Self::run_command`] reports
    /// [`CommandRunError::Unimplemented`] and a matching keypress reports
    /// [`EditorKeyResult::HostCommand`].
    ///
    /// # Errors
    ///
    /// [`RegistryError::DuplicateId`] when the id is taken — including by a kernel
    /// command, which a host must not shadow — and
    /// [`RegistryError::EmptyId`] for an empty id.
    pub fn register_command(&mut self, meta: CommandMeta) -> Result<(), RegistryError> {
        self.commands.register(meta)
    }

    /// Runs a command by id, with no keystroke involved.
    ///
    /// The palette's, the macro's and the AI host's entry point: every command in
    /// [`Self::commands`] that the kernel implements can be invoked here, and the
    /// resulting document change is applied through the same command-sourced path
    /// as a keypress, so it is undoable and read-only mode is honoured.
    ///
    /// `args` carries a count and captured characters for commands that read them;
    /// pass [`CommandArgs::NONE`] otherwise.
    ///
    /// Returns the same [`EditorKeyResult`] a keypress would, so a clipboard or
    /// search request still reaches the host.
    ///
    /// # Errors
    ///
    /// [`CommandRunError::Unimplemented`] when the kernel implements no command
    /// with that id — which is exactly the case for a host command, and the signal
    /// for the caller to run its own implementation.
    pub fn run_command(
        &mut self,
        id: &str,
        args: CommandArgs,
    ) -> Result<EditorKeyResult, CommandRunError> {
        // See `handle_key`: a palette-invoked page motion must hop exactly the
        // page a keypress would.
        self.keyboard_handler
            .set_page_rows(self.state.viewport.visible_lines);
        let result = self.keyboard_handler.run_command(
            id,
            args,
            &self.state.document,
            &self.state.cursor,
            &self.state.history,
            &self.state.config,
        )?;
        Ok(self.consume_key_result(result))
    }

    /// Returns `true` when the kernel itself implements `id`.
    ///
    /// Lets a host split [`Self::commands`] into "the editor runs these" and "I run
    /// these" without invoking anything.
    #[must_use]
    pub fn implements_command(id: &str) -> bool {
        KeyboardHandler::implements_command(id)
    }

    /// The binding layers this editor resolves keys against, lowest precedence
    /// first.
    #[must_use]
    pub const fn keymap(&self) -> &KeymapStack {
        self.keyboard_handler.keymap()
    }

    /// Which key sequence runs each command, for this editor's current bindings.
    ///
    /// The reverse of [`Self::keymap`], and the other half of what a command
    /// palette needs: [`Self::commands`] says what exists, this says what to
    /// press. A command with no entry has no key — which is the honest answer for
    /// a palette-only command, and the reason an absent hint is not an error.
    ///
    /// Recomputed whenever a keymap layer is pushed or popped, so it can never
    /// name a key that a user layer has since taken away.
    #[must_use]
    pub const fn key_hints(&self) -> &KeyHintIndex {
        self.keyboard_handler.key_hints()
    }

    /// Pushes a user keymap layer, validated against [`Self::commands`].
    ///
    /// This is decision D1 made reachable: bindings are swappable data, and a host
    /// swaps them here rather than by abandoning [`Editor`] and driving its own
    /// [`KeyboardHandler`] — which would duplicate the sticky-column and
    /// addition-order state whose staleness has bitten this codebase twice.
    ///
    /// The layer is canonicalized (so resolution stays allocation free) and the
    /// whole stack validated, so an unknown command id, an unreachable binding, and
    /// a bare prefix that would strand a chord in the default keymap are all
    /// reported here instead of becoming keys that quietly misbehave. The stack is
    /// unchanged when validation fails.
    ///
    /// # Errors
    ///
    /// The first [`KeymapError`] canonicalization or validation reports.
    pub fn push_keymap(&mut self, keymap: Keymap) -> Result<(), KeymapError> {
        self.keyboard_handler
            .push_validated_keymap(keymap, &self.commands)
    }

    /// Removes and returns the highest-precedence keymap layer.
    ///
    /// Returns `None` when no layer is left. Popping the base default keymap is
    /// permitted — a face that ships its own complete keymap wants exactly that —
    /// so the caller owns the consequence.
    pub fn pop_keymap(&mut self) -> Option<Keymap> {
        self.keyboard_handler.pop_keymap()
    }

    /// The strokes typed so far in an incomplete key sequence.
    ///
    /// A host renders this, together with [`Self::pending_key_count`], as the
    /// "waiting for another key" indicator. Surfacing it is not cosmetic: a chord
    /// leader consumes the keypress, so without an indicator the editor looks
    /// unresponsive.
    #[must_use]
    pub fn pending_key_sequence(&self) -> &[KeyPress] {
        self.keyboard_handler.pending_sequence()
    }

    /// The count typed so far into an incomplete key sequence, if any.
    #[must_use]
    pub const fn pending_key_count(&self) -> Option<u32> {
        self.keyboard_handler.pending_count()
    }

    /// Cancels any half-typed key sequence, returning `true` when one was
    /// cancelled.
    ///
    /// Call this on **focus loss**. Every cursor-invalidating host path
    /// (`set_cursor`, mouse, IME, paste, search navigation) already does it, which
    /// covers a click elsewhere; a blur with no click does not, and a chord left
    /// pending across it would eat the first keystroke after the user returns.
    pub fn abort_pending_key_sequence(&mut self) -> bool {
        self.keyboard_handler.abort_pending_sequence()
    }

    /// The active editing mode, or `None` for a non-modal keymap.
    #[must_use]
    pub const fn mode(&self) -> Option<&ModeName> {
        self.keyboard_handler.mode()
    }

    /// Sets the active editing mode, discarding any half-typed key sequence.
    ///
    /// A modal keymap normally switches modes itself, as data
    /// ([`KeyBinding::then_enter_mode`](crate::KeyBinding::then_enter_mode)); this
    /// is for the host paths a keymap cannot see, such as forcing insert mode when
    /// a widget takes focus.
    pub fn set_mode(&mut self, mode: Option<ModeName>) {
        self.keyboard_handler.set_mode(mode);
    }

    /// Applies a [`KeyResult`] to the editor state, as `handle_key` does.
    ///
    /// Shared by [`Self::handle_key`] and [`Self::run_command`] so a command
    /// invoked by id and the same command invoked by its key sequence cannot
    /// diverge.
    fn consume_key_result(&mut self, result: KeyResult) -> EditorKeyResult {
        match result {
            KeyResult::Command(cmd) => {
                if !self.state.read_only || matches!(cmd, Command::SetSelection { .. }) {
                    self.apply_command_internal(cmd);
                }
                EditorKeyResult::None
            },
            KeyResult::Clipboard(clip) => {
                if self.state.read_only && !matches!(clip, ClipboardOperation::Copy(_)) {
                    return EditorKeyResult::None;
                }
                EditorKeyResult::Clipboard(clip)
            },
            KeyResult::Search(action) => self.handle_search_action(action),
            KeyResult::History(request) => {
                // The one place a history command is performed, whether it
                // arrived as a keystroke or by id. Read-only is enforced by
                // `undo`/`redo` themselves, which is where it belongs: a jump
                // is an edit like any other.
                self.perform_history_request(request);
                EditorKeyResult::None
            },
            KeyResult::Ast(request) => {
                // The one place a structural selection is performed, whether it
                // arrived as a keystroke or by id. It ends as a
                // `Command::SetSelection`, so it is undoable and read-only-safe
                // for the same reasons every other selection change is.
                self.perform_ast_request(request);
                EditorKeyResult::None
            },
            KeyResult::HostCommand { command, args } => {
                EditorKeyResult::HostCommand { command, args }
            },
            KeyResult::Handled | KeyResult::Ignored => EditorKeyResult::None,
        }
    }

    /// Applies a structural selection change, if the tree has one to give.
    ///
    /// Returns whether the selection moved. Public so a face driving the editor
    /// without the kernel's own key handling — the web binding does — reaches the
    /// same implementation rather than growing a second one.
    ///
    /// In a build without tree-sitter this always returns `false`, which is why
    /// the web face can call it today and simply get nothing.
    ///
    /// Silent when the document has no language, when the tree cannot parse, or
    /// when the selection is already where the request would put it. All three
    /// are "nothing to do", and a key that quietly does nothing is better than
    /// one reporting a failure the person cannot act on.
    pub fn perform_ast_request(&mut self, request: AstRequest) -> bool {
        let next =
            self.state
                .syntax
                .apply_ast_request(&self.state.document, &self.state.cursor, request);

        let Some(new_state) = next else {
            return false;
        };

        let old_state = self.state.cursor.clone();
        self.apply_command_internal(Command::SetSelection {
            old_state,
            new_state,
        });
        true
    }

    /// Drops the most recently added occurrence cursor and selects the next
    /// occurrence instead, wrapping around the document.
    ///
    /// This is the "skip occurrence" verb, bound to the chord `Ctrl+K Ctrl+D` in
    /// the default keymap and also exposed here so a host that drives the editor
    /// without a keyboard can reach it; see
    /// [`KeyboardHandler::skip_last_added_occurrence`] for the exact behavior.
    ///
    /// Produces only a selection change, so it is permitted in read-only mode.
    /// Returns `true` when the cursor state changed.
    pub fn skip_last_added_occurrence(&mut self) -> bool {
        let result = self
            .keyboard_handler
            .skip_last_added_occurrence(&self.state.document, &self.state.cursor);
        match result {
            KeyResult::Command(cmd) => {
                self.apply_command_internal(cmd);
                true
            },
            KeyResult::Handled
            | KeyResult::Ignored
            | KeyResult::History(_)
            | KeyResult::Ast(_)
            | KeyResult::HostCommand { .. }
            | KeyResult::Clipboard(_)
            | KeyResult::Search(_) => false,
        }
    }

    /// Handles a search action from keyboard input.
    fn handle_search_action(&mut self, action: SearchAction) -> EditorKeyResult {
        match action {
            SearchAction::OpenSearch => EditorKeyResult::Search(action),
            SearchAction::CloseSearch => {
                self.close_search();
                EditorKeyResult::Search(action)
            },
            SearchAction::NextMatch => {
                self.goto_next_match();
                EditorKeyResult::None
            },
            SearchAction::PreviousMatch => {
                self.goto_previous_match();
                EditorKeyResult::None
            },
        }
    }

    /// Handles a mouse event.
    pub fn handle_mouse(&mut self, event: &MouseEvent) {
        let result = self.mouse_handler.handle_mouse(
            event,
            &self.state.document,
            &self.state.cursor,
            &self.state.viewport,
        );

        match result {
            MouseResult::Command(cmd) => {
                // Mouse-driven cursor changes bypass the keyboard handler;
                // drop its sticky vertical column so the next Up/Down starts
                // from the clicked position.
                self.keyboard_handler.reset_vertical_state();
                self.keyboard_handler.invalidate_cursor_order();
                self.apply_command_internal(cmd);
            },
            MouseResult::Scroll { delta_x, delta_y } => {
                // Handle scrolling
                self.scroll_by(delta_x, delta_y);
            },
            MouseResult::ToggleFold { line } => {
                // T136: Handle fold indicator click
                self.toggle_fold_at(line);
            },
            MouseResult::ScrollToLine { target_line } => {
                // Handle minimap click-to-navigate (T142)
                self.scroll_to_line(target_line);
            },
            MouseResult::Handled | MouseResult::Ignored => {},
        }
    }

    /// Handles an IME event for input method composition.
    ///
    /// Returns the current IME state if it changed, allowing the host to
    /// render composition text inline.
    pub fn handle_ime(&mut self, event: &ImeEvent) -> Option<ImeState> {
        if self.state.read_only {
            return None;
        }

        let result = self
            .ime_handler
            .handle_ime(event, &self.state.document, &self.state.cursor);

        match result {
            ImeResult::Command(cmd) => {
                // IME commits move the cursor without going through the
                // keyboard handler; drop its sticky vertical column.
                self.keyboard_handler.reset_vertical_state();
                self.keyboard_handler.invalidate_cursor_order();
                self.apply_command_internal(cmd);
                None
            },
            ImeResult::StateChanged(state) => Some(state),
            ImeResult::Handled | ImeResult::Ignored => None,
        }
    }

    /// Returns the current IME composition state.
    #[must_use]
    pub const fn ime_state(&self) -> &ImeState {
        self.ime_handler.state()
    }

    /// Returns true if IME composition is in progress.
    #[must_use]
    pub const fn is_ime_composing(&self) -> bool {
        self.ime_handler.is_composing()
    }

    /// Returns the current IME composition text, if any.
    #[must_use]
    pub fn ime_composition_text(&self) -> Option<&str> {
        self.ime_handler.composition_text()
    }

    /// Scrolls the viewport by the given amount.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn scroll_by(&mut self, delta_x: f32, delta_y: f32) {
        self.state.scroll_x = (self.state.scroll_x + delta_x).max(0.0);

        // Convert pixel delta to lines using configured line height
        let line_height_px = self.state.config.font_size * self.state.config.line_height;
        let line_delta = (delta_y / line_height_px) as i32;
        let new_line = if line_delta < 0 {
            self.state
                .scroll_line
                .saturating_sub(line_delta.unsigned_abs() as usize)
        } else {
            self.state.scroll_line.saturating_add(line_delta as usize)
        };

        // Clamp to document bounds
        let max_line = self.state.document.line_count().saturating_sub(1);
        self.state.scroll_line = new_line.min(max_line);

        self.emit(&EditorEvent::ScrollChanged {
            first_line: self.state.scroll_line,
        });
    }

    /// Scrolls to show a specific line (T142: minimap click-to-navigate).
    ///
    /// The viewport will be positioned to show the target line.
    /// Used by minimap click and drag navigation.
    pub fn scroll_to_line(&mut self, line: usize) {
        // Clamp to document bounds
        let max_line = self.state.document.line_count().saturating_sub(1);
        self.state.scroll_line = line.min(max_line);

        self.emit(&EditorEvent::ScrollChanged {
            first_line: self.state.scroll_line,
        });
    }

    /// Applies a command to the document (public, host-driven entry point).
    ///
    /// This is the out-of-band command surface for host code (apply a bare
    /// `Insert`/`Delete`/`Replace`, or push a `SetSelection`). Like every other
    /// host-driven cursor mutation it first invalidates the keyboard handler's
    /// transient state — the sticky vertical column and the multi-cursor
    /// addition-order stack — so a `SetSelection` that reconstructs a
    /// byte-identical `CursorState` cannot leave Ctrl+U/skip acting on a stale
    /// stack. The internal keyboard/mouse/IME/paste paths deliberately bypass
    /// this via [`Self::apply_command_internal`]: they manage that transient
    /// state themselves (the keyboard handler through its own post-dispatch
    /// bookkeeping, the others by resetting it explicitly before applying).
    ///
    /// Document modifications and selection changes both emit appropriate
    /// events; errors during command application are emitted as error events.
    pub fn apply_command(&mut self, command: Command) {
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        self.apply_command_internal(command);
    }

    /// Applies a command without touching the keyboard handler's transient
    /// state.
    ///
    /// Used by the input paths that own that state themselves: the keyboard
    /// handler (which runs its own post-dispatch bookkeeping in
    /// `note_operation`, and for the add verbs *must* keep its addition-order
    /// stack across command application) and the mouse/IME/paste/search paths
    /// (which reset the sticky column and addition-order stack explicitly before
    /// calling this).
    fn apply_command_internal(&mut self, command: Command) {
        let content_changed = command.modifies_content();
        let selection_changed = command.modifies_selection();

        // Computed before the edit lands, because the span is expressed in the
        // pre-edit document's coordinates and that document is about to stop
        // existing. An error here is not fatal: leaving the edit unreported
        // makes the next sync parse the document whole, which is slower and
        // still correct.
        let span = compute_edit_span(&self.state.document, &command)
            .ok()
            .flatten();

        // Apply the command
        if let Err(e) = command.apply(&mut self.state.document, &mut self.state.cursor) {
            self.emit(&EditorEvent::Error {
                message: e.to_string(),
                code: "COMMAND_FAILED".to_string(),
            });
            return;
        }

        if let Some(span) = span {
            self.state.syntax.note_edit(&self.state.document, &span);
        }

        // Push to undo history if it modifies content
        if content_changed {
            // Folds are read by the renderer on the very next frame, so they
            // are refreshed here rather than lazily. Before this the editor's
            // regions were correct only until the first keystroke.
            self.state.refresh_syntax();

            self.state.history.push(command);

            // Recorded search match ranges point into the pre-edit document;
            // re-synchronize the active search so replace operations never
            // act on stale ranges.
            if self.state.refresh_search() {
                self.emit_search_updated();
            }

            self.emit_content_changed();
        }

        // Emit selection changed if needed
        if selection_changed || content_changed {
            self.emit_selection_changed();
        }
    }

    /// Performs undo.
    ///
    /// Returns true if an action was undone.
    ///
    /// Commands recorded with a trailing `SetSelection` (all keyboard edits,
    /// paste, replace) restore the exact prior cursor state themselves. For
    /// history entries that carry no selection information (bare
    /// `Insert`/`Delete`/`Replace` from host-driven [`Self::apply_command`]
    /// calls), the cursor is explicitly placed at the edit site — see
    /// [`replayed_command_caret`] — instead of being left wherever it was.
    pub fn undo(&mut self) -> bool {
        // Note: history.undo() already returns the inverse command, so it is
        // applied directly without calling .inverse() again.
        let Some(cmd) = self.state.history.undo() else {
            return false;
        };
        if !self.apply_replayed_command(&cmd, "UNDO_FAILED") {
            return false;
        }
        self.finish_history_replay(&cmd);
        true
    }

    /// Performs redo.
    ///
    /// Returns true if an action was redone.
    ///
    /// Cursor placement follows the same rules as [`Self::undo`]: commands
    /// carrying a `SetSelection` restore the cursor state themselves, and
    /// bare content commands place the cursor at the edit site.
    pub fn redo(&mut self) -> bool {
        let Some(cmd) = self.state.history.redo() else {
            return false;
        };
        if !self.apply_replayed_command(&cmd, "REDO_FAILED") {
            return false;
        }
        self.finish_history_replay(&cmd);
        true
    }

    /// Shared post-processing for a successfully applied undo/redo command.
    ///
    /// - Places the cursor at the edit site when the replayed command carries
    ///   no `SetSelection` of its own (documented behavior of [`Self::undo`]
    ///   and [`Self::redo`]).
    /// - Brings the parse tree and the fold regions back in step with the
    ///   document.
    /// - Re-synchronizes the active search with the mutated document.
    /// - Emits content, search, and selection events.
    ///
    /// Note it deliberately does **not** reset the keyboard handler's sticky
    /// vertical column. The handler validates its sticky columns against the
    /// exact cursor state they were captured for, so restoring the prior
    /// cursor state on undo/redo also revives its per-cursor sticky columns —
    /// a multi-cursor block built by add-above/below keeps its preferred
    /// columns across an undone edit. A stale sticky column cannot leak,
    /// because any other restored state simply fails the identity check.
    pub(super) fn finish_history_replay(&mut self, cmd: &Command) {
        if !command_restores_selection(cmd) {
            if let Some(position) = replayed_command_caret(cmd) {
                let clamped = self.state.document.clamp_position(position);
                self.state.cursor = CursorState::at(clamped);
            }
        }

        // History replay moves the cursor outside the keyboard handler. The
        // multi-cursor addition-order stack is meaningless against a replayed
        // state and must not let Ctrl+U/skip act on it, so it is invalidated.
        // The sticky columns are deliberately NOT discarded: restoring the
        // exact cursor state an edit was made from also revives the per-cursor
        // sticky columns captured before that edit (a multi-cursor block built
        // by add-above/below keeps its preferred columns across an undone
        // edit). Clearing only the dirty flag lets that revival happen while
        // the columns' cursor-state/revision validation still guards against
        // resurrecting a mismatched set.
        self.keyboard_handler.invalidate_cursor_order();
        self.keyboard_handler
            .revalidate_vertical_columns(&self.state.cursor, self.state.document.revision());

        // A replay changes the document exactly as a command does, so the tree
        // and the folds derived from it have to move with it. Without this an
        // undo left every fold region describing the document as it was before
        // the undo, until some later content command happened to refresh them —
        // and the renderer reads those regions on the very next frame.
        self.state.refresh_syntax();

        if self.state.refresh_search() {
            self.emit_search_updated();
        }

        self.emit_content_changed();
        self.emit_selection_changed();
    }

    /// Handles a paste operation with the given text.
    ///
    /// The text is inserted at every cursor (each cursor's selection is
    /// replaced) through the same multi-cursor command builder as keyboard
    /// input, so the whole paste is one reversible command whose trailing
    /// `SetSelection` restores both text and the exact prior cursor state on
    /// undo.
    pub fn paste(&mut self, text: &str) {
        if self.state.read_only || text.is_empty() {
            return;
        }

        // Paste moves the cursor without going through handle_key.
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();

        let edits = editing::replace_all_edits(&self.state.cursor, text);
        if let Some(command) =
            editing::build_multi_cursor_command(&self.state.document, &self.state.cursor, edits)
        {
            self.apply_command_internal(command);
        }
    }

    /// Adds an event listener.
    pub fn add_listener<F>(&mut self, listener: F)
    where
        F: Fn(&EditorEvent) + Send + Sync + 'static,
    {
        self.listeners.push(Box::new(listener));
    }

    /// Emits an event to all listeners.
    pub(super) fn emit(&self, event: &EditorEvent) {
        for listener in &self.listeners {
            listener(event);
        }
    }

    /// Emits a content changed event (T060).
    fn emit_content_changed(&self) {
        self.emit(&EditorEvent::ContentChanged {
            content: self.state.content(),
        });
    }

    /// Emits a selection changed event (T061).
    fn emit_selection_changed(&self) {
        let selections: Vec<Selection> = self.state.cursor.all_selections().copied().collect();
        self.emit(&EditorEvent::SelectionChanged { selections });
    }

    /// Returns the current theme (T152).
    #[must_use]
    pub const fn get_theme(&self) -> &Theme {
        &self.state.theme
    }

    /// Returns the current configuration.
    #[must_use]
    pub const fn get_config(&self) -> &EditorConfig {
        &self.state.config
    }

    /// Replaces the configuration at runtime.
    ///
    /// The undo grouping timeout is applied to the existing history as well
    /// as stored, because `UndoTree` reads it at construction. Storing it
    /// alone would leave the setting visible in the config and inert in
    /// behaviour — the config would say one thing and the editor do another,
    /// which is worse than not supporting the change at all.
    ///
    /// Nothing else in [`EditorConfig`] is cached elsewhere: the remaining
    /// fields are read at the point of use.
    ///
    /// # Arguments
    ///
    /// * `config` - The new configuration
    pub fn set_config(&mut self, config: EditorConfig) {
        self.state
            .history
            .set_group_timeout_ms(config.undo_group_timeout_ms);
        self.state.config = config;
    }

    /// Sets the theme at runtime (T147, T152).
    ///
    /// This updates the editor's theme and emits a `ThemeChanged` event.
    /// The change takes effect immediately without requiring a restart.
    ///
    /// # Arguments
    ///
    /// * `theme` - The new theme to apply
    ///
    /// # Example
    ///
    /// ```
    /// use iridium_editor::{Editor, EditorConfig};
    /// use iridium_editor::theme::Theme;
    ///
    /// let mut editor = Editor::with_defaults();
    ///
    /// // Switch to light theme
    /// editor.set_theme(Theme::light());
    ///
    /// // Or load a custom theme from JSON
    /// // let custom = Theme::from_json(json_str).unwrap();
    /// // editor.set_theme(custom);
    /// ```
    pub fn set_theme(&mut self, theme: Theme) {
        let theme_name = theme.name.clone();
        let is_dark = theme.is_dark;

        self.state.theme = theme;

        self.emit(&EditorEvent::ThemeChanged {
            theme_name,
            is_dark,
        });
    }

    /// Returns true if the current theme is dark.
    #[must_use]
    pub const fn is_dark_theme(&self) -> bool {
        self.state.theme.is_dark
    }

    /// Switches to the default dark theme.
    pub fn use_dark_theme(&mut self) {
        self.set_theme(Theme::dark());
    }

    /// Switches to the default light theme.
    pub fn use_light_theme(&mut self) {
        self.set_theme(Theme::light());
    }

    /// Emits a theme changed event (T147).
    #[allow(dead_code)]
    fn emit_theme_changed(&self) {
        self.emit(&EditorEvent::ThemeChanged {
            theme_name: self.state.theme.name.clone(),
            is_dark: self.state.theme.is_dark,
        });
    }

    // =========================================================================
    // Code Folding (T130-T133)
    // =========================================================================

    /// Sets the language for syntax-aware folding.
    ///
    /// This initializes the fold detector for the given language and
    /// detects fold regions in the current content.
    pub fn set_language(&mut self, language: Language) {
        self.state.set_language(language);
        self.emit_fold_changed();
    }

    /// Returns the current language, if any.
    #[must_use]
    pub const fn language(&self) -> Option<Language> {
        self.state.language()
    }

    /// Returns a reference to the fold state.
    #[must_use]
    pub const fn fold_state(&self) -> &FoldState {
        &self.state.fold_state
    }

    /// Returns a mutable reference to the fold state.
    pub const fn fold_state_mut(&mut self) -> &mut FoldState {
        &mut self.state.fold_state
    }

    /// Folds the region at the given line (T131).
    ///
    /// Returns true if the fold was successful, false if the line
    /// is not foldable or already folded.
    pub fn fold_at(&mut self, line: usize) -> bool {
        let result = self.state.fold_state.fold_at(line);
        if result {
            self.emit_fold_changed();
        }
        result
    }

    /// Unfolds the region at the given line (T132).
    ///
    /// Returns true if the unfold was successful, false if the line
    /// is not currently folded.
    pub fn unfold_at(&mut self, line: usize) -> bool {
        let result = self.state.fold_state.unfold_at(line);
        if result {
            self.emit_fold_changed();
        }
        result
    }

    /// Toggles the fold state of the region at the given line.
    ///
    /// Returns true if the toggle was successful, false if the line
    /// is not foldable.
    pub fn toggle_fold_at(&mut self, line: usize) -> bool {
        let result = self.state.fold_state.toggle_fold_at(line);
        if result {
            self.emit_fold_changed();
        }
        result
    }

    /// Folds all foldable regions (T133).
    pub fn fold_all(&mut self) {
        self.state.fold_state.fold_all();
        self.emit_fold_changed();
    }

    /// Unfolds all folded regions (T133).
    pub fn unfold_all(&mut self) {
        self.state.fold_state.unfold_all();
        self.emit_fold_changed();
    }

    /// Folds all regions of a specific kind.
    pub fn fold_all_of_kind(&mut self, kind: FoldKind) {
        self.state.fold_state.fold_all_of_kind(kind);
        self.emit_fold_changed();
    }

    /// Unfolds all regions of a specific kind.
    pub fn unfold_all_of_kind(&mut self, kind: FoldKind) {
        self.state.fold_state.unfold_all_of_kind(kind);
        self.emit_fold_changed();
    }

    /// Returns true if the given line is foldable.
    #[must_use]
    pub fn is_foldable(&self, line: usize) -> bool {
        self.state.fold_state.is_foldable(line)
    }

    /// Returns true if the given line is currently folded.
    #[must_use]
    pub fn is_folded(&self, line: usize) -> bool {
        self.state.fold_state.is_folded(line)
    }

    /// Returns true if the given line is hidden by a fold.
    #[must_use]
    pub fn is_line_hidden(&self, line: usize) -> bool {
        self.state.fold_state.is_line_hidden(line)
    }

    /// Returns the number of currently folded regions.
    #[must_use]
    pub fn folded_count(&self) -> usize {
        self.state.fold_state.folded_count()
    }

    /// Returns the total number of foldable regions.
    #[must_use]
    pub fn fold_region_count(&self) -> usize {
        self.state.fold_state.regions().len()
    }

    /// Maps a visual line number to a document line number.
    ///
    /// Visual lines skip over hidden (folded) lines.
    #[must_use]
    pub fn visual_to_document_line(&self, visual_line: usize) -> usize {
        self.state.fold_state.visual_to_document_line(visual_line)
    }

    /// Maps a document line number to a visual line number.
    ///
    /// Returns None if the document line is hidden by a fold.
    #[must_use]
    pub fn document_to_visual_line(&self, doc_line: usize) -> Option<usize> {
        self.state.fold_state.document_to_visual_line(doc_line)
    }

    /// Returns the visible line count (total lines minus hidden lines).
    #[must_use]
    pub fn visible_line_count(&self) -> usize {
        let total = self.state.document.line_count();
        self.state.fold_state.visible_line_count(total)
    }

    /// Exports fold information for persistence.
    #[must_use]
    pub fn export_fold_info(&self) -> FoldInfo {
        self.state.fold_state.export_fold_info()
    }

    /// Imports fold information (e.g., from a saved session).
    pub fn import_fold_info(&mut self, info: &FoldInfo) {
        self.state.fold_state.import_fold_info(info);
        self.emit_fold_changed();
    }

    /// Emits a fold changed event.
    fn emit_fold_changed(&self) {
        self.emit(&EditorEvent::FoldChanged {
            folded_count: self.state.fold_state.folded_count(),
            total_regions: self.state.fold_state.regions().len(),
        });
    }

    // ==================== Search Methods (T121-T126) ====================

    /// Starts a search with the given query and options.
    ///
    /// This finds all matches in the document and updates the search state.
    /// The current match is set to the first match found (if any).
    ///
    /// # Arguments
    ///
    /// * `query` - The search query (plain text or regex pattern)
    /// * `options` - Search options controlling matching behavior
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or `Err(String)` if the regex is invalid.
    pub fn find(&mut self, query: &str, options: &SearchOptions) -> Result<(), String> {
        self.state
            .search
            .find_all(query, options, &self.state.document)?;

        // Move to nearest match from current cursor position
        if self.state.search.has_matches() {
            self.state
                .search
                .goto_nearest_match(self.state.cursor.primary.head);

            // Move cursor to current match
            if let Some(range) = self.state.search.current_range() {
                self.keyboard_handler.reset_vertical_state();
                self.keyboard_handler.invalidate_cursor_order();
                self.state.cursor = CursorState::at(range.start);
                self.emit_selection_changed();
            }
        }

        self.emit_search_updated();
        Ok(())
    }

    /// Updates the search query incrementally.
    ///
    /// This is useful for live search where the query is updated as the user types.
    ///
    /// # Arguments
    ///
    /// * `query` - The new search query
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the matches changed, `Ok(false)` otherwise,
    /// or `Err(String)` if the regex is invalid.
    pub fn update_search(&mut self, query: &str) -> Result<bool, String> {
        let changed = self
            .state
            .search
            .update_query(query, &self.state.document)?;
        if changed {
            self.emit_search_updated();
        }
        Ok(changed)
    }

    /// Sets search options and re-runs the search.
    pub fn set_search_options(&mut self, options: SearchOptions) -> Result<(), String> {
        self.state
            .search
            .set_options(options, &self.state.document)?;
        self.emit_search_updated();
        Ok(())
    }

    /// Goes to the next search match.
    ///
    /// Wraps around to the first match when at the end.
    pub fn goto_next_match(&mut self) {
        self.state.search.next_match();

        if let Some(range) = self.state.search.current_range() {
            self.keyboard_handler.reset_vertical_state();
            self.keyboard_handler.invalidate_cursor_order();
            self.state.cursor = CursorState::at(range.start);
            self.emit_selection_changed();
        }

        self.emit_search_updated();
    }

    /// Goes to the previous search match.
    ///
    /// Wraps around to the last match when at the beginning.
    pub fn goto_previous_match(&mut self) {
        self.state.search.previous_match();

        if let Some(range) = self.state.search.current_range() {
            self.keyboard_handler.reset_vertical_state();
            self.keyboard_handler.invalidate_cursor_order();
            self.state.cursor = CursorState::at(range.start);
            self.emit_selection_changed();
        }

        self.emit_search_updated();
    }

    /// Replaces the current match with the replacement text.
    ///
    /// After replacement, moves to the next match.
    ///
    /// # Arguments
    ///
    /// * `replacement` - The text to replace the current match with
    ///
    /// # Returns
    ///
    /// Returns `true` if a replacement was made, `false` otherwise.
    pub fn replace_current_match(&mut self, replacement: &str) -> bool {
        if self.state.read_only {
            return false;
        }

        let cmd = replace_current(
            &self.state.search,
            replacement,
            &self.state.document,
            &self.state.cursor,
        );

        if let Some(cmd) = cmd {
            // The replace command carries a SetSelection that moves the
            // cursor without going through handle_key.
            self.keyboard_handler.reset_vertical_state();
            self.keyboard_handler.invalidate_cursor_order();
            self.apply_command_internal(cmd);

            // Re-run search from the top so navigation restarts at the first
            // match (apply_command already refreshed the stale ranges; this
            // additionally resets the current-match index for the
            // goto_next_match below, preserving long-standing behavior).
            let query = self.state.search.query.clone();
            let options = self.state.search.options.clone();
            let _ = self
                .state
                .search
                .find_all(&query, &options, &self.state.document);

            // Move to next match
            self.goto_next_match();
            true
        } else {
            false
        }
    }

    /// Replaces all matches with the replacement text.
    ///
    /// This is a single undoable operation per FR-030.
    ///
    /// # Arguments
    ///
    /// * `replacement` - The text to replace each match with
    ///
    /// # Returns
    ///
    /// Returns the number of replacements made.
    pub fn replace_all_matches(&mut self, replacement: &str) -> usize {
        if self.state.read_only {
            return 0;
        }

        let result = replace_all(
            &self.state.search,
            replacement,
            &self.state.document,
            &self.state.cursor,
        );

        if let Some((cmd, replace_result)) = result {
            // The replace command carries a SetSelection that moves the
            // cursor without going through handle_key.
            self.keyboard_handler.reset_vertical_state();
            self.keyboard_handler.invalidate_cursor_order();
            self.apply_command_internal(cmd);

            // Clear search since all matches are replaced
            self.close_search();

            replace_result.count
        } else {
            0
        }
    }

    /// Closes/clears the current search.
    pub fn close_search(&mut self) {
        self.state.search.clear();
        self.emit_search_updated();
    }

    /// Returns true if a search is currently active.
    #[must_use]
    pub const fn is_searching(&self) -> bool {
        self.state.search.is_active
    }

    /// Returns the current search state.
    #[must_use]
    pub const fn search_state(&self) -> &SearchState {
        &self.state.search
    }

    /// Returns the number of search matches.
    #[must_use]
    pub fn search_match_count(&self) -> usize {
        self.state.search.match_count()
    }

    /// Returns the current match index (0-based), if any.
    #[must_use]
    pub const fn current_match_index(&self) -> Option<usize> {
        self.state.search.current_match
    }

    /// Emits a search updated event (T125).
    fn emit_search_updated(&self) {
        self.emit(&EditorEvent::SearchUpdated {
            match_count: self.state.search.match_count(),
            current_index: self.state.search.current_match,
        });
    }
}

/// Returns true if the command (or any nested command) sets the cursor state
/// itself via `SetSelection`.
///
/// Such commands need no fallback caret placement after undo/redo: the
/// recorded selection states restore the cursor exactly.
fn command_restores_selection(command: &Command) -> bool {
    match command {
        Command::SetSelection { .. } => true,
        Command::Compound { commands } => commands.iter().any(command_restores_selection),
        Command::Insert { .. } | Command::Delete { .. } | Command::Replace { .. } => false,
    }
}

/// Fallback caret position for a replayed (undo/redo) command that carries no
/// `SetSelection` of its own.
///
/// The caret is placed at the edit site of the applied command:
/// - `Insert` (the undo of a delete, or a redone insert): the end of the
///   inserted text.
/// - `Delete` (the undo of an insert, or a redone delete): the start of the
///   removed range.
/// - `Replace`: the end of the newly inserted text.
/// - `Compound`: the first content command's site.
///
/// Returns `None` when the command contains no content edit at all.
fn replayed_command_caret(command: &Command) -> Option<Position> {
    match command {
        Command::Insert { position, text } => Some(position.advanced_through(text)),
        Command::Delete { range, .. } => Some(range.start),
        Command::Replace {
            range, new_text, ..
        } => Some(range.start.advanced_through(new_text)),
        Command::SetSelection { .. } => None,
        Command::Compound { commands } => commands.iter().find_map(replayed_command_caret),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::document::Range;
    use crate::input::{KeyCode, Modifiers};

    #[test]
    fn editor_state_new() {
        let state = EditorState::new("Hello, world!");
        assert_eq!(state.content(), "Hello, world!");
        assert_eq!(state.cursor.cursor_count(), 1);
    }

    #[test]
    fn editor_set_content() {
        let mut editor = Editor::with_defaults();
        editor.set_content("Hello");
        assert_eq!(editor.content(), "Hello");
    }

    #[test]
    fn editor_cursor() {
        let mut editor = Editor::with_defaults();
        editor.set_content("Hello\nWorld");
        editor.set_cursor(Position::new(1, 3));
        assert_eq!(editor.cursor(), Position::new(1, 3));
    }

    #[test]
    fn editor_get_theme() {
        let editor = Editor::with_defaults();
        let theme = editor.get_theme();
        assert_eq!(theme.name, "Iridium Dark");
    }

    #[test]
    fn editor_set_theme() {
        let mut editor = Editor::with_defaults();
        assert!(editor.is_dark_theme());

        editor.set_theme(Theme::light());
        assert!(!editor.is_dark_theme());
        assert_eq!(editor.get_theme().name, "Iridium Light");
    }

    #[test]
    fn editor_use_dark_light_theme() {
        let mut editor = Editor::with_defaults();

        editor.use_light_theme();
        assert!(!editor.is_dark_theme());

        editor.use_dark_theme();
        assert!(editor.is_dark_theme());
    }

    #[test]
    fn editor_theme_changed_event() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut editor = Editor::with_defaults();
        let event_received = Arc::new(AtomicBool::new(false));
        let event_received_clone = Arc::clone(&event_received);

        editor.add_listener(move |event| {
            if matches!(event, EditorEvent::ThemeChanged { .. }) {
                event_received_clone.store(true, Ordering::SeqCst);
            }
        });

        editor.set_theme(Theme::light());
        assert!(event_received.load(Ordering::SeqCst));
    }

    // =========================================================================
    // Fold Tests (T130-T133)
    // =========================================================================

    #[test]
    fn editor_fold_at() {
        let mut editor = Editor::with_defaults();
        let code = "fn main() {\n    println!(\"hello\");\n}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        // Should have detected the function
        assert!(editor.fold_region_count() > 0);
        assert!(editor.is_foldable(0));
        assert!(!editor.is_folded(0));

        // Fold it
        assert!(editor.fold_at(0));
        assert!(editor.is_folded(0));
        assert!(editor.is_line_hidden(1));
    }

    #[test]
    fn editor_unfold_at() {
        let mut editor = Editor::with_defaults();
        let code = "fn main() {\n    println!(\"hello\");\n}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        editor.fold_at(0);
        assert!(editor.is_folded(0));

        // Unfold it
        assert!(editor.unfold_at(0));
        assert!(!editor.is_folded(0));
        assert!(!editor.is_line_hidden(1));
    }

    #[test]
    fn editor_toggle_fold_at() {
        let mut editor = Editor::with_defaults();
        let code = "fn main() {\n    println!(\"hello\");\n}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        assert!(!editor.is_folded(0));
        assert!(editor.toggle_fold_at(0));
        assert!(editor.is_folded(0));
        assert!(editor.toggle_fold_at(0));
        assert!(!editor.is_folded(0));
    }

    #[test]
    fn editor_fold_all_unfold_all() {
        let mut editor = Editor::with_defaults();
        let code = r#"fn foo() {
    println!("foo");
}

fn bar() {
    println!("bar");
}"#;
        editor.set_content(code);
        editor.set_language(Language::Rust);

        editor.fold_all();
        assert!(editor.is_folded(0));
        assert!(editor.is_folded(4));

        editor.unfold_all();
        assert!(!editor.is_folded(0));
        assert!(!editor.is_folded(4));
    }

    #[test]
    fn editor_visible_line_count() {
        let mut editor = Editor::with_defaults();
        let code = r"fn main() {
    line 1
    line 2
    line 3
}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        let total_lines = 5;
        assert_eq!(editor.visible_line_count(), total_lines);

        editor.fold_at(0);
        // After folding, 4 lines are hidden (lines 1-4)
        assert_eq!(editor.visible_line_count(), 1);
    }

    #[test]
    fn editor_visual_line_mapping() {
        let mut editor = Editor::with_defaults();
        let code = r"line 0
fn foo() {
    hidden 1
    hidden 2
}
line 5";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        // Fold the function
        editor.fold_at(1);

        // Visual line mapping
        assert_eq!(editor.document_to_visual_line(0), Some(0));
        assert_eq!(editor.document_to_visual_line(1), Some(1));
        assert_eq!(editor.document_to_visual_line(2), None); // Hidden
        assert_eq!(editor.document_to_visual_line(3), None); // Hidden
        assert_eq!(editor.document_to_visual_line(4), None); // Hidden (closing brace)
        assert_eq!(editor.document_to_visual_line(5), Some(2));
    }

    #[test]
    fn editor_fold_changed_event() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut editor = Editor::with_defaults();
        let event_received = Arc::new(AtomicBool::new(false));
        let event_received_clone = Arc::clone(&event_received);

        editor.add_listener(move |event| {
            if matches!(event, EditorEvent::FoldChanged { .. }) {
                event_received_clone.store(true, Ordering::SeqCst);
            }
        });

        let code = "fn main() {\n    println!(\"hello\");\n}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        // Language setting should emit fold changed
        assert!(event_received.load(Ordering::SeqCst));
    }

    // =========================================================================
    // Paste (multi-cursor command path)
    // =========================================================================

    /// Returns the head positions of all cursors in `all_selections` order
    /// (primary first).
    fn cursor_heads(editor: &Editor) -> Vec<Position> {
        editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.head)
            .collect()
    }

    #[test]
    fn paste_multi_cursor_inserts_at_every_cursor() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abcd");
        editor.set_cursor(Position::new(0, 0));
        editor
            .state_mut()
            .cursor
            .add_cursor(Selection::collapsed(Position::new(0, 2)));

        editor.paste("X");

        assert_eq!(editor.content(), "XabXcd");
        assert_eq!(
            cursor_heads(&editor),
            vec![Position::new(0, 1), Position::new(0, 4)]
        );
        assert!(
            editor
                .state()
                .cursor
                .all_selections()
                .all(Selection::is_collapsed)
        );
    }

    #[test]
    fn undo_of_paste_restores_text_and_all_cursors() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abcd");
        editor.set_cursor(Position::new(0, 0));
        editor
            .state_mut()
            .cursor
            .add_cursor(Selection::collapsed(Position::new(0, 2)));

        editor.paste("X");
        assert_eq!(editor.content(), "XabXcd");

        // A single undo must restore both the text and the exact prior
        // multi-cursor state.
        assert!(editor.undo());
        assert_eq!(editor.content(), "abcd");
        assert_eq!(
            cursor_heads(&editor),
            vec![Position::new(0, 0), Position::new(0, 2)]
        );
    }

    #[test]
    fn paste_replaces_forward_selection_and_undoes() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abcd");
        // Forward selection: anchor before head. The old single-cursor paste
        // inserted at the stale pre-delete head after deleting the selection.
        editor.set_selection(Position::new(0, 1), Position::new(0, 3));

        editor.paste("Z");
        assert_eq!(editor.content(), "aZd");
        assert_eq!(editor.cursor(), Position::new(0, 2));

        assert!(editor.undo());
        assert_eq!(editor.content(), "abcd");
        assert_eq!(editor.state().cursor.primary.anchor, Position::new(0, 1));
        assert_eq!(editor.state().cursor.primary.head, Position::new(0, 3));
    }

    #[test]
    fn paste_is_noop_when_read_only_or_empty() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abcd");
        editor.set_cursor(Position::new(0, 2));

        editor.paste("");
        assert_eq!(editor.content(), "abcd");

        editor.state_mut().read_only = true;
        editor.paste("X");
        assert_eq!(editor.content(), "abcd");
        assert_eq!(editor.cursor(), Position::new(0, 2));
    }

    /// Regression test (Norn review, 2026-07-12): delete-forward leaves every
    /// caret in place, so the generated compound used to carry no
    /// `SetSelection` — undo's bare-command fallback then collapsed the
    /// multi-cursor state to a single caret. The builder now always records
    /// the cursor transition when content changes, so undo restores every
    /// cursor exactly.
    #[test]
    fn undo_of_multi_cursor_delete_forward_restores_all_cursors() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abc\ndef");
        editor.state_mut().cursor = CursorState {
            primary: Selection::collapsed(Position::new(0, 1)),
            secondary: vec![Selection::collapsed(Position::new(1, 1))],
        };

        let event = KeyEvent::new(KeyCode::Delete, Modifiers::none());
        editor.handle_key(&event);
        assert_eq!(editor.content(), "ac\ndf");
        assert_eq!(editor.state().cursor.cursor_count(), 2);

        assert!(editor.undo());
        assert_eq!(editor.content(), "abc\ndef");
        assert_eq!(
            editor.state().cursor.cursor_count(),
            2,
            "undo must restore the multi-cursor state, not collapse it"
        );
        assert_eq!(editor.state().cursor.primary.head, Position::new(0, 1));
        assert_eq!(editor.state().cursor.secondary[0].head, Position::new(1, 1));

        assert!(editor.redo());
        assert_eq!(editor.content(), "ac\ndf");
        assert_eq!(editor.state().cursor.cursor_count(), 2);
    }

    // =========================================================================
    // Multi-cursor skip verb (threaded through the editor)
    // =========================================================================

    #[test]
    fn editor_skip_last_added_occurrence_drops_and_advances() {
        let mut editor = Editor::with_defaults();
        editor.set_content("foo foo foo foo");
        editor.set_cursor(Position::new(0, 1));

        // Build up three occurrence cursors via Ctrl+D.
        let ctrl_d = KeyEvent::new(KeyCode::Char('d'), Modifiers::ctrl());
        editor.handle_key(&ctrl_d); // select word "foo"
        editor.handle_key(&ctrl_d); // add @4
        editor.handle_key(&ctrl_d); // add @8
        assert_eq!(editor.state().cursor.cursor_count(), 3);

        // Skip drops the @8 cursor and advances to @12.
        assert!(editor.skip_last_added_occurrence());
        let heads: Vec<Position> = editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.range().start)
            .collect();
        assert_eq!(
            heads,
            vec![
                Position::new(0, 0),
                Position::new(0, 4),
                Position::new(0, 12)
            ]
        );
    }

    #[test]
    fn editor_skip_last_added_occurrence_single_cursor_is_noop() {
        let mut editor = Editor::with_defaults();
        editor.set_content("hello world");
        editor.set_cursor(Position::new(0, 0));

        // No word under a caret at a boundary/space? Caret at 0 is on 'h'; the
        // bootstrap selects "hello". A second skip with only one occurrence is
        // a no-op.
        assert!(editor.skip_last_added_occurrence());
        assert!(!editor.skip_last_added_occurrence());
    }

    // =========================================================================
    // Undo/redo cursor placement for bare (selection-less) commands
    // =========================================================================

    #[test]
    fn undo_of_bare_insert_places_cursor_at_insert_position() {
        let mut editor = Editor::with_defaults();
        editor.set_content("hello world");
        editor.apply_command(Command::Insert {
            position: Position::new(0, 5),
            text: "X".to_string(),
        });
        assert_eq!(editor.content(), "helloX world");

        // Move the caret away to prove undo repositions it explicitly.
        editor.set_cursor(Position::new(0, 0));

        assert!(editor.undo());
        assert_eq!(editor.content(), "hello world");
        // Undoing an insert removes the text; the caret lands at the start
        // of the removed range.
        assert_eq!(editor.cursor(), Position::new(0, 5));

        assert!(editor.redo());
        assert_eq!(editor.content(), "helloX world");
        // Redoing the insert places the caret at the end of the inserted text.
        assert_eq!(editor.cursor(), Position::new(0, 6));
    }

    #[test]
    fn undo_of_bare_delete_places_cursor_at_end_of_restored_text() {
        let mut editor = Editor::with_defaults();
        editor.set_content("hello world");
        editor.apply_command(Command::Delete {
            range: Range::new(Position::new(0, 0), Position::new(0, 6)),
            deleted_text: "hello ".to_string(),
        });
        assert_eq!(editor.content(), "world");
        editor.set_cursor(Position::new(0, 3));

        assert!(editor.undo());
        assert_eq!(editor.content(), "hello world");
        // Undoing a delete re-inserts the text; the caret lands at its end.
        assert_eq!(editor.cursor(), Position::new(0, 6));
    }

    // =========================================================================
    // Search invalidation on document mutation
    // =========================================================================

    #[test]
    fn typing_with_active_search_refreshes_match_ranges() {
        let mut editor = Editor::with_defaults();
        editor.set_content("foo bar foo");
        editor.find("foo", &SearchOptions::default()).unwrap();
        assert_eq!(editor.search_match_count(), 2);

        editor.set_cursor(Position::new(0, 0));
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "xfoo bar foo");

        // Matches must reflect the mutated document, not the stale offsets.
        let starts: Vec<Position> = editor
            .search_state()
            .all_matches()
            .iter()
            .map(|range| range.start)
            .collect();
        assert_eq!(starts, vec![Position::new(0, 1), Position::new(0, 9)]);
    }

    #[test]
    fn replace_all_literal_after_edit_ignores_stale_ranges() {
        let mut editor = Editor::with_defaults();
        editor.set_content("foo bar foo");
        editor.find("foo", &SearchOptions::default()).unwrap();

        // Mutate the document while the search is active: every recorded
        // range shifts right by one column.
        editor.set_cursor(Position::new(0, 0));
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "xfoo bar foo");

        // Literal-mode replace trusts the recorded ranges verbatim; without
        // the refresh it would have replaced "xfo" and " fo".
        let replaced = editor.replace_all_matches("Z");
        assert_eq!(replaced, 2);
        assert_eq!(editor.content(), "xZ bar Z");
    }

    #[test]
    fn replace_all_regex_after_edit_replaces_refreshed_matches() {
        let mut editor = Editor::with_defaults();
        editor.set_content("foo bar foo");
        editor.find("f(o+)", &SearchOptions::regex_mode()).unwrap();

        editor.set_cursor(Position::new(0, 0));
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "xfoo bar foo");

        // Regex replace verifies matches; without the refresh both recorded
        // ranges would fail verification and nothing would be replaced.
        let replaced = editor.replace_all_matches("[$1]");
        assert_eq!(replaced, 2);
        assert_eq!(editor.content(), "x[oo] bar [oo]");
    }

    #[test]
    fn undo_with_active_search_refreshes_match_ranges() {
        let mut editor = Editor::with_defaults();
        editor.set_content("foo bar foo");
        editor.set_cursor(Position::new(0, 0));
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "xfoo bar foo");

        editor.find("foo", &SearchOptions::default()).unwrap();
        let starts: Vec<Position> = editor
            .search_state()
            .all_matches()
            .iter()
            .map(|range| range.start)
            .collect();
        assert_eq!(starts, vec![Position::new(0, 1), Position::new(0, 9)]);

        // Undo mutates the document outside apply_command; matches must
        // follow the restored text.
        assert!(editor.undo());
        assert_eq!(editor.content(), "foo bar foo");
        let starts: Vec<Position> = editor
            .search_state()
            .all_matches()
            .iter()
            .map(|range| range.start)
            .collect();
        assert_eq!(starts, vec![Position::new(0, 0), Position::new(0, 8)]);
    }

    // =========================================================================
    // Page motion: the hop is the viewport's height, from either entry point
    // =========================================================================

    /// A hundred numbered lines, so any wrong hop lands on a nameable line.
    fn hundred_lines() -> String {
        let lines: Vec<String> = (0..100).map(|index| format!("line {index}")).collect();
        lines.join("\n")
    }

    #[test]
    fn page_down_hops_the_caret_one_viewportful() {
        let mut editor = Editor::with_defaults();
        editor.set_content(&hundred_lines());
        // 600px of text at 20px per line: 30 visible rows.
        editor.state_mut().viewport = Viewport::new(800.0, 600.0, 20.0);

        editor.handle_key(&KeyEvent::simple(KeyCode::PageDown));
        assert_eq!(editor.cursor(), Position::new(30, 0));

        editor.handle_key(&KeyEvent::simple(KeyCode::PageUp));
        assert_eq!(editor.cursor(), Position::new(0, 0));
    }

    #[test]
    fn page_down_pages_identically_from_the_palette() {
        // `Editor::run_command` must page exactly as the key does: the palette
        // and the keyboard share one implementation, viewport hop included.
        let mut editor = Editor::with_defaults();
        editor.set_content(&hundred_lines());
        editor.state_mut().viewport = Viewport::new(800.0, 600.0, 20.0);

        editor
            .run_command("cursor.pageDown", CommandArgs::NONE)
            .unwrap_or_else(|error| panic!("cursor.pageDown must be a kernel command: {error}"));
        assert_eq!(editor.cursor(), Position::new(30, 0));
    }

    #[test]
    fn a_zero_row_viewport_still_pages_one_line() {
        // `visible_lines == 0` is legitimate — chrome can consume every row a
        // terminal has (the viewport panic of 699327f) — and a page key pressed
        // in that state must neither die nor panic. It degrades to one line,
        // so the key always does something visible.
        let mut editor = Editor::with_defaults();
        editor.set_content(&hundred_lines());
        editor.state_mut().viewport = Viewport::new(800.0, 0.0, 20.0);

        editor.handle_key(&KeyEvent::simple(KeyCode::PageDown));
        assert_eq!(editor.cursor(), Position::new(1, 0));
    }

    #[test]
    fn shift_page_down_extends_the_selection_a_viewportful() {
        let mut editor = Editor::with_defaults();
        editor.set_content(&hundred_lines());
        editor.state_mut().viewport = Viewport::new(800.0, 600.0, 20.0);

        editor.handle_key(&KeyEvent::new(KeyCode::PageDown, Modifiers::shift()));
        let selection = editor.state().cursor.primary;
        assert_eq!(selection.anchor, Position::new(0, 0));
        assert_eq!(selection.head, Position::new(30, 0));
    }

    #[test]
    fn page_motion_keeps_the_sticky_column_through_short_lines() {
        // Paging from a long line across a short one and on again must return
        // to the remembered column, exactly as single-line vertical motion
        // does — a page hop is vertical motion, so it shares the sticky state.
        let mut editor = Editor::with_defaults();
        // Line 0 and line 2 are 10 wide; line 1 is 2 wide. One-row pages.
        editor.set_content("aaaaaaaaaa\nbb\ncccccccccc");
        editor.state_mut().viewport = Viewport::new(800.0, 20.0, 20.0);
        editor.set_cursor(Position::new(0, 8));

        editor.handle_key(&KeyEvent::simple(KeyCode::PageDown));
        assert_eq!(editor.cursor(), Position::new(1, 2));
        editor.handle_key(&KeyEvent::simple(KeyCode::PageDown));
        assert_eq!(
            editor.cursor(),
            Position::new(2, 8),
            "the page hop must remember the column the way arrow motion does"
        );
    }

    #[test]
    fn page_down_clamps_to_the_last_line_and_page_up_to_the_first() {
        let mut editor = Editor::with_defaults();
        editor.set_content(&hundred_lines());
        // 30-row pages over a 100-line document.
        editor.state_mut().viewport = Viewport::new(800.0, 600.0, 20.0);
        editor.set_cursor(Position::new(90, 3));

        // 90 + 30 overshoots: clamp to line 99, keeping the column.
        editor.handle_key(&KeyEvent::simple(KeyCode::PageDown));
        assert_eq!(editor.cursor(), Position::new(99, 3));

        // Already at the last line: to the line end, as Down does.
        editor.handle_key(&KeyEvent::simple(KeyCode::PageDown));
        assert_eq!(editor.cursor(), Position::new(99, 7));

        // 20 - 30 undershoots: clamp to line 0, keeping the column.
        editor.set_cursor(Position::new(20, 3));
        editor.handle_key(&KeyEvent::simple(KeyCode::PageUp));
        assert_eq!(editor.cursor(), Position::new(0, 3));

        // Already at the first line: to the document start, as Up does.
        editor.handle_key(&KeyEvent::simple(KeyCode::PageUp));
        assert_eq!(editor.cursor(), Position::new(0, 0));
    }

    // =========================================================================
    // Sticky-column invalidation for cursor changes outside handle_key
    // =========================================================================

    #[test]
    fn set_cursor_resets_sticky_column_for_vertical_moves() {
        let mut editor = Editor::with_defaults();
        // Lines of length 10 / 2 / 10 (Norn review scenario).
        editor.set_content("aaaaaaaaaa\nbb\ncccccccccc");

        editor.set_cursor(Position::new(0, 8));
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(1, 2));

        // The host moves the cursor without going through handle_key; the
        // sticky column from the previous Down (8) must not survive, or the
        // next Down would jump to (2, 8).
        editor.set_cursor(Position::new(1, 1));
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(2, 1));
    }

    #[test]
    fn undo_preserves_sticky_columns_for_multi_cursor_block() {
        // Finding 3: sticky columns are validated against the exact cursor
        // state they were captured for, so undo/redo restoring that state also
        // revives its per-cursor sticky columns instead of discarding them.
        let mut editor = Editor::with_defaults();
        // Lines of length 10 / 2 / 10.
        editor.set_content("aaaaaaaaaa\nbb\ncccccccccc");
        editor.set_cursor(Position::new(0, 8));

        let ctrl_alt_down = KeyEvent::new(
            KeyCode::Down,
            Modifiers {
                shift: false,
                ctrl: true,
                alt: true,
                meta: false,
                alt_graph: false,
            },
        );

        // Add a cursor below: cursors at (0,8) and (1,2), sticky columns [8,8].
        editor.handle_key(&ctrl_alt_down);
        let heads: Vec<Position> = editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.head)
            .collect();
        assert_eq!(heads, vec![Position::new(0, 8), Position::new(1, 2)]);

        // Type at both cursors, then undo the edit.
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('z')));
        assert!(editor.undo());
        let heads: Vec<Position> = editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.head)
            .collect();
        assert_eq!(heads, vec![Position::new(0, 8), Position::new(1, 2)]);

        // Add below again: the line-1 cursor's sticky column (8) survived the
        // undo, so it lands at (2, 8) — not the clamped column (2).
        editor.handle_key(&ctrl_alt_down);
        let heads: Vec<Position> = editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.head)
            .collect();
        assert_eq!(
            heads,
            vec![
                Position::new(0, 8),
                Position::new(1, 2),
                Position::new(2, 8)
            ]
        );
    }

    #[test]
    fn undo_resets_sticky_column_for_vertical_moves() {
        let mut editor = Editor::with_defaults();
        editor.set_content("aaaaaaaaaa\nbb\ncccccccccc");

        // Bare insert on line 1 so undo's fallback caret placement moves the
        // cursor without a SetSelection.
        editor.apply_command(Command::Insert {
            position: Position::new(1, 1),
            text: "Q".to_string(),
        });

        editor.set_cursor(Position::new(0, 8));
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(1, 3));

        // Undo moves the caret to (1, 1); the sticky column (8) must reset.
        assert!(editor.undo());
        assert_eq!(editor.cursor(), Position::new(1, 1));
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(2, 1));
    }

    #[test]
    fn bare_content_edit_invalidates_sticky_column_via_revision() {
        // Finding 3 (revision): a host-applied bare content command shifts text
        // under the caret without moving its coordinates, so cursor-state
        // identity alone still matches. Validating the sticky column against the
        // document revision discards it, so the next vertical move re-seeds from
        // the live head column.
        let mut editor = Editor::with_defaults();
        editor.set_content("aaaaaaaaaa\nbb\ncccccccccc");
        editor.set_cursor(Position::new(0, 8));

        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(1, 2));

        // Insert on line 1 without moving the caret's (1, 2) coordinates.
        editor.apply_command(Command::Insert {
            position: Position::new(1, 0),
            text: "Z".to_string(),
        });
        assert_eq!(editor.cursor(), Position::new(1, 2));

        // Without the revision check the stale sticky column (8) would land the
        // caret at (2, 8); the fix re-seeds from column 2.
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(2, 2));
    }

    #[test]
    fn vertical_add_columns_survive_undo_of_a_later_edit() {
        // Finding 5 (reachable case): a vertical add produces a selection-only
        // SetSelection, which is intentionally NOT recorded in the undo history
        // (selection changes are not undoable, matching VS Code), so
        // editor.undo() never reverts the add itself. The reachable exactness
        // concern is undoing a *content* edit made after a vertical add: the
        // pre-edit sticky columns must survive so a subsequent add reproduces
        // the exact columns.
        let mut editor = Editor::with_defaults();
        // Four lines so add-below has room after the restored two-cursor block.
        editor.set_content("aaaaaaaaaa\nbb\ncccccccccc\ndddddddddd");
        editor.set_cursor(Position::new(0, 8));

        let ctrl_alt_down = KeyEvent::new(
            KeyCode::Down,
            Modifiers {
                shift: false,
                ctrl: true,
                alt: true,
                meta: false,
                alt_graph: false,
            },
        );

        // Down records sticky column 8; add-below clones it to (2, 8).
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        editor.handle_key(&ctrl_alt_down);
        let heads: Vec<Position> = editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.head)
            .collect();
        assert_eq!(heads, vec![Position::new(1, 2), Position::new(2, 8)]);

        // A content edit at both cursors, then undo it.
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('z')));
        assert!(editor.undo());
        let heads: Vec<Position> = editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.head)
            .collect();
        assert_eq!(heads, vec![Position::new(1, 2), Position::new(2, 8)]);

        // Add below again: the (2, 8) cursor's sticky column survived the undo,
        // so its clone lands at (3, 8) — not the clamped (3, 2).
        editor.handle_key(&ctrl_alt_down);
        let heads: Vec<Position> = editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.head)
            .collect();
        assert_eq!(
            heads,
            vec![
                Position::new(1, 2),
                Position::new(2, 8),
                Position::new(3, 8)
            ]
        );
    }

    #[test]
    fn mouse_reconstruction_invalidates_addition_order_stack() {
        // Finding 2: build a three-cursor block with add-above then add-below
        // (stack = [top, bottom]). Mouse clicks then rebuild the exact same
        // three-cursor state. Because the mouse path invalidates the
        // addition-order stack, Ctrl+U no-ops instead of removing the wrong
        // (stale-stack) cursor.
        use crate::input::{MouseButton, MouseEvent};
        use crate::render::Viewport;

        let mut editor = Editor::with_defaults();
        editor.set_content("aa\nbb\ncc");
        // Known geometry: line_height 20, so line L spans y in [20L, 20L+20);
        // gutter 48, char width 12, so column 1 is x = 48 + 12 + 6 = 66.
        editor.state_mut().viewport = Viewport::new(800.0, 600.0, 20.0);

        editor.set_cursor(Position::new(1, 1));
        let ctrl_alt_up = KeyEvent::new(
            KeyCode::Up,
            Modifiers {
                shift: false,
                ctrl: true,
                alt: true,
                meta: false,
                alt_graph: false,
            },
        );
        let ctrl_alt_down = KeyEvent::new(
            KeyCode::Down,
            Modifiers {
                shift: false,
                ctrl: true,
                alt: true,
                meta: false,
                alt_graph: false,
            },
        );
        editor.handle_key(&ctrl_alt_up); // add (0,1)
        editor.handle_key(&ctrl_alt_down); // add (2,1)
        assert_eq!(editor.state().cursor.cursor_count(), 3);

        // Plain-click the middle line (collapse), then ctrl-click bottom and
        // top, reconstructing the same three-cursor state.
        editor.handle_mouse(&MouseEvent::press(MouseButton::Left, 66.0, 30.0));
        editor.handle_mouse(&MouseEvent::release(MouseButton::Left, 66.0, 30.0));
        editor.handle_mouse(&MouseEvent::press(MouseButton::Left, 66.0, 50.0).with_ctrl());
        editor.handle_mouse(&MouseEvent::release(MouseButton::Left, 66.0, 50.0));
        editor.handle_mouse(&MouseEvent::press(MouseButton::Left, 66.0, 10.0).with_ctrl());
        editor.handle_mouse(&MouseEvent::release(MouseButton::Left, 66.0, 10.0));

        let heads: Vec<Position> = editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.head)
            .collect();
        assert_eq!(
            heads,
            vec![
                Position::new(1, 1),
                Position::new(0, 1),
                Position::new(2, 1)
            ],
            "mouse clicks must reconstruct the exact three-cursor state"
        );

        // Ctrl+U must be a no-op: the stack was invalidated by the mouse path.
        editor.handle_key(&KeyEvent::new(KeyCode::Char('u'), Modifiers::ctrl()));
        assert_eq!(
            editor.state().cursor.cursor_count(),
            3,
            "Ctrl+U must not remove a cursor from an invalidated stack"
        );
    }

    /// Builds the three-cursor block ([top, bottom] addition-order stack) used
    /// by the Finding 1 reconstruction tests.
    fn three_cursor_block() -> Editor {
        let mut editor = Editor::with_defaults();
        editor.set_content("aa\nbb\ncc");
        editor.set_cursor(Position::new(1, 1));

        let ctrl_alt = Modifiers {
            shift: false,
            ctrl: true,
            alt: true,
            meta: false,
            alt_graph: false,
        };
        editor.handle_key(&KeyEvent::new(KeyCode::Up, ctrl_alt)); // add (0,1)
        editor.handle_key(&KeyEvent::new(KeyCode::Down, ctrl_alt)); // add (2,1)
        assert_eq!(editor.state().cursor.cursor_count(), 3);
        editor
    }

    #[test]
    fn public_apply_command_reconstruction_invalidates_addition_order_stack() {
        // Finding 1 (public command surface): a host reconstructs the exact
        // three-cursor state through two public SetSelection commands. The
        // rebuilt state is byte-identical and the document revision is
        // unchanged, so value equality alone would let the stale [top, bottom]
        // stack survive and Ctrl+U would pop the wrong cursor. Public
        // apply_command must invalidate the stack.
        let mut editor = three_cursor_block();
        let three = editor.state().cursor.clone();

        editor.apply_command(Command::SetSelection {
            old_state: three.clone(),
            new_state: CursorState::at(Position::new(1, 1)),
        });
        editor.apply_command(Command::SetSelection {
            old_state: CursorState::at(Position::new(1, 1)),
            new_state: three.clone(),
        });
        assert_eq!(editor.state().cursor, three);

        editor.handle_key(&KeyEvent::new(KeyCode::Char('u'), Modifiers::ctrl()));
        assert_eq!(
            editor.state().cursor.cursor_count(),
            3,
            "Ctrl+U must not pop from a stack invalidated by public apply_command"
        );
    }

    #[test]
    fn state_mut_reconstruction_invalidates_addition_order_stack() {
        // Finding 1 (raw state_mut surface): the same reconstruction performed
        // directly through state_mut().cursor must also invalidate the stack.
        let mut editor = three_cursor_block();
        let three = editor.state().cursor.clone();

        editor.state_mut().cursor = CursorState::at(Position::new(1, 1));
        editor.state_mut().cursor = three.clone();
        assert_eq!(editor.state().cursor, three);

        editor.handle_key(&KeyEvent::new(KeyCode::Char('u'), Modifiers::ctrl()));
        assert_eq!(
            editor.state().cursor.cursor_count(),
            3,
            "Ctrl+U must not pop from a stack invalidated by state_mut"
        );
    }

    #[test]
    fn horizontal_round_trip_before_edit_is_not_resurrected_by_undo() {
        // Finding 2: a horizontal round trip invalidates the sticky column
        // *before* a content edit; undoing that edit must NOT revive it.
        let mut editor = Editor::with_defaults();
        // Lines of length 10 / 2 / 10.
        editor.set_content("aaaaaaaaaa\nbb\ncccccccccc");

        editor.set_cursor(Position::new(0, 8));
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(1, 2)); // records sticky column 8

        // Left-then-Right returns the caret to (1,2) but permanently discards
        // the sticky column: a selection-only round trip is never undoable, so
        // no later undo may revive it.
        editor.handle_key(&KeyEvent::simple(KeyCode::Left));
        editor.handle_key(&KeyEvent::simple(KeyCode::Right));
        assert_eq!(editor.cursor(), Position::new(1, 2));

        // Type one character, then undo it. Undo restores (1,2) but must not
        // resurrect the sticky column (8) invalidated before the edit.
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.cursor(), Position::new(1, 3));
        assert!(editor.undo());
        assert_eq!(editor.cursor(), Position::new(1, 2));

        // The next Down re-seeds from the live column (2) → (2,2), not (2,8).
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(2, 2));
    }

    #[test]
    fn ctrl_click_at_selection_end_does_not_duplicate_typed_text() {
        // Finding 5 (reachable via Ctrl-click): a Ctrl-click at the exact end of
        // an existing selection lands a collapsed caret touching the endpoint.
        // It must merge into the selection so a single keystroke replaces it
        // once ("xdef"), rather than also inserting at the boundary ("xxdef").
        use crate::input::{MouseButton, MouseEvent};
        use crate::render::Viewport;

        let mut editor = Editor::with_defaults();
        editor.set_content("abcdef");
        // Geometry: line_height 20, gutter 48, char width 12, so column c is
        // x = 48 + c*12 + 6; column 3 -> 90, line 0 center y = 10.
        editor.state_mut().viewport = Viewport::new(800.0, 600.0, 20.0);

        editor.set_selection(Position::new(0, 0), Position::new(0, 3));
        editor.handle_mouse(&MouseEvent::press(MouseButton::Left, 90.0, 10.0).with_ctrl());
        editor.handle_mouse(&MouseEvent::release(MouseButton::Left, 90.0, 10.0));
        assert_eq!(
            editor.state().cursor.cursor_count(),
            1,
            "a caret touching the selection endpoint must merge into it"
        );

        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "xdef");
    }

    #[test]
    fn ctrl_click_at_selection_start_does_not_duplicate_typed_text() {
        // Finding 5 (both ends): the symmetric Ctrl-click at the selection start
        // must also merge.
        use crate::input::{MouseButton, MouseEvent};
        use crate::render::Viewport;

        let mut editor = Editor::with_defaults();
        editor.set_content("abcdef");
        editor.state_mut().viewport = Viewport::new(800.0, 600.0, 20.0);

        // Select columns 2..5 ("cde"); Ctrl-click at the start (column 2 -> x=78).
        editor.set_selection(Position::new(0, 2), Position::new(0, 5));
        editor.handle_mouse(&MouseEvent::press(MouseButton::Left, 78.0, 10.0).with_ctrl());
        editor.handle_mouse(&MouseEvent::release(MouseButton::Left, 78.0, 10.0));
        assert_eq!(editor.state().cursor.cursor_count(), 1);

        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "abxf");
    }
}
