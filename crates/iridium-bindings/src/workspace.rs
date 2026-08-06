//! The workspace wire shape: tabs, nested groups, and the ids that survive
//! a round trip through a host language.
//!
//! Deliberately **not** gated on `feature = "web"` or `feature = "napi"`.
//! Everything here is plain functions over a borrowed
//! [`Workspace`](iridium_editor::workspace::Workspace), so `cargo test`
//! exercises it on the host target; only the thin `#[wasm_bindgen]` and
//! `#[napi]` adapters are face-specific. A conversion bug that can only be
//! reproduced in a browser is a conversion bug nobody reproduces.
//!
//! # Ids on the wire
//!
//! The kernel has two id spaces — [`NodeId`] names a *place* (a tab or a
//! group) and [`DocumentId`] names a *buffer* — and Rust's type system keeps
//! them apart for free. On the wire that protection is gone: both would
//! become numbers, and because both counters start from the same place,
//! document 3 and node 3 both exist. A host that passed one where the other
//! was expected would not get an error; it would get a real but wrong
//! object, and the tab strip would activate something arbitrary.
//!
//! So each id is **tagged with its space**: `"n:7"` for a node, `"d:3"` for
//! a document. A swapped id then fails to parse instead of resolving, which
//! turns a silent wrong-object bug into a loud one. The tag costs two bytes
//! per id and is the only thing standing between the two spaces once they
//! leave Rust.
//!
//! Ids are **strings, not numbers**, for the same reason
//! [`UndoNodeInfo`](iridium_editor::history) serializes them that way: the
//! counters are `u64` and JavaScript numbers are `f64`, which silently
//! rounds above 2^53. A workspace will not reach that in practice, but
//! "in practice" is not a property the format should depend on.
//!
//! # What is not here
//!
//! There is no `modified` flag on a tab. The kernel does no file I/O, so it
//! cannot know whether a buffer differs from what is on disk — only the face
//! that saved it knows. Each tab therefore carries its document's
//! [`revision`](iridium_editor::Document::revision), which a face compares
//! against the revision it last wrote out. Inventing a kernel-side
//! `modified` bool would mean the kernel guessing, and a dot that is wrong
//! is worse than no dot.
//!
//! That comparison is only sound because the counter advances across a
//! whole-content replacement rather than restarting — writing this module is
//! what surfaced that it did not, and the kernel now carries the counter
//! forward. Without it, reloading a tab would reset the number to a value the
//! *previous* file had already used, and a face would draw "saved" over
//! unsaved text.

use iridium_editor::workspace::{DocumentId, Node, NodeId, Workspace, WorkspaceCommandError};
use serde::{Deserialize, Serialize};

/// The tag prefixing a serialized [`NodeId`].
const NODE_TAG: &str = "n:";

/// The tag prefixing a serialized [`DocumentId`].
const DOCUMENT_TAG: &str = "d:";

/// Serializes a node id as `"n:<counter>"`.
#[must_use]
pub fn encode_node(id: NodeId) -> String {
    format!("{NODE_TAG}{}", id.as_raw())
}

/// Serializes a document id as `"d:<counter>"`.
#[must_use]
pub fn encode_document(id: DocumentId) -> String {
    format!("{DOCUMENT_TAG}{}", id.as_raw())
}

/// Parses a node id, or `None` if `text` is not one.
///
/// A document id is refused here rather than reinterpreted — that is the
/// whole point of the tag. So is a bare number, a negative value, and
/// anything that overflows `u64`.
#[must_use]
pub fn decode_node(text: &str) -> Option<NodeId> {
    text.strip_prefix(NODE_TAG)?
        .parse::<u64>()
        .ok()
        .map(NodeId::from_raw)
}

/// Parses a document id, or `None` if `text` is not one.
///
/// See [`decode_node`]: a node id is refused rather than reinterpreted.
#[must_use]
pub fn decode_document(text: &str) -> Option<DocumentId> {
    text.strip_prefix(DOCUMENT_TAG)?
        .parse::<u64>()
        .ok()
        .map(DocumentId::from_raw)
}

/// What a node is: a tab showing a buffer, or a group holding other nodes.
///
/// A string on the wire rather than a bool, because a third kind — a split,
/// a terminal pane — is plausible, and a `isGroup: false` that has to become
/// `kind: "terminal"` breaks every consumer at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    /// A group: it holds children and cannot be activated.
    Group,
    /// A tab: it shows one document and is what activation lands on.
    Tab,
}

