//! Event handling for TypeScript bindings.

use napi_derive::napi;

/// Event emitter for editor events.
#[napi]
pub struct EventEmitter {
    // Event handling will be implemented
    _placeholder: (),
}

#[napi]
impl EventEmitter {
    /// Creates a new event emitter.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new()
    }
}
