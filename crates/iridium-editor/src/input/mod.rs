//! Input handling for keyboard, mouse, and IME.
//!
//! [`keyboard`] is a public module, not merely a set of re-exports, because a host
//! that contributes a command needs the same per-cursor edit builders the kernel's
//! own commands use — [`keyboard::editing`] and [`keyboard::motions`]. Without
//! them, "add a command from outside the kernel" would mean "reimplement
//! multi-cursor editing outside the kernel", and every host command would silently
//! act on the primary cursor only.

mod ime;
pub mod keyboard;
mod mouse;

pub use ime::{ImeEvent, ImeHandler, ImeResult, ImeState};
pub use keyboard::{
    AstRequest, ClipboardOperation, CommandRunError, HistoryRequest, KeyCode, KeyEvent, KeyResult,
    KeyboardHandler, Modifiers, SearchAction,
};
pub use mouse::{
    GutterClickConfig, MouseButton, MouseEvent, MouseEventKind, MouseHandler, MouseResult,
};