/// One node as a sidebar or tab strip renders it.
///
/// `camelCase` on the wire; the field names are the TypeScript surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceNode {
    /// The tagged node id, and what a host passes back to act on this node.
    pub id: String,
    /// Whether this is a tab or a group.
    pub kind: NodeKind,
    /// The text to draw: a filename for a tab, a group's name for a group.
    pub label: String,
    /// The containing group, or `None` at the top level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// The children in display order — always empty for a tab.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    /// The buffer this tab shows, or `None` for a group.
    ///
    /// Two tabs may carry the *same* document id. That is not a bug to be
    /// deduplicated: it is one file open in two places, sharing a buffer and
    /// an undo history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// How deep this node sits, with a top-level node at `0`.
    ///
    /// Carried rather than left for the host to compute, so that a face
    /// rendering a flat list with indentation does not have to walk parent
    /// links per row.
    pub depth: u32,
    /// Whether this node is the active one.
    pub is_active: bool,
    /// The document's revision counter, or `None` for a group.
    ///
    /// See the [module documentation](self) for why this rather than a
    /// `modified` flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

/// Everything a face needs to draw the workspace, in one payload.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    /// Every node, in depth-first display order.
    ///
    /// Pre-flattened because that is the order both a sidebar and a tab strip
    /// draw in, and because a host reconstructing it from `children` links
    /// would be reimplementing a traversal the kernel already tests.
    pub nodes: Vec<WorkspaceNode>,
    /// The top-level node ids, in display order.
    pub roots: Vec<String>,
    /// The tabs only, in the order movement walks them.
    ///
    /// This is the tab strip. It crosses group boundaries, exactly as
    /// `workspace.nextTab` does, so the strip and the keys agree by
    /// construction rather than by two implementations happening to match.
    pub tab_order: Vec<String>,
    /// The active node, or `None` when nothing is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_id: Option<String>,
    /// The active tab's document, or `None` when nothing is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_document_id: Option<String>,
    /// How many distinct buffers are open.
    ///
    /// Lower than the tab count when a file is open in two places.
    pub document_count: u32,
}

/// Builds the full snapshot.
///
/// One traversal, allocating one [`WorkspaceNode`] per node. A tab strip
/// redraws on activation and on open/close, not per keystroke, so this is
/// nowhere near a hot path — and a partial-update protocol would be a second
/// source of truth about the tree.
#[must_use]
pub fn snapshot(workspace: &Workspace) -> WorkspaceSnapshot {
    let mut nodes = Vec::new();
    let active = workspace.active();
    for root in workspace.roots() {
        push_subtree(workspace, *root, None, 0, active, &mut nodes);
    }

    WorkspaceSnapshot {
        nodes,
        roots: workspace.roots().iter().copied().map(encode_node).collect(),
        tab_order: workspace.tabs().into_iter().map(encode_node).collect(),
        active_id: active.map(encode_node),
        active_document_id: workspace.active_document().map(encode_document),
        document_count: u32::try_from(workspace.document_count()).unwrap_or(u32::MAX),
    }
}

/// Appends `id` and everything beneath it, depth-first, in display order.
///
/// Iterative rather than recursive: nesting is user-controlled, and a
/// deeply nested workspace must not be able to overflow the stack of
/// whichever thread a face happens to serialize on. The explicit stack
/// carries the parent and depth that recursion would have carried in frames.
fn push_subtree(
    workspace: &Workspace,
    id: NodeId,
    parent: Option<NodeId>,
    depth: u32,
    active: Option<NodeId>,
    out: &mut Vec<WorkspaceNode>,
) {
    let mut stack = vec![(id, parent, depth)];
    while let Some((id, parent, depth)) = stack.pop() {
        let Some(node) = workspace.node(id) else {
            // Unreachable through the public API — `roots` and `children`
            // only ever name live nodes. Skipped rather than asserted so
            // that a future bug drops one row instead of taking down the
            // face that was drawing the tab strip.
            continue;
        };

        out.push(WorkspaceNode {
            id: encode_node(id),
            kind: if node.is_group() {
                NodeKind::Group
            } else {
                NodeKind::Tab
            },
            label: node.label().to_owned(),
            parent_id: parent.map(encode_node),
            children: node.children().iter().copied().map(encode_node).collect(),
            document_id: node.document().map(encode_document),
            depth,
            is_active: active == Some(id),
            revision: node
                .document()
                .and_then(|document| workspace.editor(document))
                .map(|editor| editor.state().document.revision()),
        });

        // Reversed, because `pop` takes from the end: without this the
        // children of every group would be emitted right-to-left, which is
        // the kind of wrong that looks right in a group of one.
        for child in node.children().iter().rev() {
            stack.push((*child, Some(id), depth + 1));
        }
    }
}

