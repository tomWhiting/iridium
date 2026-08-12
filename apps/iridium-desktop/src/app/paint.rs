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
use crate::file_tree;
use crate::overlay::{PaintedFrame, PanelAnchor, PanelContent, PanelKind, StripContent};
use crate::units::u32_to_f32;

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
        // The explorer's directory reads land on a worker thread, and
        // nothing wakes winit when they do. Collecting them here — before
        // the panels are composed — is what puts them on *this* frame rather
        // than the next input's, and `is_waiting` below is what guarantees
        // there is a next frame at all.
        let waiting = self.explorer.as_mut().is_some_and(|explorer| {
            explorer.poll();
            explorer.is_waiting()
        });

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
        let mut highlights = document.syntax.resolver(theme);
        let panel_refs: Vec<(PanelKind, &PanelContent)> = panels
            .iter()
            .map(|(kind, content)| (*kind, content))
            .collect();
        // Filled by the overlay pass and taken below only if the frame
        // presented. ⚠️ A frame that failed leaves the *previous* one on
        // screen, so its record is the one a press must still resolve
        // against — adopting a half-built record here would hit-test against
        // chrome nobody can see.
        let mut painted = PaintedFrame::default();

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
                    &mut painted,
                )?;
            }
            Ok(())
        });

        if waiting && let Some(shell) = &self.shell {
            // Exactly while a read is outstanding, and not one frame longer:
            // a panel that repainted whenever it was merely *open* would spin
            // the GPU at the display's refresh rate for as long as someone
            // was reading a file list.
            shell.window.request_redraw();
        }

        match outcome {
            Ok(()) => {
                // Adopted only here: this is the frame that reached the
                // screen, and so the only one a pointer can have aimed at. A
                // frame with no overlay pass leaves the default — nothing
                // painted — which is the honest record of a screen showing
                // only the document.
                self.painted = painted;
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
    /// Each panel is paired with its [`PanelKind`], which travels with it into
    /// the painter and comes back in the frame's record: what a press resolves
    /// to and what was drawn are then the same list read twice, rather than
    /// two lists that have to be kept in step.
    ///
    /// A window too small for an honest panel composes none; the panels' keys
    /// keep working regardless, so `Escape` is never trapped behind a resize.
    fn panel_contents(&mut self) -> Vec<(PanelKind, PanelContent)> {
        // Taken before the shell is borrowed below, and `None` unless the
        // explorer is placed as a sidebar *and* this window can hold one —
        // the same one answer the left reserve is computed from, so the band
        // that is reserved and the panel that is drawn cannot disagree.
        let sidebar = self.explorer_sidebar_fit();
        let Some(shell) = &mut self.shell else {
            return Vec::new();
        };
        let Some(fit) = shell
            .overlay
            .panel_fit(shell.surface.width(), shell.surface.height())
        else {
            return Vec::new();
        };
        // The same measurement the top reserve is made from, so a sidebar's
        // top edge and the document's agree rather than being two numbers.
        let strip = shell
            .overlay
            .tab_strip_height(u32_to_f32(shell.surface.height()));
        let Some(editor) = self.workspace.active_editor() else {
            return Vec::new();
        };
        let theme = &editor.state().theme;
        let mut panels = Vec::new();
        if self.search_open {
            panels.push((PanelKind::Search, self.search.content(editor, theme, fit)));
        }
        if self.history_open {
            panels.push((PanelKind::History, self.history.content(editor, theme, fit)));
        }
        if let Some(explorer) = self.explorer.as_mut() {
            // ⭐ The anchor is chosen here rather than in the panel, and that
            // is the whole of what the explorer knows about being a sidebar:
            // nothing. Its rows, its keys and its filter are the same code in
            // both placements, so there is nothing to keep in step — and since
            // the panel moved to `iridium-panel` it is not merely that it does
            // not look, it is that it structurally cannot. `PanelAnchor` does
            // not exist over there.
            let anchor = sidebar.map_or(PanelAnchor::Top, |sidebar| PanelAnchor::Left {
                top: strip,
                interior_rows: sidebar.max_interior_rows,
            });
            let content = file_tree::content(explorer, theme, sidebar.unwrap_or(fit), anchor);
            panels.push((PanelKind::Explorer, content));
        }
        if self.palette_open {
            panels.push((
                PanelKind::Palette,
                self.palette.content(editor, &self.mru, theme, fit),
            ));
        }
        // Last, and so on top: the frame's record is read topmost-first, so a
        // press inside a menu opened over another panel reaches the menu.
        if let Some(menu) = &self.menu {
            panels.push((PanelKind::Menu, menu.content(theme, fit)));
        }
        apply_hover(&mut panels, self.hover);
        panels
    }
}

/// Marks the hovered row on whichever composed panel the pointer is over.
///
/// Applied after composition for the same reason the sidebar's anchor is: a
/// builder has no idea where a pointer is, and a panel that had to be told
/// would carry a mouse in its signature into the terminal face, which has
/// none.
///
/// A free function so it is checkable without a window — [`DesktopApp::redraw`]
/// composes nothing at all before the event loop resumes, so this is the only
/// part of the hover path a test can reach directly.
fn apply_hover(panels: &mut [(PanelKind, PanelContent)], hover: Option<(PanelKind, usize)>) {
    let Some((kind, row)) = hover else {
        return;
    };
    for (panel_kind, content) in panels {
        if *panel_kind == kind {
            content.hovered = Some(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::theme::Color;

    use super::{PanelAnchor, PanelContent, PanelKind, apply_hover};
    use crate::overlay::{PanelRow, Span};

    /// A one-row panel with nothing hovered.
    fn panel() -> PanelContent {
        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: 8,
            rows: vec![PanelRow::new(vec![Span::new(
                "row",
                Color::new(1.0, 1.0, 1.0, 1.0),
            )])],
            caret: None,
            hovered: None,
        }
    }

    #[test]
    fn the_hover_lands_on_the_panel_it_names_and_no_other() {
        let mut panels = vec![
            (PanelKind::Explorer, panel()),
            (PanelKind::Palette, panel()),
        ];
        apply_hover(&mut panels, Some((PanelKind::Palette, 3)));
        assert_eq!(panels[0].1.hovered, None, "the explorer is not hovered");
        assert_eq!(panels[1].1.hovered, Some(3));
    }

    #[test]
    fn no_hover_marks_nothing() {
        let mut panels = vec![(PanelKind::Explorer, panel())];
        apply_hover(&mut panels, None);
        assert_eq!(panels[0].1.hovered, None);
    }

    #[test]
    fn a_hover_on_a_panel_that_is_no_longer_composed_marks_nothing() {
        // The pointer rested on the palette and the palette then closed; the
        // frame between the two must not paint a band on the panel that took
        // its place.
        let mut panels = vec![(PanelKind::Explorer, panel())];
        apply_hover(&mut panels, Some((PanelKind::Palette, 0)));
        assert_eq!(panels[0].1.hovered, None);
    }
}
