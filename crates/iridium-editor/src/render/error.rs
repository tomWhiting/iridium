//! Rendering error types.
//!
//! Provides detailed error types for GPU rendering operations, following
//! the principle of no silent failures - every error should be visible
//! with actionable information.

use thiserror::Error;

/// Errors that can occur during rendering operations.
///
/// Each error variant provides detailed information to help diagnose
/// and fix the issue. Errors are designed to be actionable.
#[derive(Debug, Error)]
pub enum RenderError {
    /// No compatible GPU adapter was found.
    ///
    /// This typically means:
    /// - No GPU is available
    /// - The GPU doesn't support the required features
    /// - GPU drivers are not installed or outdated
    #[error(
        "No compatible GPU adapter found. Ensure you have a GPU that supports WebGPU/Vulkan/Metal/DX12 \
        and that drivers are up to date. Requested: power_preference={power_preference}, \
        compatible_surface={has_surface}"
    )]
    AdapterNotFound {
        /// The power preference that was requested.
        power_preference: String,
        /// Whether a compatible surface was required.
        has_surface: bool,
    },

    /// Failed to request a device from the adapter.
    ///
    /// This can happen if:
    /// - The requested features are not supported
    /// - The requested limits exceed hardware capabilities
    /// - The GPU is in use by another process exclusively
    #[error(
        "Failed to create GPU device from adapter: {message}. \
        Adapter: {adapter_name}, Backend: {backend}. \
        This may indicate unsupported features or resource exhaustion."
    )]
    DeviceCreationFailed {
        /// The error message from wgpu.
        message: String,
        /// The name of the adapter that was being used.
        adapter_name: String,
        /// The graphics backend (Vulkan, Metal, DX12, etc.).
        backend: String,
    },

    /// Failed to create a surface for rendering.
    #[error(
        "Failed to create rendering surface: {message}. \
        Ensure the window handle is valid and the display server is running."
    )]
    SurfaceCreationFailed {
        /// The error message.
        message: String,
    },

    /// Surface configuration failed.
    #[error(
        "Failed to configure surface: width={width}, height={height}, format={format:?}. \
        Error: {message}. Ensure dimensions are non-zero and the format is supported."
    )]
    SurfaceConfigurationFailed {
        /// The requested surface width.
        width: u32,
        /// The requested surface height.
        height: u32,
        /// The requested texture format.
        format: wgpu::TextureFormat,
        /// The error message.
        message: String,
    },

    /// Failed to acquire the next surface texture.
    #[error("Failed to acquire surface texture: {status:?}. {recovery_hint}")]
    SurfaceTextureAcquisitionFailed {
        /// The surface error status.
        status: wgpu::SurfaceError,
        /// A hint on how to recover from this error.
        recovery_hint: String,
    },

    /// Shader compilation failed.
    #[error(
        "Shader compilation failed: {message}. \
        Shader: {shader_name}. Check shader source for syntax errors."
    )]
    ShaderCompilationFailed {
        /// The error message from the shader compiler.
        message: String,
        /// The name or identifier of the shader.
        shader_name: String,
    },

    /// Pipeline creation failed.
    #[error(
        "Failed to create render pipeline: {message}. \
        Pipeline: {pipeline_name}. Check vertex/fragment shader compatibility."
    )]
    PipelineCreationFailed {
        /// The error message.
        message: String,
        /// The name of the pipeline being created.
        pipeline_name: String,
    },

    /// Texture creation failed.
    #[error(
        "Failed to create texture: {message}. \
        Size: {width}x{height}, Format: {format:?}. \
        This may indicate insufficient GPU memory or unsupported format."
    )]
    TextureCreationFailed {
        /// The error message.
        message: String,
        /// The requested texture width.
        width: u32,
        /// The requested texture height.
        height: u32,
        /// The requested texture format.
        format: wgpu::TextureFormat,
    },

    /// Buffer creation or upload failed.
    #[error(
        "Buffer operation failed: {message}. \
        Buffer: {buffer_name}, Size: {size} bytes. \
        This may indicate insufficient GPU memory."
    )]
    BufferOperationFailed {
        /// The error message.
        message: String,
        /// The name of the buffer.
        buffer_name: String,
        /// The size of the buffer in bytes.
        size: u64,
    },

    /// Font loading or text shaping failed.
    #[error("Font error: {message}. Font: {font_info}")]
    FontError {
        /// The error message.
        message: String,
        /// Information about the font that caused the error.
        font_info: String,
    },

    /// Text layout failed.
    #[error("Text layout failed: {message}")]
    TextLayoutFailed {
        /// The error message.
        message: String,
    },

    /// The renderer is not initialized.
    #[error(
        "Renderer not initialized. Call initialize() before rendering. \
        Current state: {state}"
    )]
    NotInitialized {
        /// Description of the current renderer state.
        state: String,
    },

    /// Invalid dimensions provided.
    #[error(
        "Invalid dimensions: width={width}, height={height}. \
        Both dimensions must be positive and within GPU limits (max={max_dimension})."
    )]
    InvalidDimensions {
        /// The provided width.
        width: u32,
        /// The provided height.
        height: u32,
        /// The maximum supported dimension.
        max_dimension: u32,
    },

    /// A GPU validation error occurred (debug builds only).
    #[error("GPU validation error: {message}. This indicates a bug in the renderer.")]
    ValidationError {
        /// The validation error message.
        message: String,
    },

    /// An internal error that should not occur in normal operation.
    #[error("Internal renderer error: {message}. Please report this bug.")]
    InternalError {
        /// The error message.
        message: String,
    },
}