/// Why an action a host asked for did not happen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum WorkspaceActionError {
    /// The string was not a node id — most often a document id passed by
    /// mistake, which the tag exists to catch.
    MalformedNodeId(String),
    /// The string was not a document id.
    MalformedDocumentId(String),
    /// A well-formed id that names nothing. Stale, not invalid: the node was
    /// closed between the snapshot the host drew and the click it sent.
    UnknownNode(String),
    /// A parent that exists but is a tab. A tab holds text, not nodes, so
    /// putting something "inside" one has no meaning and is refused rather
    /// than quietly redirected to the top level.
    NotAGroup(String),
    /// The kernel declined an operation this boundary had already checked.
    ///
    /// Unreachable unless the two checks disagree — which is exactly why it
    /// is reported rather than folded into a neighbouring variant. A
    /// boundary check is a *proxy* for the kernel's, and the moment they
    /// diverge the honest thing is to say so loudly instead of returning
    /// whichever error looks most plausible.
    Refused(String),
    /// The id was not a `workspace.*` command.
    NotAWorkspaceCommand(String),
}

impl core::fmt::Display for WorkspaceActionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MalformedNodeId(text) => {
                write!(f, "`{text}` is not a node id (expected `n:<number>`)")
            },
            Self::MalformedDocumentId(text) => {
                write!(f, "`{text}` is not a document id (expected `d:<number>`)")
            },
            Self::UnknownNode(text) => write!(f, "no node `{text}` — it may have been closed"),
            Self::NotAGroup(text) => write!(f, "`{text}` is a tab, and a tab cannot hold nodes"),
            Self::Refused(what) => write!(
                f,
                "the kernel refused to {what} after the boundary accepted it — \
                 the two validity checks disagree"
            ),
            Self::NotAWorkspaceCommand(id) => write!(f, "`{id}` is not a workspace command"),
        }
    }
}

impl core::error::Error for WorkspaceActionError {}

/// Resolves a host-supplied node id against the live workspace.
///
/// # Errors
///
/// [`WorkspaceActionError::MalformedNodeId`] when the tag or number is
/// wrong, and [`WorkspaceActionError::UnknownNode`] when the id parses but
/// names nothing. The two are distinguished because they call for different
/// responses: the first is a bug in the host, the second is an ordinary race
/// between a drawn snapshot and a click.
pub fn resolve_node(workspace: &Workspace, id: &str) -> Result<NodeId, WorkspaceActionError> {
    let node =
        decode_node(id).ok_or_else(|| WorkspaceActionError::MalformedNodeId(id.to_owned()))?;
    if workspace.node(node).is_none() {
        return Err(WorkspaceActionError::UnknownNode(id.to_owned()));
    }
    Ok(node)
}

/// Resolves an optional parent id, requiring that it names a *group*.
///
/// `None` maps to `Ok(None)` — the top level, which is always a valid place
/// to put something.
///
/// # Errors
///
/// See [`resolve_node`], plus [`WorkspaceActionError::NotAGroup`] when the
/// id names a tab. Checked here rather than left to the kernel's bare
/// `None` so that a host is told *which* of the three things went wrong.
fn resolve_parent(
    workspace: &Workspace,
    parent: Option<&str>,
) -> Result<Option<NodeId>, WorkspaceActionError> {
    let Some(parent) = parent else {
        return Ok(None);
    };
    let id = resolve_node(workspace, parent)?;
    if workspace.node(id).is_some_and(Node::is_group) {
        Ok(Some(id))
    } else {
        Err(WorkspaceActionError::NotAGroup(parent.to_owned()))
    }
}

/// Activates a tab by its wire id, reporting whether the activation **moved**.
///
/// `Ok(false)` means the tab was already active, or the id named a group —
/// groups are not activatable. Both are ordinary outcomes of a click, not
/// errors.
///
/// Note the deliberate difference from
/// [`Workspace::activate`](iridium_editor::workspace::Workspace::activate),
/// which answers "could it be activated" and so returns `true` for a tab that
/// was already active. That is the right question inside the kernel, where
/// an unknown id and a group are both worth distinguishing from success. It
/// is the wrong question at a boundary, where the caller has already been
/// told about unknown ids through [`Err`] and the one thing left to decide is
/// whether to redraw. Every other action here reports movement; an
/// `activate` that reported reachability instead would hand a face two
/// meanings for the same-shaped `bool` and get believed either way.
///
/// # Errors
///
/// See [`resolve_node`].
pub fn activate(workspace: &mut Workspace, id: &str) -> Result<bool, WorkspaceActionError> {
    let node = resolve_node(workspace, id)?;
    if workspace.active() == Some(node) {
        return Ok(false);
    }
    Ok(workspace.activate(node))
}

