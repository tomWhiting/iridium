//! The workspace itself: what is open, how it is organised, and which tab
//! is active.

use core::fmt;
use std::collections::{HashMap, HashSet};

use super::{DocumentId, Node, NodeId};
use crate::editor::{Editor, EditorConfig};
use crate::theme::Theme;

/// Every open document, how they are organised, and which one is active.
///
/// # Synchronised state
///
/// Because each open document owns a whole [`Editor`], the settings that
/// are conceptually *per window* — theme, configuration, viewport size —
/// exist once per document and must be kept in step. That is done in
/// [`set_theme`](Self::set_theme) and [`set_config`](Self::set_config),
/// which apply to every open editor and not merely the active one.
///
/// **The failure this prevents**, stated so the tests can pin it: reading
/// the *active* editor's theme as if it were the workspace's theme agrees
/// with the truth right up until a document is opened after a theme
/// change, at which point the new tab renders in the old colours.
pub struct Workspace {
    /// The buffers, by id. Dropped when the last node referencing one goes.
    documents: HashMap<DocumentId, Editor>,
    /// The organisation, by id.
    nodes: HashMap<NodeId, Node>,
    /// Child to parent, so `parent_of` is O(1) rather than a search over
    /// every group's children on every keyboard movement.
    ///
    /// Roots are absent from this map rather than present with a sentinel;
    /// "has no parent" and "is a root" are the same fact and should have one
    /// representation.
    parents: HashMap<NodeId, NodeId>,
    /// Top-level places, in display order.
    roots: Vec<NodeId>,
    /// The active tab. Only a tab can be active — activating a group is
    /// meaningless, since there would be no document to type into.
    active: Option<NodeId>,
    /// Monotonic; see the module docs on id reuse.
    next_document: u64,
    /// Monotonic; see the module docs on id reuse.
    next_node: u64,
    /// The configuration every editor is created with and kept at.
    config: EditorConfig,
    /// The theme every editor is kept at.
    theme: Theme,
}

impl Workspace {
    /// Creates an empty workspace.
    ///
    /// Empty means no documents and no groups, which is a legitimate state —
    /// an editor started with no file open — and every method below is
    /// defined on it.
    #[must_use]
    pub fn new(config: EditorConfig, theme: Theme) -> Self {
        Self {
            documents: HashMap::new(),
            nodes: HashMap::new(),
            parents: HashMap::new(),
            roots: Vec::new(),
            active: None,
            next_document: 0,
            next_node: 0,
            config,
            theme,
        }
    }

    /// Opens `content` as a new document with a tab under `parent`.
    ///
    /// `parent` of `None` places the tab at the top level. Returns the new
    /// tab's id, or `None` if `parent` does not name a group — a tab cannot
    /// contain anything, so placing a node "inside" one has no meaning and
    /// is refused rather than silently redirected to the root.
    ///
    /// The first document opened becomes active, so a face that opens one
    /// file does not also have to remember to activate it.
    pub fn open(
        &mut self,
        content: &str,
        title: impl Into<String>,
        parent: Option<NodeId>,
    ) -> Option<NodeId> {
        if !self.is_valid_parent(parent) {
            return None;
        }

        let document = self.allocate_document(content);
        Some(self.insert_tab(document, title.into(), parent))
    }

    /// Opens a second tab onto a document that is already open.
    ///
    /// This is the reason [`DocumentId`] and [`NodeId`] are distinct: the
    /// two tabs share one buffer, one undo history and one set of cursors,
    /// so an edit made through either is immediately visible through the
    /// other.
    ///
    /// Returns `None` if `document` is not open or `parent` is not a group.
    pub fn open_existing(
        &mut self,
        document: DocumentId,
        title: impl Into<String>,
        parent: Option<NodeId>,
    ) -> Option<NodeId> {
        if !self.documents.contains_key(&document) || !self.is_valid_parent(parent) {
            return None;
        }
        Some(self.insert_tab(document, title.into(), parent))
    }

    /// Creates an empty group under `parent`.
    ///
    /// Returns `None` if `parent` does not name a group.
    pub fn create_group(
        &mut self,
        name: impl Into<String>,
        parent: Option<NodeId>,
    ) -> Option<NodeId> {
        if !self.is_valid_parent(parent) {
            return None;
        }
        let id = self.allocate_node();
        self.nodes.insert(
            id,
            Node::Group {
                name: name.into(),
                children: Vec::new(),
            },
        );
        self.attach(id, parent, None);
        Some(id)
    }

