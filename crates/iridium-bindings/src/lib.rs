//! # Iridium Bindings
//!
//! TypeScript and WASM bindings for the Iridium editor.
//!
//! This crate provides napi-rs bindings that work for both native
//! Node.js modules and WebAssembly targets.
//!
//! # Quick Start
//!
//! ```typescript
//! import { createEditor, isWebGPUSupported, IridiumEditor } from 'iridium-bindings';
//!
//! // Check WebGPU support first
//! const supported = await isWebGPUSupported();
//! if (!supported) {
//!   console.error('WebGPU is not supported in this environment');
//!   return;
//! }
//!
//! // Create an editor instance
//! const editor = createEditor();
//!
//! // Or with configuration
//! const editor = createEditor({
//!   tabWidth: 2,
//!   insertSpaces: true,
//!   showLineNumbers: true,
//! });
//!
//! // Set content
//! editor.setContent('fn main() {\n    println!("Hello, Iridium!");\n}');
//!
//! // Subscribe to events
//! editor.on('contentChanged', (content) => {
//!   console.log('Content changed:', content.length, 'chars');
//! });
//!
//! // Clean up when done
//! editor.destroy();
//! ```

#![doc(html_root_url = "https://docs.rs/iridium-bindings/0.1.0")]

// Shared, target-independent modules (used by the WASM surface,
// unit-tested natively).
pub mod display_scale;
pub mod edit_tracking;
pub mod highlight_span_shift;
pub mod js_index;
pub mod key_map;
pub mod palette;
pub mod remote_edit;
pub mod text_range;
#[cfg(feature = "web")]
pub mod web_folds;
pub mod workspace;

// Napi modules (Node.js bindings)
#[cfg(feature = "napi")]
mod editor;
#[cfg(feature = "napi")]
mod events;
#[cfg(feature = "napi")]
mod types;

#[cfg(feature = "napi")]
pub use editor::IridiumEditor;
#[cfg(feature = "napi")]
pub use events::{EventCallback, EventEmitter, JsEventEmitter};
#[cfg(feature = "napi")]
pub use types::*;

#[cfg(feature = "napi")]
use napi::bindgen_prelude::*;
#[cfg(feature = "napi")]
use napi_derive::napi;

/// Adapts a JavaScript string owned by napi-rs to APIs that only need a text view.
///
/// napi-rs requires JavaScript primitive arguments by value, so exported functions
/// cannot use the otherwise preferable `&str` signature.
#[cfg(feature = "napi")]
fn with_napi_str<T>(value: impl AsRef<str>, f: impl FnOnce(&str) -> T) -> T {
    f(value.as_ref())
}

// WASM module (browser bindings)
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod wasm;
// Compiled for the host as well when testing. The span index is ordinary
// Rust over `rust_lapper` with no binding surface of its own, so the
// `wasm32` gate on it was never about what it could compile — it rode along
// with `wasm.rs`, its only consumer. The effect was that nothing in this
// module could be tested by any gate: `cargo test` never built it, and the
// wasm gate is a `check`, which compiles without running a single test.
#[cfg(all(feature = "web", any(target_arch = "wasm32", test)))]
mod web_span_index;

#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub use wasm::*;

// ============================================================================
// Factory Functions (T154) - napi only
// ============================================================================

#[cfg(feature = "napi")]
/// Creates a new editor instance with optional configuration.
///
/// This is the recommended way to create an editor in TypeScript.
///
/// # Example
///
/// ```typescript
/// import { createEditor } from 'iridium-bindings';
///
/// const editor = createEditor({ tabWidth: 2 });
/// ```
#[napi]
pub fn create_editor(config: Option<JsEditorConfig>) -> Result<IridiumEditor> {
    IridiumEditor::with_config(config)
}

#[cfg(feature = "napi")]
/// Creates an editor instance with initial content.
///
/// This is a convenience function that creates an editor and sets content
/// in a single call.
///
/// # Example
///
/// ```typescript
/// import { createEditorWithContent } from 'iridium-bindings';
///
/// const editor = createEditorWithContent('Hello, world!');
/// ```
#[napi]
pub fn create_editor_with_content(
    content: String,
    config: Option<JsEditorConfig>,
) -> Result<IridiumEditor> {
    let editor = IridiumEditor::with_config(config)?;
    editor.set_content(content);
    Ok(editor)
}