/// Closes a node — a tab, or a group and everything in it.
///
/// # Errors
///
/// See [`resolve_node`].
pub fn close(workspace: &mut Workspace, id: &str) -> Result<bool, WorkspaceActionError> {
    let node = resolve_node(workspace, id)?;
    Ok(workspace.close(node))
}

/// Renames a tab or a group.
///
/// # Errors
///
/// See [`resolve_node`].
pub fn rename(
    workspace: &mut Workspace,
    id: &str,
    label: &str,
) -> Result<bool, WorkspaceActionError> {
    let node = resolve_node(workspace, id)?;
    Ok(workspace.rename(node, label))
}

/// Reparents a node, placing it at `index` among its new siblings.
///
/// This is the drag-and-drop verb. `parent` is `None` for the top level.
/// An index past the end appends rather than failing, because a drop below
/// the last row is a drop at the end, not a mistake.
///
/// # Errors
///
/// See [`resolve_node`] for `id` and [`resolve_parent`] for `parent`.
pub fn move_node(
    workspace: &mut Workspace,
    id: &str,
    parent: Option<&str>,
    index: u32,
) -> Result<bool, WorkspaceActionError> {
    let node = resolve_node(workspace, id)?;
    let parent = resolve_parent(workspace, parent)?;
    // Saturating rather than truncating: `usize` is at least 32 bits on
    // every supported target, so this cannot actually saturate — but a
    // truncating cast would turn a large index into a small one and drop
    // the node in the wrong place, which is the failure worth ruling out
    // by construction rather than by knowing the target.
    let index = usize::try_from(index).unwrap_or(usize::MAX);
    Ok(workspace.move_node(node, parent, index))
}

/// Creates a group, returning its wire id.
///
/// `parent` is `None` for a top-level group.
///
/// # Errors
///
/// See [`resolve_parent`], and
/// [`WorkspaceActionError::Refused`] in the disagreement case documented
/// there.
pub fn create_group(
    workspace: &mut Workspace,
    name: &str,
    parent: Option<&str>,
) -> Result<String, WorkspaceActionError> {
    let parent_id = resolve_parent(workspace, parent)?;
    workspace
        .create_group(name, parent_id)
        .map(encode_node)
        .ok_or_else(|| WorkspaceActionError::Refused("create a group".to_owned()))
}

/// Opens `content` as a new tab, returning its wire id.
///
/// # Errors
///
/// See [`create_group`].
pub fn open(
    workspace: &mut Workspace,
    content: &str,
    title: &str,
    parent: Option<&str>,
) -> Result<String, WorkspaceActionError> {
    let parent_id = resolve_parent(workspace, parent)?;
    workspace
        .open(content, title, parent_id)
        .map(encode_node)
        .ok_or_else(|| WorkspaceActionError::Refused("open a tab".to_owned()))
}

/// Runs a `workspace.*` command by id, reporting whether anything changed.
///
/// This is the routing target for a
/// [`KeyResult::HostCommand`](iridium_editor::KeyResult) the face does not
/// implement itself. `Ok(false)` is "understood, correctly did nothing" —
/// next-tab at the last tab — and a face that treats it as a failure flashes
/// a warning at someone who simply reached the end of the strip.
///
/// # Errors
///
/// [`WorkspaceActionError::NotAWorkspaceCommand`] when the id belongs to
/// another table. Reported rather than ignored: a face forwarding an unknown
/// id has a routing bug, and a silent `false` would make it look identical
/// to a command that correctly declined.
pub fn run_command(workspace: &mut Workspace, id: &str) -> Result<bool, WorkspaceActionError> {
    let command = iridium_editor::CommandId::new(id.to_owned());
    workspace
        .run_command(&command)
        .map_err(|error| match error {
            WorkspaceCommandError::NotAWorkspaceCommand => {
                WorkspaceActionError::NotAWorkspaceCommand(id.to_owned())
            },
        })
}

/// Whether `id` names a command the workspace runs.
///
/// For a face deciding where to route a host command: here, or to its own
/// palette-and-panel handling.
#[must_use]
pub fn handles_command(workspace: &Workspace, id: &str) -> bool {
    workspace.handles_command(&iridium_editor::CommandId::new(id.to_owned()))
}

/// The label a node draws with, without building a whole snapshot.
///
/// A convenience for a face that has one id and wants one string — a window
/// title, say — rather than the tree.
#[must_use]
pub fn label_of(workspace: &Workspace, id: &str) -> Option<String> {
    let node = decode_node(id)?;
    workspace.node(node).map(Node::label).map(str::to_owned)
}

#[cfg(test)]
mod tests;
