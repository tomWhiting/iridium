//! Prepare a route before changing the active history path.

use super::{Command, UndoNodeId, UndoTree};
use crate::history::NavigationRequest;

struct Route {
    target: UndoNodeId,
    commands: Vec<Command>,
    /// Traversal order matters: descent wins at the common ancestor.
    edges: Vec<(UndoNodeId, UndoNodeId)>,
}

impl UndoTree {
    /// Runs a caller's transactional replay before committing navigation.
    /// The exclusive borrow prevents tree changes between planning and commit.
    /// Failed replay leaves pointer, preferences and grouping time untouched.
    pub(crate) fn navigate<E>(
        &mut self,
        request: NavigationRequest,
        apply: impl FnOnce(&[Command]) -> Result<(), E>,
    ) -> Result<Option<Vec<Command>>, E> {
        let Some(route) = self.route(request) else {
            return Ok(None);
        };
        apply(&route.commands)?;
        Ok(Some(self.commit_route(route)))
    }

    /// Undoes the current node and returns its inverse for the caller to apply.
    /// This low-level API moves history only; `Editor::undo` also applies text
    /// and commits navigation only when the replay succeeds.
    pub fn undo(&mut self) -> Option<Command> {
        let route = self.route(NavigationRequest::Undo)?;
        self.commit_route(route).pop()
    }

    /// Redoes along the preferred branch, or the newest branch if unset.
    /// The caller must apply the returned command to its document.
    pub fn redo(&mut self) -> Option<Command> {
        let route = self.route(NavigationRequest::Redo)?;
        self.commit_route(route).pop()
    }

    /// Enters a child by its creation-order index, making it preferred.
    /// An unknown index leaves the tree unchanged. The caller applies the
    /// returned command; `Editor::redo_branch` provides transactional replay.
    pub fn redo_branch(&mut self, branch_index: usize) -> Option<Command> {
        let route = self.route(NavigationRequest::Branch(branch_index))?;
        self.commit_route(route).pop()
    }

    /// Moves to a node and returns the ordered undo/redo commands to apply.
    /// Unknown nodes are refused. Every traversed edge becomes preferred;
    /// downward traversal wins at the common ancestor. Use the editor-level
    /// API when navigation must also transact with document changes.
    pub fn jump_to_node(&mut self, target_id: UndoNodeId) -> Option<Vec<Command>> {
        let route = self.route(NavigationRequest::Jump(target_id))?;
        Some(self.commit_route(route))
    }

    fn route(&self, request: NavigationRequest) -> Option<Route> {
        let current = self.nodes.get(&self.current)?;
        match request {
            NavigationRequest::Undo => {
                let parent = current.parent?;
                self.nodes.get(&parent)?;
                Some(Route {
                    target: parent,
                    commands: vec![current.command.as_ref()?.inverse()],
                    edges: vec![(parent, self.current)],
                })
            },
            NavigationRequest::Redo => {
                let child = current
                    .preferred_child
                    .filter(|id| current.children.contains(id))
                    .or_else(|| current.children.last().copied())?;
                self.route_to_child(child)
            },
            NavigationRequest::Branch(index) => self.route_to_child(*current.children.get(index)?),
            NavigationRequest::Jump(target) => self.route_to_node(target),
        }
    }

    fn route_to_child(&self, child: UndoNodeId) -> Option<Route> {
        Some(Route {
            target: child,
            commands: vec![self.nodes.get(&child)?.command.clone()?],
            edges: vec![(self.current, child)],
        })
    }

    fn route_to_node(&self, target: UndoNodeId) -> Option<Route> {
        self.nodes.get(&target)?;
        let from = self.path_to_root(self.current);
        let to = self.path_to_root(target);
        let ancestors: std::collections::HashSet<_> = from.iter().copied().collect();
        let common = to.iter().find(|id| ancestors.contains(id)).copied()?;
        let mut route = Route {
            target,
            commands: Vec::new(),
            edges: Vec::new(),
        };
        for id in from.into_iter().take_while(|id| *id != common) {
            let node = self.nodes.get(&id)?;
            route.commands.push(node.command.as_ref()?.inverse());
            route.edges.push((node.parent?, id));
        }
        let descent: Vec<_> = to.into_iter().take_while(|id| *id != common).collect();
        let mut parent = common;
        for id in descent.into_iter().rev() {
            let node = self.nodes.get(&id)?;
            route.commands.push(node.command.clone()?);
            route.edges.push((parent, id));
            parent = id;
        }
        Some(route)
    }

    fn commit_route(&mut self, route: Route) -> Vec<Command> {
        // A jump to the current node must not interrupt typing's undo group.
        if !route.edges.is_empty() {
            for (parent, child) in route.edges {
                // Every parent was resolved during planning, and the exclusive
                // borrow in navigate prevents removal before this commit.
                if let Some(node) = self.nodes.get_mut(&parent) {
                    node.preferred_child = Some(child);
                }
            }
            self.current = route.target;
            self.finish_group();
        }
        route.commands
    }
}
