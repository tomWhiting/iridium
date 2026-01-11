//! Event handling for TypeScript bindings.
//!
//! This module provides an event subscription system that allows TypeScript code
//! to subscribe to editor events using callbacks. Events are emitted synchronously
//! when editor state changes.
//!
//! # Supported Events
//!
//! - `contentChanged`: Fired when document content changes
//! - `selectionChanged`: Fired when selection/cursor position changes
//! - `themeChanged`: Fired when theme is switched
//! - `focus`: Fired when editor gains focus
//! - `blur`: Fired when editor loses focus
//! - `searchUpdated`: Fired when search results update
//! - `foldChanged`: Fired when fold state changes
//!
//! # Example (TypeScript)
//!
//! ```typescript
//! const id = editor.on('contentChanged', (content: string) => {
//!   console.log('Content:', content);
//! });
//!
//! // Later, to unsubscribe:
//! editor.off(id);
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;

/// Type alias for event callback functions.
///
/// The callback receives a single string argument containing event data.
/// For structured data (like selections), the string contains JSON.
/// In napi-rs v3, ThreadsafeFunction uses const generics instead of ErrorStrategy.
pub type EventCallback = ThreadsafeFunction<String, ()>;

/// A subscription entry holding the callback.
struct Subscription {
    id: u32,
    callback: EventCallback,
}

/// Thread-safe event emitter for broadcasting editor events to TypeScript callbacks.
///
/// This implementation uses thread-safe function callbacks from napi-rs to safely
/// call JavaScript functions from any thread.
pub struct EventEmitter {
    /// Map of event name to list of subscriptions
    subscriptions: Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
    /// Counter for generating unique subscription IDs
    next_id: AtomicU32,
}

impl EventEmitter {
    /// Creates a new event emitter.
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicU32::new(1), // Start at 1, 0 reserved for "no subscription"
        }
    }

    /// Subscribes to an event with a callback.
    ///
    /// Returns a subscription ID that can be used to unsubscribe.
    pub fn subscribe(&self, event: &str, callback: EventCallback) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let subscription = Subscription { id, callback };

        let mut subs = self
            .subscriptions
            .write()
            .unwrap_or_else(|e| e.into_inner());
        subs.entry(event.to_string())
            .or_default()
            .push(subscription);

        id
    }

    /// Unsubscribes using a subscription ID.
    pub fn unsubscribe(&self, subscription_id: u32) {
        let mut subs = self
            .subscriptions
            .write()
            .unwrap_or_else(|e| e.into_inner());

        for listeners in subs.values_mut() {
            listeners.retain(|s| s.id != subscription_id);
        }
    }

    /// Removes all listeners for a specific event.
    pub fn remove_listeners(&self, event: &str) {
        let mut subs = self
            .subscriptions
            .write()
            .unwrap_or_else(|e| e.into_inner());
        subs.remove(event);
    }

    /// Clears all subscriptions.
    pub fn clear_all(&self) {
        let mut subs = self
            .subscriptions
            .write()
            .unwrap_or_else(|e| e.into_inner());
        subs.clear();
    }

    /// Emits an event to all subscribers.
    ///
    /// The data is passed as a string to all callbacks. For structured data,
    /// serialize to JSON before calling this method.
    pub fn emit(&self, event: &str, data: &str) {
        let subs = self.subscriptions.read().unwrap_or_else(|e| e.into_inner());

        if let Some(listeners) = subs.get(event) {
            for subscription in listeners {
                // Call the callback with the data
                // Using non-blocking mode; ignore errors since callbacks may be disconnected
                let _ = subscription.callback.call(
                    Ok(data.to_string()),
                    ThreadsafeFunctionCallMode::NonBlocking,
                );
            }
        }
    }

    /// Returns the number of active subscriptions for an event.
    pub fn listener_count(&self, event: &str) -> usize {
        let subs = self.subscriptions.read().unwrap_or_else(|e| e.into_inner());
        subs.get(event).map_or(0, Vec::len)
    }

    /// Returns the total number of active subscriptions across all events.
    pub fn total_listener_count(&self) -> usize {
        let subs = self.subscriptions.read().unwrap_or_else(|e| e.into_inner());
        subs.values().map(Vec::len).sum()
    }

    /// Returns a list of event names that have active listeners.
    pub fn event_names(&self) -> Vec<String> {
        let subs = self.subscriptions.read().unwrap_or_else(|e| e.into_inner());
        subs.keys()
            .filter(|k| !subs.get(*k).map_or(true, Vec::is_empty))
            .cloned()
            .collect()
    }
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: EventEmitter is safe to send between threads and share across threads:
// - `subscriptions` is wrapped in `Arc<RwLock<...>>` which is Send + Sync
// - `next_id` is AtomicU32 which is Send + Sync
// - The contained `ThreadsafeFunction` from napi-rs is designed to be thread-safe
//   (it's the mechanism for safely calling JS from any thread)
// The auto-derive doesn't work because ThreadsafeFunction doesn't implement
// Send/Sync directly, but it's documented as safe for cross-thread use.
#[expect(unsafe_code, reason = "Required for cross-thread event emission")]
unsafe impl Send for EventEmitter {}
#[expect(unsafe_code, reason = "Required for cross-thread event emission")]
unsafe impl Sync for EventEmitter {}

/// Standalone event emitter exposed to TypeScript.
///
/// This can be used independently of `IridiumEditor` for custom event handling.
#[napi]
pub struct JsEventEmitter {
    inner: EventEmitter,
}

#[napi]
impl JsEventEmitter {
    /// Creates a new event emitter.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: EventEmitter::new(),
        }
    }

    /// Subscribes to an event with a callback.
    ///
    /// Returns a subscription ID that can be used to unsubscribe.
    #[napi]
    pub fn on(&self, event: String, callback: EventCallback) -> u32 {
        self.inner.subscribe(&event, callback)
    }

    /// Unsubscribes using a subscription ID.
    #[napi]
    pub fn off(&self, subscription_id: u32) {
        self.inner.unsubscribe(subscription_id);
    }

    /// Removes all listeners for a specific event.
    #[napi]
    pub fn remove_all_listeners(&self, event: Option<String>) {
        if let Some(event_name) = event {
            self.inner.remove_listeners(&event_name);
        } else {
            self.inner.clear_all();
        }
    }

    /// Emits an event to all subscribers.
    #[napi]
    pub fn emit(&self, event: String, data: String) {
        self.inner.emit(&event, &data);
    }

    /// Returns the number of listeners for an event.
    #[napi]
    pub fn listener_count(&self, event: String) -> u32 {
        self.inner.listener_count(&event) as u32
    }

    /// Returns the event names that have listeners.
    #[napi]
    pub fn event_names(&self) -> Vec<String> {
        self.inner.event_names()
    }
}

impl Default for JsEventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: We can't fully test EventEmitter in unit tests because
    // ThreadsafeFunction requires a JavaScript runtime.
    // These tests verify the basic structure.

    #[test]
    fn event_emitter_creation() {
        let emitter = EventEmitter::new();
        assert_eq!(emitter.total_listener_count(), 0);
    }

    #[test]
    fn event_emitter_clear_all() {
        let emitter = EventEmitter::new();
        emitter.clear_all();
        assert_eq!(emitter.total_listener_count(), 0);
    }

    #[test]
    fn event_names_empty() {
        let emitter = EventEmitter::new();
        let names = emitter.event_names();
        assert!(names.is_empty());
    }
}
