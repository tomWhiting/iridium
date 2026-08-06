//! Running the `workspace.*` commands by id.
//!
//! This is the half that keeps the faces honest. A face receives
//! [`KeyResult::HostCommand`](crate::input::KeyResult::HostCommand) naming
//! a `workspace.*` id and forwards it here; it does not decide what "next
//! tab" means. Were each face to decide, the terminal and the browser would
//! disagree about what "next" is across a nested group — and would disagree
//! silently, since both answers look right in a workspace with no groups.

use crate::commands::CommandId;
use crate::commands::builtin::{
    WORKSPACE_CLOSE_TAB, WORKSPACE_FIRST_TAB, WORKSPACE_LAST_TAB, WORKSPACE_NEXT_TAB,
    WORKSPACE_PREVIOUS_TAB,
};

use super::Workspace;

/// Why a workspace command did not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceCommandError {
    /// The id is not a `workspace.*` command.
    ///
    /// Reported rather than ignored: a face that forwards an unknown id has
    /// a routing bug, and silently returning "nothing happened" would make
    /// it indistinguishable from a command that correctly declined.
    NotAWorkspaceCommand,
}

impl core::fmt::Display for WorkspaceCommandError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAWorkspaceCommand => formatter.write_str("not a workspace command"),
        }
    }
}

impl core::error::Error for WorkspaceCommandError {}

impl<T> Workspace<T> {
    /// Runs a `workspace.*` command, reporting whether it changed anything.
    ///
    /// `Ok(true)` means the workspace changed — the face should redraw.
    /// `Ok(false)` means the command was understood and correctly did
    /// nothing: next-tab at the last tab, close with nothing open. That is
    /// **not** an error, and treating it as one is how a face ends up
    /// flashing an error at a user who simply reached the end of the strip.
    ///
    /// # Errors
    ///
    /// [`WorkspaceCommandError::NotAWorkspaceCommand`] if `id` is not one of
    /// the ids in
    /// [`WORKSPACE`](crate::commands::builtin::WORKSPACE).
    pub fn run_command(&mut self, id: &CommandId) -> Result<bool, WorkspaceCommandError> {
        if *id == WORKSPACE_NEXT_TAB {
            return Ok(self.next_tab());
        }
        if *id == WORKSPACE_PREVIOUS_TAB {
            return Ok(self.previous_tab());
        }
        if *id == WORKSPACE_FIRST_TAB {
            return Ok(self.first_tab());
        }
        if *id == WORKSPACE_LAST_TAB {
            return Ok(self.last_tab());
        }
        if *id == WORKSPACE_CLOSE_TAB {
            return Ok(self.active().is_some_and(|tab| self.close(tab)));
        }
        Err(WorkspaceCommandError::NotAWorkspaceCommand)
    }

    /// Whether `id` names a command this workspace runs.
    ///
    /// For a face deciding where to route a
    /// [`KeyResult::HostCommand`](crate::input::KeyResult::HostCommand):
    /// here, or to its own palette-and-panel handling.
    #[must_use]
    pub fn handles_command(&self, id: &CommandId) -> bool {
        crate::commands::builtin::WORKSPACE
            .iter()
            .any(|meta| *meta.id() == *id)
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::{Workspace, WorkspaceCommandError};
    use crate::commands::builtin::{
        PALETTE_OPEN, WORKSPACE, WORKSPACE_CLOSE_TAB, WORKSPACE_FIRST_TAB, WORKSPACE_LAST_TAB,
        WORKSPACE_NEXT_TAB, WORKSPACE_PREVIOUS_TAB,
    };
    use crate::editor::EditorConfig;
    use crate::theme::Theme;
    use crate::workspace::workspace_tests::active_label;

    fn two_tabs() -> Workspace {
        let mut workspace: Workspace = Workspace::new(EditorConfig::default(), Theme::default());
        workspace.open("a", "a.rs", None).expect("top level");
        workspace.open("b", "b.rs", None).expect("top level");
        workspace
    }

    #[test]
    fn every_declared_workspace_command_is_dispatched() {
        // The guard against a table entry that nobody wired up: adding a
        // `CommandMeta` without an arm would leave a command bindable,
        // searchable in the palette, and inert.
        let mut workspace = two_tabs();
        for meta in WORKSPACE {
            assert!(
                workspace.run_command(meta.id()).is_ok(),
                "{} is declared but not dispatched",
                meta.id()
            );
            assert!(workspace.handles_command(meta.id()));
        }
    }

    #[test]
    fn a_command_from_another_table_is_reported_not_ignored() {
        let mut workspace = two_tabs();

        assert_eq!(
            workspace.run_command(&PALETTE_OPEN),
            Err(WorkspaceCommandError::NotAWorkspaceCommand)
        );
        assert!(!workspace.handles_command(&PALETTE_OPEN));
    }

    #[test]
    fn the_movement_commands_walk_the_strip() {
        let mut workspace = two_tabs();
        assert_eq!(active_label(&workspace), Some("a.rs"));

        assert_eq!(workspace.run_command(&WORKSPACE_NEXT_TAB), Ok(true));
        assert_eq!(active_label(&workspace), Some("b.rs"));
        assert_eq!(workspace.run_command(&WORKSPACE_PREVIOUS_TAB), Ok(true));
        assert_eq!(active_label(&workspace), Some("a.rs"));
        assert_eq!(workspace.run_command(&WORKSPACE_LAST_TAB), Ok(true));
        assert_eq!(active_label(&workspace), Some("b.rs"));
        assert_eq!(workspace.run_command(&WORKSPACE_FIRST_TAB), Ok(true));
        assert_eq!(active_label(&workspace), Some("a.rs"));
    }

    /// Reaching the end is an outcome, not a failure.
    #[test]
    fn a_command_that_correctly_does_nothing_reports_false_not_an_error() {
        let mut workspace = two_tabs();
        workspace.last_tab();

        assert_eq!(
            workspace.run_command(&WORKSPACE_NEXT_TAB),
            Ok(false),
            "a face must be able to tell 'at the end' from 'went wrong'"
        );
    }

    #[test]
    fn closing_runs_on_the_active_tab_and_moves_the_activation() {
        let mut workspace = two_tabs();

        assert_eq!(workspace.run_command(&WORKSPACE_CLOSE_TAB), Ok(true));

        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(active_label(&workspace), Some("b.rs"));
    }

    #[test]
    fn closing_with_nothing_open_reports_false_rather_than_erroring() {
        let mut workspace: Workspace = Workspace::new(EditorConfig::default(), Theme::default());

        assert_eq!(workspace.run_command(&WORKSPACE_CLOSE_TAB), Ok(false));
    }
}
