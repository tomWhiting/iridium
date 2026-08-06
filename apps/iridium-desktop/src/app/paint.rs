//! Composing and presenting one frame.
//!
//! Two passes on one texture view: the document through the compositor, then
//! the overlays. The panels are composed here, against the window's current
//! grid, so every frame shows the selection window and the match counts as
//! they are *now* — the undo tree's `*` moves the frame after a jump.

use std::io::{self, Write as _};
use std::time::Instant;

use iridium_editor::Editor;
use iridium_editor::render::FrameTarget;

use super::startup::Shell;
use super::state::DesktopApp;
use crate::overlay::{PanelContent, StripContent};

impl DesktopApp {
    /// Composes and presents one frame: the document through the compositor,
    /// then — when a prompt, a message or any panel is up — the overlays as a
    /// second pass on the same texture view. See [`crate::overlay`] for the
    /// pass structure.
    ///
    /// The panels are composed here, against the window's current grid, so
    /// every frame shows the selection window and the match counts as they
    /// are *now* — the undo tree's `*` moves the frame after a jump.
    ///
    /// A failed frame is reported on standard error and the session carries
    /// on — the web face does the same. The next event requests the next
    /// attempt; a fault that persists keeps naming itself rather than
    /// silently freezing the window.
    pub(super) fn redraw(&mut self) {
        // Costs a generation comparison when nothing changed; the spans are
        // rebuilt — over the frame's viewport plus overscan, never the whole
        // document — only when the kernel actually reparsed or the viewport
        // escaped the covered window.
        let window = self.viewport_window();
        if let Some((editor, document)) = self.workspace.active_editor_and_payload_mut() {
            match window {
                Some(window) => document.syntax.refresh_windowed(editor, window),
                None => document.syntax.refresh(editor),
            }
        }
        let tabs = self.tab_strip_content();
        let strip = self.strip_content();
        let panels = self.panel_contents();
        let Some((editor, document)) = self.workspace.active_editor_and_payload_mut() else {
            return;
        };
        let editor: &Editor = editor;
        let scroll_y = document.scroll_y;
        let Some(shell) = &mut self.shell else {
            return;
        };
        let Shell {
            surface,
            compositor,
            overlay,
            ..
        } = shell;

        let width = surface.width();
        let height = surface.height();
        let fold_state = editor.fold_state();
        let theme = &editor.state().theme;
        let mut highlights = document.syntax.resolver(&theme.syntax);
        let panel_refs: Vec<&PanelContent> = panels.iter().collect();

        let outcome = surface.render_frame(|view, device, queue| {
            compositor.compose(
                editor,
                fold_state,
                scroll_y,
                &mut highlights,
                FrameTarget {
                    view,
                    device,
                    queue,
                    width,
                    height,
                },
            )?;
            if tabs.is_some() || strip.is_some() || !panel_refs.is_empty() {
                overlay.paint(
                    FrameTarget {
                        view,
                        device,
                        queue,
                        width,
                        height,
                    },
                    tabs.as_ref(),
                    strip.as_ref(),
                    &panel_refs,
                    theme,
                )?;
            }
            Ok(())
        });

        match outcome {
            Ok(()) => {
                // The frame is submitted and presented; the pending keydown,
                // if any, is answered. A failed frame keeps it pending — the
                // keystroke's effect is still unpresented, so the eventual
                // sample honestly spans the failed attempt.
                if self.latency.is_armed()
                    && let Some(milliseconds) = self.latency.frame_presented(Instant::now())
                {
                    let _ = writeln!(
                        io::stderr(),
                        "latency: keydown->present {milliseconds:.2}ms"
                    );
                }
            },
            Err(error) => {
                let _ = writeln!(io::stderr(), "iridium-desktop: frame failed: {error}");
            },
        }
    }

    /// What the prompt strip should show this frame, if anything.
    ///
    /// The open prompt wins over a message — it is the question being asked —
    /// and a frame with neither skips the overlay pass entirely.
    fn strip_content(&self) -> Option<StripContent> {
        if let Some(prompt) = &self.prompt {
            return Some(StripContent {
                text: prompt.line(),
                caret_column: prompt.caret_column(),
                is_error: false,
            });
        }
        self.message.as_ref().map(|message| StripContent {
            text: message.text().to_owned(),
            caret_column: None,
            is_error: message.is_error(),
        })
    }

    /// Composes every open panel for this frame, bottom-most first so the
    /// palette — the most modal thing on screen — paints on top.
    ///
    /// A window too small for an honest panel composes none; the panels' keys
    /// keep working regardless, so `Escape` is never trapped behind a resize.
    fn panel_contents(&mut self) -> Vec<PanelContent> {
        let Some(shell) = &mut self.shell else {
            return Vec::new();
        };
        let Some(fit) = shell
            .overlay
            .panel_fit(shell.surface.width(), shell.surface.height())
        else {
            return Vec::new();
        };
        let Some(editor) = self.workspace.active_editor() else {
            return Vec::new();
        };
        let theme = &editor.state().theme;
        let mut panels = Vec::new();
        if self.search_open {
            panels.push(self.search.content(editor, theme, fit));
        }
        if self.history_open {
            panels.push(self.history.content(editor, theme, fit));
        }
        if self.palette_open {
            panels.push(self.palette.content(editor, &self.mru, theme, fit));
        }
        // Last, and so on top and last in the painter's placement record —
        // which is how `painted_menu` finds it again for the pointer.
        if let Some(menu) = &self.menu {
            panels.push(menu.content(theme, fit));
        }
        panels
    }
}
