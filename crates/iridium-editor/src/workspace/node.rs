//! A place in the organisation: a group, or a tab onto a document.

use super::{DocumentId, NodeId};

/// A place in the organisation: a group of other places, or a tab showing
/// one document.
///
/// Groups nest without limit, and a group's children may be groups and tabs
/// in any mixture — so "a group of tabs inside a group" needs no special
/// case in the model or in any consumer of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A named container. Its children are drawn in the order given.
    Group {
        /// What the group is called, as shown in a sidebar.
        name: String,
        /// The places inside it, in display order.
        children: Vec<NodeId>,
    },
    /// A view onto one document.
    Tab {
        /// The buffer this tab shows.
        document: DocumentId,
        /// What the tab is labelled, usually a file name.
        title: String,
    },
}

impl Node {
    /// Whether this is a group.
    #[must_use]
    pub const fn is_group(&self) -> bool {
        matches!(self, Self::Group { .. })
    }

    /// The document this node shows, if it is a tab.
    #[must_use]
    pub const fn document(&self) -> Option<DocumentId> {
        match self {
            Self::Tab { document, .. } => Some(*document),
            Self::Group { .. } => None,
        }
    }

    /// The node's display label — a group's name or a tab's title.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Group { name, .. } => name,
            Self::Tab { title, .. } => title,
        }
    }

    /// The children of a group, or an empty slice for a tab.
    #[must_use]
    pub fn children(&self) -> &[NodeId] {
        match self {
            Self::Group { children, .. } => children,
            Self::Tab { .. } => &[],
        }
    }
}
