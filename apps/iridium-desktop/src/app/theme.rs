//! Which theme the window wears, and who decided.
//!
//! Three things can set it, in strictly increasing precedence:
//!
//! 1. the kernel's default, which every session starts on;
//! 2. the system appearance, read once when the window opens and followed
//!    through [`WindowEvent::ThemeChanged`](winit::event::WindowEvent);
//! 3. an explicit choice — `--theme` on the command line, or the
//!    `view.toggleTheme` command — which **pins** the session and stops it
//!    following.
//!
//! # Why a pin, and why it never lifts
//!
//! Under macOS's "Auto" appearance the system flips at sunrise and sunset.
//! Without a pin, that flip silently reverses a choice the user made
//! deliberately, at a moment they did not pick and with no indication that
//! anything decided against them. A pin that lasts until the window closes is
//! the smallest thing that prevents it. Lifting it would need somewhere to
//! record the preference, and the estate has no preferences store yet (D-7).
//!
//! # ⚠️ Why the titlebar can disagree with the editor
//!
//! macOS will match the titlebar to a pinned theme if the window's appearance
//! is set — but calling `Window::set_theme` **permanently silences
//! `ThemeChanged` for that window** (winit's `macos/window_delegate.rs`). The
//! system-following default is worth more than a matched titlebar, so this
//! face never calls it, and a manually-pinned light editor under a dark system
//! keeps a dark titlebar. That is a knowing trade, and a reversible one: a
//! later ruling can swap it for a matched titlebar in one line.

use iridium_editor::theme::Theme;

use super::state::{DesktopApp, Flow};

/// Where the window's theme came from, and so whether the system's appearance
/// still gets a say.
///
/// An enum rather than an `is_pinned` boolean because the question a reader
/// asks at the call site is "who decided this?", and a bare `true` answers it
/// only if you already know. `DesktopApp` carries three other flags that are
/// genuinely open/closed; a fourth that was not would have read like one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ThemeSource {
    /// Following the system appearance, and repainted whenever it changes.
    #[default]
    System,
    /// An explicit choice — `--theme`, or the `view.toggleTheme` command —
    /// which the system no longer overrides. Lasts until the window closes.
    Pinned,
}

impl DesktopApp {
    /// Puts `theme` on the workspace and on the compositor behind it.
    ///
    /// Both, always. The workspace's copy is what every tab reads and what
    /// survives a tab switch; the compositor's is what the next frame is
    /// painted from. Setting one without the other is a window whose colours
    /// change on the next unrelated redraw, which is the shape the two callers
    /// here exist to keep identical.
    pub(super) fn apply_theme(&mut self, theme: Theme) {
        self.workspace.set_theme(theme.clone());
        if let Some(shell) = self.shell.as_mut() {
            shell.compositor.set_theme(theme);
        }
        self.request_redraw();
    }

    /// Swaps light for dark and back, and pins the session.
    ///
    /// A toggle rather than a `useLight`/`useDark` pair: there is no state to
    /// abandon, so the pair would be two ids where one does.
    pub(super) fn toggle_theme(&mut self) -> Flow {
        let next = if self.workspace.theme().is_dark {
            Theme::light()
        } else {
            Theme::dark()
        };
        self.theme_source = ThemeSource::Pinned;
        self.apply_theme(next);
        Flow::Running
    }

    /// Follows the system appearance, unless this session is pinned.
    ///
    /// Takes a `bool` rather than winit's `Theme` so the decision is testable
    /// without a window; the winit seam does the translation. Answers whether
    /// the appearance was applied, which is what the tests read.
    pub(super) fn follow_system_appearance(&mut self, dark: bool) -> bool {
        if self.theme_source == ThemeSource::Pinned {
            return false;
        }
        // Compared before applying, because `apply_theme` requests a redraw
        // and a `ThemeChanged` that changes nothing should cost no frame.
        // macOS delivers one on some unrelated appearance changes.
        if self.workspace.theme().is_dark == dark {
            return false;
        }
        self.apply_theme(if dark { Theme::dark() } else { Theme::light() });
        true
    }
}
