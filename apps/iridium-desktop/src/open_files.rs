//! Files the operating system asks the running app to open.
//!
//! macOS sends them to an application delegate: a double-click in Finder, a
//! drop on the Dock icon. Other platforms pass files to open as arguments,
//! which startup already reads, so there is nothing to install there.

use winit::event_loop::EventLoop;

use crate::events::DesktopEvent;

#[cfg(target_os = "macos")]
#[path = "macos_files.rs"]
mod macos;

/// Installs the file-open delegate on the application.
///
/// winit guarantees it installs no application delegate of its own. Call
/// this after the event loop is built, and keep the returned value until
/// `run_app` returns: `AppKit` holds its delegate weakly.
///
/// # Errors
///
/// When not on the main thread, or when another delegate is already
/// installed.
#[cfg(target_os = "macos")]
pub fn install(event_loop: &EventLoop<DesktopEvent>) -> Result<impl Sized + use<>, String> {
    macos::install(event_loop.create_proxy())
}

/// What a platform with no file-open delegate keeps: nothing.
#[cfg(not(target_os = "macos"))]
pub struct NoDelegate;

/// Files to open arrive as arguments here, so there is nothing to install.
///
/// # Errors
///
/// Never; the signature matches the macOS one.
#[cfg(not(target_os = "macos"))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "one signature on every platform, and macOS can fail"
)]
pub const fn install(_event_loop: &EventLoop<DesktopEvent>) -> Result<NoDelegate, String> {
    Ok(NoDelegate)
}
