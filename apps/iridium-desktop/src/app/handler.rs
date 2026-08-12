//! The winit seam.
//!
//! Every entry point in this module's siblings is reached from here, and
//! nothing here decides anything: an event is routed to the one method that
//! owns it. Keeping the routing in one file is what makes "which of these
//! does winit actually call?" a question with a visible answer.

use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Theme as WinitTheme, WindowId};

use super::startup::Shell;
use super::state::{DesktopApp, Flow};
use crate::keys;
use crate::units::pixel_from_f64;

impl ApplicationHandler for DesktopApp {
    /// Creates the window and everything behind it, once.
    ///
    /// winit delivers `resumed` before any window event and may deliver it
    /// again on some platforms; the shell is created on the first and kept on
    /// the rest. A failure here is recorded and the loop ended — there is no
    /// session without a window, and no return channel but the field.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.shell.is_some() {
            return;
        }
        // The workspace's theme, not the active tab's: the window is painted
        // once for every tab, and the workspace is where that one answer
        // lives.
        match Shell::open(event_loop, self.workspace.theme().clone()) {
            Ok(shell) => {
                shell.window.request_redraw();
                // Read before the shell is stored, because that is the first
                // moment a window exists to ask. `follow_system_appearance`
                // declines when `--theme` already pinned the session, and
                // when winit has no answer (`None` on platforms that do not
                // report one) the kernel's default stands.
                let appearance = shell.window.theme();
                self.shell = Some(shell);
                if let Some(appearance) = appearance {
                    self.follow_system_appearance(appearance == WinitTheme::Dark);
                }
                self.sync_top_inset();
                self.sync_left_inset();
                self.sync_kernel_viewport();
                self.refresh_title();
            },
            Err(message) => {
                self.failure = Some(message);
                event_loop.exit();
            },
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // One window per session; anything else is not ours.
        if self
            .shell
            .as_ref()
            .is_none_or(|shell| shell.window.id() != window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                if self.request_quit() == Flow::Exit {
                    event_loop.exit();
                }
            },
            WindowEvent::DroppedFile(path) => self.dropped(&path),
            WindowEvent::Resized(size) => self.resized(size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.rescaled(scale_factor),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // The latency clock starts at receipt, before translation —
                // the user's finger does not care where the time went. When
                // the flag is off this is the apparatus's whole cost: one
                // branch, no clock read.
                let received = if self.latency.is_armed() {
                    Some(Instant::now())
                } else {
                    None
                };
                // Releases are not presses, and synthetic presses are focus
                // bookkeeping, not typing.
                if event.state == ElementState::Pressed
                    && !is_synthetic
                    && let Some(key) =
                        keys::translate(&event.logical_key, event.repeat, self.modifiers)
                {
                    // Recorded only for a dispatched key: `press` requests a
                    // redraw for every one, so a pending timestamp always has
                    // a frame coming. A dropped key's timestamp dies here —
                    // see `crate::latency` for the whole policy.
                    if let Some(at) = received {
                        self.latency.key_dispatched(at);
                    }
                    if self.press(&key) == Flow::Exit {
                        event_loop.exit();
                    }
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_moved(pixel_from_f64(position.x), pixel_from_f64(position.y));
            },
            WindowEvent::MouseInput { state, button, .. } => {
                match (button, state) {
                    (winit::event::MouseButton::Left, ElementState::Pressed) => {
                        if self.pointer_pressed() == Flow::Exit {
                            event_loop.exit();
                        }
                    },
                    (winit::event::MouseButton::Left, ElementState::Released) => {
                        self.pointer_released();
                    },
                    // The secondary button opens the context menu, and only
                    // the secondary button does (D-3): Ctrl+click keeps the
                    // add-cursor meaning it has shipped with.
                    (winit::event::MouseButton::Right, ElementState::Pressed) => {
                        self.secondary_pressed();
                    },
                    _ => {},
                }
            },
            WindowEvent::MouseWheel { delta, .. } => self.wheel(&delta),
            WindowEvent::ThemeChanged(appearance) => {
                self.follow_system_appearance(appearance == WinitTheme::Dark);
            },
            WindowEvent::Focused(false) => self.blurred(),
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {},
        }
    }
}