impl RenderError {
    /// Creates an adapter not found error.
    pub fn adapter_not_found(power_preference: wgpu::PowerPreference, has_surface: bool) -> Self {
        Self::AdapterNotFound {
            power_preference: format!("{power_preference:?}"),
            has_surface,
        }
    }

    /// Creates a device creation failed error.
    pub fn device_creation_failed(
        error: impl std::fmt::Display,
        adapter_info: &wgpu::AdapterInfo,
    ) -> Self {
        Self::DeviceCreationFailed {
            message: error.to_string(),
            adapter_name: adapter_info.name.clone(),
            backend: format!("{:?}", adapter_info.backend),
        }
    }

    /// Creates a surface texture acquisition failed error with recovery hint.
    pub fn surface_texture_failed(error: wgpu::SurfaceError) -> Self {
        let recovery_hint = match error {
            wgpu::SurfaceError::Lost => {
                "The surface was lost. Recreate the surface and try again.".to_string()
            }
            wgpu::SurfaceError::OutOfMemory => {
                "GPU out of memory. Try reducing texture sizes or closing other applications."
                    .to_string()
            }
            wgpu::SurfaceError::Outdated => {
                "Surface configuration is outdated. Reconfigure the surface with current window size."
                    .to_string()
            }
            wgpu::SurfaceError::Timeout => {
                "Timeout waiting for surface. The GPU may be overloaded. Try again.".to_string()
            }
            wgpu::SurfaceError::Other => {
                "An unspecified surface error occurred. Check GPU drivers and try again.".to_string()
            }
        };

        Self::SurfaceTextureAcquisitionFailed {
            status: error,
            recovery_hint,
        }
    }

    /// Returns true if this error is recoverable by retrying or reconfiguring.
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::SurfaceTextureAcquisitionFailed { status, .. }
                if matches!(status,
                    wgpu::SurfaceError::Timeout |
                    wgpu::SurfaceError::Outdated |
                    wgpu::SurfaceError::Lost
                )
        )
    }

    /// Returns true if this error indicates the surface needs to be reconfigured.
    #[must_use]
    pub fn needs_surface_reconfigure(&self) -> bool {
        matches!(
            self,
            Self::SurfaceTextureAcquisitionFailed { status, .. }
                if matches!(status, wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_messages_are_descriptive() {
        let error = RenderError::AdapterNotFound {
            power_preference: "HighPerformance".to_string(),
            has_surface: true,
        };
        let msg = error.to_string();
        assert!(msg.contains("HighPerformance"));
        assert!(msg.contains("WebGPU"));
    }

    #[test]
    fn test_surface_error_recovery_hints() {
        let error = RenderError::surface_texture_failed(wgpu::SurfaceError::Lost);
        let msg = error.to_string();
        assert!(msg.contains("Recreate the surface"));
    }

    #[test]
    fn test_is_recoverable() {
        let timeout = RenderError::surface_texture_failed(wgpu::SurfaceError::Timeout);
        assert!(timeout.is_recoverable());

        let oom = RenderError::surface_texture_failed(wgpu::SurfaceError::OutOfMemory);
        assert!(!oom.is_recoverable());
    }

    #[test]
    fn test_needs_surface_reconfigure() {
        let outdated = RenderError::surface_texture_failed(wgpu::SurfaceError::Outdated);
        assert!(outdated.needs_surface_reconfigure());

        let timeout = RenderError::surface_texture_failed(wgpu::SurfaceError::Timeout);
        assert!(!timeout.needs_surface_reconfigure());
    }
}