    /// The node with this id, if it exists.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// The top-level places, in display order.
    #[must_use]
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// The group containing `id`, or `None` if it is a root or unknown.
    #[must_use]
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        self.parents.get(&id).copied()
    }

    /// How many documents are open.
    ///
    /// Documents, not tabs: two tabs onto one file count once, because the
    /// number this answers is "how much is in memory".
    #[must_use]
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// Whether nothing is open and nothing is organised.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The editor for a document, if it is open.
    #[must_use]
    pub fn editor(&self, document: DocumentId) -> Option<&Editor> {
        self.documents.get(&document)
    }

    /// The editor for a document, mutably, if it is open.
    pub fn editor_mut(&mut self, document: DocumentId) -> Option<&mut Editor> {
        self.documents.get_mut(&document)
    }

    /// The active tab, if there is one.
    #[must_use]
    pub const fn active(&self) -> Option<NodeId> {
        self.active
    }

    /// The document shown by the active tab, if there is one.
    #[must_use]
    pub fn active_document(&self) -> Option<DocumentId> {
        self.active
            .and_then(|id| self.nodes.get(&id))
            .and_then(Node::document)
    }

    /// The editor the user is typing into, if any.
    #[must_use]
    pub fn active_editor(&self) -> Option<&Editor> {
        self.active_document()
            .and_then(|document| self.documents.get(&document))
    }

    /// The editor the user is typing into, mutably, if any.
    pub fn active_editor_mut(&mut self) -> Option<&mut Editor> {
        let document = self.active_document()?;
        self.documents.get_mut(&document)
    }

    /// Makes `id` the active tab, returning whether it could be.
    ///
    /// Refuses a group and an unknown id, leaving the previous activation
    /// alone in both cases: a click that raced a close should not also lose
    /// the user's place.
    pub fn activate(&mut self, id: NodeId) -> bool {
        if self.nodes.get(&id).is_some_and(|node| !node.is_group()) {
            self.active = Some(id);
            return true;
        }
        false
    }

    /// Renames a group or retitles a tab, returning whether it existed.
    pub fn rename(&mut self, id: NodeId, label: impl Into<String>) -> bool {
        match self.nodes.get_mut(&id) {
            Some(Node::Group { name, .. }) => {
                *name = label.into();
                true
            },
            Some(Node::Tab { title, .. }) => {
                *title = label.into();
                true
            },
            None => false,
        }
    }

    /// Removes `id` and everything under it.
    ///
    /// Returns whether it existed. A document is dropped only once no
    /// remaining tab shows it, so closing one of two tabs onto a file keeps
    /// the buffer, its cursors and its undo history alive.
    ///
    /// If the active tab was inside what was removed, activation moves to a
    /// neighbour by the rule in [`neighbour_of`](Self::neighbour_of) —
    /// chosen before the removal, while the old position is still known.
    pub fn close(&mut self, id: NodeId) -> bool {
        if !self.nodes.contains_key(&id) {
            return false;
        }

        let removed = self.subtree_of(id);
        let active_was_removed = self.active.is_some_and(|active| removed.contains(&active));
        // Resolved first: after the detach there is no position to search
        // from, and falling back to "whatever is first" would move the
        // user's place across the workspace instead of one tab over.
        let replacement = if active_was_removed {
            self.neighbour_of(id)
        } else {
            self.active
        };

        self.detach(id);
        for node in &removed {
            self.nodes.remove(node);
            self.parents.remove(node);
        }
        self.drop_unreferenced_documents();

        self.active = replacement.filter(|node| self.nodes.contains_key(node));
        true
    }

    /// Moves `id` to be a child of `parent`, at `index` among its siblings.
    ///
    /// `index` is clamped, so appending is `usize::MAX` and prepending is
    /// `0`. Returns whether the move happened.
    ///
    /// **Refuses to move a node into its own subtree.** That would build a
    /// cycle, and a cycle in this structure is not a visible glitch — it is
    /// a traversal that never terminates, in a projection that runs on every
    /// frame. Refusing is the only safe answer; there is no partial move
    /// that would be better than none.
    pub fn move_node(&mut self, id: NodeId, parent: Option<NodeId>, index: usize) -> bool {
        if !self.nodes.contains_key(&id) || !self.is_valid_parent(parent) {
            return false;
        }
        if let Some(parent) = parent {
            if parent == id || self.subtree_of(id).contains(&parent) {
                return false;
            }
        }

        self.detach(id);
        self.attach(id, parent, Some(index));
        true
    }

    /// The theme every open editor is using.
    #[must_use]
    pub const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Sets the theme on the workspace **and on every open editor**.
    ///
    /// Applying it to only the active editor would leave every background
    /// tab on the old theme, and a document opened afterwards would inherit
    /// the workspace's — so the two would disagree in opposite directions
    /// depending on which tab you looked at.
    pub fn set_theme(&mut self, theme: Theme) {
        for editor in self.documents.values_mut() {
            editor.set_theme(theme.clone());
        }
        self.theme = theme;
    }

    /// The configuration every open editor is using.
    #[must_use]
    pub const fn config(&self) -> &EditorConfig {
        &self.config
    }

    /// Sets the configuration on the workspace **and on every open editor**,
    /// for the reason given on [`set_theme`](Self::set_theme).
    pub fn set_config(&mut self, config: EditorConfig) {
        for editor in self.documents.values_mut() {
            editor.set_config(config.clone());
        }
        self.config = config;
    }

    /// Whether `parent` may receive a child: absent, or a known group.
    fn is_valid_parent(&self, parent: Option<NodeId>) -> bool {
        parent.is_none_or(|id| self.nodes.get(&id).is_some_and(Node::is_group))
    }

    /// Allocates the next document id and its editor.
    fn allocate_document(&mut self, content: &str) -> DocumentId {
        let id = DocumentId(self.next_document);
        self.next_document = self.next_document.saturating_add(1);

        let mut editor = Editor::new(self.config.clone());
        editor.set_theme(self.theme.clone());
        editor.set_content(content);
        self.documents.insert(id, editor);
        id
    }

    /// Allocates the next node id.
    const fn allocate_node(&mut self) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node = self.next_node.saturating_add(1);
        id
    }

    /// Creates a tab node, attaches it, and activates it if nothing else is.
    ///
    /// Infallible by the time it is reached: both callers have already
    /// checked the parent, which is the only thing that can refuse a tab.
    fn insert_tab(
        &mut self,
        document: DocumentId,
        title: String,
        parent: Option<NodeId>,
    ) -> NodeId {
        let id = self.allocate_node();
        self.nodes.insert(id, Node::Tab { document, title });
        self.attach(id, parent, None);
        if self.active.is_none() {
            self.active = Some(id);
        }
        id
    }

    /// Places `id` under `parent` at `index`, clamped, appending if `None`.
    fn attach(&mut self, id: NodeId, parent: Option<NodeId>, index: Option<usize>) {
        let siblings = match parent {
            Some(parent) => match self.nodes.get_mut(&parent) {
                Some(Node::Group { children, .. }) => children,
                // Unreachable through the public API — every caller checks
                // `is_valid_parent` first — but written as a return rather
                // than an assumption, because a future caller that forgets
                // should lose one attachment, not corrupt the tree.
                Some(Node::Tab { .. }) | None => return,
            },
            None => &mut self.roots,
        };
        let at = index.unwrap_or(usize::MAX).min(siblings.len());
        siblings.insert(at, id);

        match parent {
            Some(parent) => {
                self.parents.insert(id, parent);
            },
            None => {
                self.parents.remove(&id);
            },
        }
    }

    /// Removes `id` from its parent's child list, leaving the node itself.
    fn detach(&mut self, id: NodeId) {
        let siblings = match self.parents.get(&id).copied() {
            Some(parent) => match self.nodes.get_mut(&parent) {
                Some(Node::Group { children, .. }) => children,
                Some(Node::Tab { .. }) | None => return,
            },
            None => &mut self.roots,
        };
        siblings.retain(|child| *child != id);
        self.parents.remove(&id);
    }

    /// `id` and every node beneath it.
    ///
    /// `pub(super)` because `nav` needs it to choose what to activate when a
    /// subtree closes; not public, because a caller outside this module has
    /// no business enumerating the organisation node by node.
    ///
    /// Iterative rather than recursive: depth here is the user's to choose,
    /// and a workspace nested deeply enough to overflow the stack is exactly
    /// the input a model that promises not to panic has to survive.
    pub(super) fn subtree_of(&self, id: NodeId) -> HashSet<NodeId> {
        let mut seen = HashSet::new();
        let mut stack = vec![id];
        while let Some(node) = stack.pop() {
            if !seen.insert(node) {
                // Defensive against a cycle rather than a claim one can
                // exist: `move_node` refuses to build one. Without this the
                // failure mode would be a hang, which is far worse to
                // diagnose than a subtree that stops early.
                continue;
            }
            if let Some(children) = self.nodes.get(&node).map(Node::children) {
                stack.extend(children.iter().copied());
            }
        }
        seen
    }

    /// Drops every document no remaining tab refers to.
    fn drop_unreferenced_documents(&mut self) {
        let referenced: HashSet<DocumentId> =
            self.nodes.values().filter_map(Node::document).collect();
        self.documents
            .retain(|document, _| referenced.contains(document));
    }
}

impl fmt::Debug for Workspace {
    /// Written by hand because [`Editor`] is not `Debug` — and it should not
    /// become so for this: an editor's debug output would be the whole
    /// document, which is exactly what nobody wants printed. The documents
    /// appear here as their ids and count.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut documents: Vec<DocumentId> = self.documents.keys().copied().collect();
        documents.sort_unstable();
        formatter
            .debug_struct("Workspace")
            .field("documents", &documents)
            .field("nodes", &self.nodes)
            .field("roots", &self.roots)
            .field("active", &self.active)
            .field("theme", &self.theme.name)
            .finish_non_exhaustive()
    }
}
