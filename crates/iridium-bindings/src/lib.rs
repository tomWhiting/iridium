//! # Iridium Bindings
//!
//! TypeScript and WASM bindings for the Iridium editor.
//!
//! This crate provides napi-rs bindings that work for both native
//! Node.js modules and WebAssembly targets.

#![doc(html_root_url = "https://docs.rs/iridium-bindings/0.1.0")]

mod editor;
mod events;
mod types;

pub use editor::IridiumEditor;
pub use events::EventEmitter;
pub use types::*;

use napi_derive::napi;

/// Check if WebGPU is supported in the current environment.
#[napi]
pub async fn is_webgpu_supported() -> bool {
    // WebGPU support check will be implemented
    true
}

/// Get the list of supported languages.
#[napi]
pub fn get_supported_languages() -> Vec<String> {
    iridium_syntax::Language::all().iter().map(|l| l.id().to_string()).collect()
}

/// Library version.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