// ============================================================================
// WebGPU Support Check (T155) - napi only
// ============================================================================

#[cfg(feature = "napi")]
async fn request_high_performance_adapter()
-> std::result::Result<wgpu::Adapter, wgpu::RequestAdapterError> {
    use wgpu::{Backends, Instance, InstanceDescriptor, InstanceFlags};

    let instance = Instance::new(&InstanceDescriptor {
        backends: Backends::all(),
        flags: InstanceFlags::empty(),
        ..Default::default()
    });
    instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
}

#[cfg(feature = "napi")]
fn request_high_performance_adapter_blocking()
-> Result<std::result::Result<wgpu::Adapter, wgpu::RequestAdapterError>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|error| {
            Error::from_reason(format!("failed to create WebGPU worker runtime: {error}"))
        })?;
    Ok(runtime.block_on(request_high_performance_adapter()))
}

#[cfg(feature = "napi")]
/// Worker task used by [`is_webgpu_supported`].
pub struct WebGpuSupportTask;

#[cfg(feature = "napi")]
#[napi]
impl Task for WebGpuSupportTask {
    type Output = bool;
    type JsValue = bool;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(request_high_performance_adapter_blocking()?.is_ok())
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[cfg(feature = "napi")]
/// Check if WebGPU is supported in the current environment.
///
/// This performs an actual check by attempting to request a GPU adapter.
/// Returns true if WebGPU is available and can be used for rendering.
///
/// # Example
///
/// ```typescript
/// import { isWebGPUSupported } from 'iridium-bindings';
///
/// const supported = await isWebGPUSupported();
/// if (!supported) {
///   console.error('WebGPU is required but not available');
/// }
/// ```
#[napi]
pub fn is_webgpu_supported() -> AsyncTask<WebGpuSupportTask> {
    AsyncTask::new(WebGpuSupportTask)
}

#[cfg(feature = "napi")]
/// Worker task used by [`get_gpu_info`].
pub struct GpuInfoTask;

#[cfg(feature = "napi")]
#[napi]
impl Task for GpuInfoTask {
    type Output = Option<JsGPUInfo>;
    type JsValue = Option<JsGPUInfo>;

