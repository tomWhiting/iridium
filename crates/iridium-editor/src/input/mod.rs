//! Input handling for keyboard, mouse, and IME.

mod ime;
mod keyboard;
mod mouse;

pub use ime::{ImeEvent, ImeHandler, ImeResult, ImeState};
pub use keyboard::{
    ClipboardOperation, KeyCode, KeyEvent, KeyResult, KeyboardHandler, Modifiers, SearchAction,
};
pub use mouse::{
    GutterClickConfig, MouseButton, MouseEvent, MouseEventKind, MouseHandler, MouseResult,
};
