//! Finder file delivery shares the existing tab-opening and error paths.

use std::sync::Arc;

use winit::event_loop::EventLoopProxy;

use super::{DesktopApp, Flow};
use crate::events::DesktopEvent;
use crate::prompt::Message;

impl DesktopApp {
    /// Has every tab's parser thread wake the event loop when a parse lands,
    /// so the structure it found is drawn without waiting for the next key.
    ///
    /// Called from [`crate::run`], because only an `EventLoop` can make a
    /// proxy. Without it a landed parse still shows, on the next frame some
    /// other event asks for.
    pub fn attach_syntax_wake(&mut self, proxy: EventLoopProxy<DesktopEvent>) {
        self.workspace.parse_in_background(Some(Arc::new(move || {
            // An error means the event loop has gone, and with it any frame
            // the parse could have reached.
            let _ = proxy.send_event(DesktopEvent::SyntaxLanded);
        })));
    }

    pub(super) fn native_event(&mut self, event: DesktopEvent) -> Flow {
        match event {
            DesktopEvent::Menu(command) => self.run_menu_command(&command),
            DesktopEvent::OpenFiles(files) => {
                self.pending_open.extend(files);
                self.drain_native_files();
                Flow::Running
            },
            DesktopEvent::SyntaxLanded => {
                self.request_redraw();
                Flow::Running
            },
        }
    }

    pub(super) fn drain_native_files(&mut self) {
        // A save/close confirmation belongs to the current tab. Switching
        // documents underneath that question could save or close another tab.
        if self.pending_open.is_empty() || self.menu_availability().inert {
            return;
        }
        let mut errors = Vec::new();
        for file in std::mem::take(&mut self.pending_open) {
            match file {
                Ok(path) => {
                    self.open_file(&path);
                    if let Some(message) = &self.message
                        && message.is_error()
                    {
                        errors.push(message.text().to_owned());
                    }
                },
                Err(error) => errors.push(error),
            }
        }
        // A successful later file must not hide an earlier failure.
        if !errors.is_empty() {
            self.message = Some(Message::error(errors.join("; ")));
        }
        self.request_redraw();
    }
}
