//! Finder file delivery shares the existing tab-opening and error paths.

use super::{DesktopApp, Flow};
use crate::events::DesktopEvent;
use crate::prompt::Message;

impl DesktopApp {
    pub(super) fn native_event(&mut self, event: DesktopEvent) -> Flow {
        match event {
            DesktopEvent::Menu(command) => self.run_menu_command(&command),
            DesktopEvent::OpenFiles(files) => {
                self.pending_open.extend(files);
                self.drain_native_files();
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
