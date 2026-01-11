//! Input handling for keyboard and mouse events.
//!
//! This module provides a unified input system that translates raw keyboard
//! and mouse events into editor actions. It handles modifiers, key mapping,
//! and mouse gestures.

mod keyboard;
mod mouse;
mod types;

pub use keyboard::{Key, KeyEvent, KeyHandler, Modifiers};
pub use mouse::{ClickState, MouseButton, MouseEvent, MouseHandler};
pub use types::{InputAction, InputResult};