    fn compute(&mut self) -> Result<Self::Output> {
        let adapter = request_high_performance_adapter_blocking()?.ok();
        Ok(adapter.map(|adapter| {
            let info = adapter.get_info();
            JsGPUInfo {
                name: info.name,
                vendor: info.vendor.to_string(),
                device_type: format!("{:?}", info.device_type),
                backend: format!("{:?}", info.backend),
            }
        }))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[cfg(feature = "napi")]
/// Get detailed GPU information.
///
/// Returns information about the available GPU adapter, or None if
/// WebGPU is not supported.
///
/// # Example
///
/// ```typescript
/// import { getGPUInfo } from 'iridium-bindings';
///
/// const info = await getGPUInfo();
/// if (info) {
///   console.log('GPU:', info.name, '-', info.backend);
/// }
/// ```
#[napi]
pub fn get_gpu_info() -> AsyncTask<GpuInfoTask> {
    AsyncTask::new(GpuInfoTask)
}

#[cfg(feature = "napi")]
/// GPU information for TypeScript.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct JsGPUInfo {
    /// GPU name
    pub name: String,
    /// GPU vendor ID
    pub vendor: String,
    /// Device type (e.g., `DiscreteGpu`, `IntegratedGpu`)
    pub device_type: String,
    /// Graphics backend (e.g., `Vulkan`, `Metal`, `Dx12`)
    pub backend: String,
}

// ============================================================================
// Utility Functions - napi only
// ============================================================================

#[cfg(all(feature = "napi", feature = "syntax"))]
/// Get the list of supported languages for syntax highlighting.
///
/// Returns an array of language IDs that can be used with `setLanguage()`.
///
/// # Example
///
/// ```typescript
/// import { getSupportedLanguages } from 'iridium-bindings';
///
/// const languages = getSupportedLanguages();
/// console.log('Supported:', languages.join(', '));
/// ```
#[napi]
pub fn get_supported_languages() -> Vec<String> {
    iridium_syntax::Language::all()
        .iter()
        .map(|l| l.id().to_string())
        .collect()
}

#[cfg(all(feature = "napi", feature = "syntax"))]
/// Get language information for a file extension.
///
/// Returns the language ID for the given extension, or None if not recognized.
///
/// # Example
///
/// ```typescript
/// import { getLanguageForExtension } from 'iridium-bindings';
///
/// const lang = getLanguageForExtension('rs'); // Returns 'rust'
/// ```
#[napi]
pub fn get_language_for_extension(extension: String) -> Option<String> {
    with_napi_str(extension, |extension| {
        iridium_syntax::Language::from_extension(extension)
            .map(|language| language.id().to_string())
    })
}

#[cfg(all(feature = "napi", feature = "syntax"))]
/// Get the list of file extensions for a language.
///
/// Returns an array of file extensions (without dots) that match the language.
///
/// # Example
///
/// ```typescript
/// import { getExtensionsForLanguage } from 'iridium-bindings';
///
/// const exts = getExtensionsForLanguage('typescript'); // Returns ['ts', 'mts', 'cts']
/// ```
///
/// An unrecognised language name returns an empty array rather than an error:
/// asking which files a language claims is a question with a legitimate answer
/// of "none I know of".
///
/// This reads [`iridium_syntax::Language::extensions`] rather than carrying its
/// own table. It used to carry one, and that copy was already wrong — it had
/// never heard of `.jsonc`, and nothing could have told it so.
#[napi]
pub fn get_extensions_for_language(language: String) -> Vec<String> {
    use iridium_syntax::Language;

    with_napi_str(language, |language| {
        Language::from_id(language).map_or_else(Vec::new, |language| {
            language
                .extensions()
                .iter()
                .map(|extension| (*extension).to_string())
                .collect()
        })
    })
}

#[cfg(feature = "napi")]
/// Library version.
///
/// Returns the version of the iridium-bindings package.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(feature = "napi")]
/// Get full version information.
///
/// Returns version information for all Iridium components.
#[napi]
pub fn get_version_info() -> JsVersionInfo {
    JsVersionInfo {
        bindings: env!("CARGO_PKG_VERSION").to_string(),
        editor: env!("CARGO_PKG_VERSION").to_string(),
        syntax: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(feature = "napi")]
/// Version information for all components.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct JsVersionInfo {
    /// Version of iridium-bindings
    pub bindings: String,
    /// Version of iridium-editor
    pub editor: String,
    /// Version of iridium-syntax
    pub syntax: String,
}

#[cfg(all(test, feature = "napi"))]
mod tests {
    use super::*;

    #[test]
    fn create_editor_default() {
        let editor = create_editor(None).unwrap();
        assert_eq!(editor.get_content(), "");
    }

    #[test]
    fn create_editor_with_content_test() {
        let editor = create_editor_with_content("Hello".to_string(), None).unwrap();
        assert_eq!(editor.get_content(), "Hello");
    }

    #[test]
    #[cfg(feature = "syntax")]
    fn get_supported_languages_returns_list() {
        let languages = get_supported_languages();
        assert!(!languages.is_empty());
        assert!(languages.contains(&"rust".to_string()));
    }

    #[test]
    #[cfg(feature = "syntax")]
    fn get_language_for_extension_works() {
        assert_eq!(
            get_language_for_extension("rs".to_string()),
            Some("rust".to_string())
        );
        assert_eq!(
            get_language_for_extension("py".to_string()),
            Some("python".to_string())
        );
        assert_eq!(get_language_for_extension("unknown".to_string()), None);
    }

    #[test]
    fn version_is_set() {
        let v = version();
        assert!(!v.is_empty());
    }
}
