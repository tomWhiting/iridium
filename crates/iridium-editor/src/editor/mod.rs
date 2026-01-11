//! Editor core state and configuration.
//!
//! This module contains:
//! - [`Editor`] - The main editor instance
//! - [`EditorState`] - Complete editor state
//! - [`EditorConfig`] - Configuration options
//! - [`EditorEvent`] - Events emitted to the host
//! - [`IridiumError`] - Error types

mod config;
mod core;

pub use config::EditorConfig;
pub use core::{Editor, EditorEvent, EditorKeyResult, EditorState};

use thiserror::Error;

/// Errors that can occur in the Iridium editor.
#[derive(Debug, Error)]
pub enum IridiumError {
    /// GPU initialization failed.
    #[error("GPU initialization failed: {message}")]
    GpuInitFailed {
        /// Detailed error message
        message: String,
    },

    /// Shader compilation failed.
    #[error("Shader compilation failed: {message}")]
    ShaderCompileFailed {
        /// Detailed error message
        message: String,
    },

    /// Font loading failed.
    #[error("Font loading failed: {message}")]
    FontLoadFailed {
        /// Detailed error message
        message: String,
    },

    /// Parsing error in syntax highlighting.
    #[error("Parse error: {message}")]
    ParseError {
        /// Detailed error message
        message: String,
    },

    /// Invalid UTF-8 sequence encountered.
    #[error("Invalid UTF-8: {message}")]
    InvalidUtf8 {
        /// Detailed error message
        message: String,
    },

    /// Invalid position in document.
    #[error("Invalid position: line {line}, column {column}")]
    InvalidPosition {
        /// Line number
        line: usize,
        /// Column number
        column: usize,
    },

    /// Invalid range in document.
    #[error("Invalid range")]
    InvalidRange,

    /// Operation not supported.
    #[error("Operation not supported: {message}")]
    NotSupported {
        /// Detailed error message
        message: String,
    },
}

/// Error codes for programmatic error handling.
///
/// These codes are exposed to the host application for error handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// GPU initialization failed
    GpuInitFailed,
    /// Shader compilation failed
    ShaderCompileFailed,
    /// Font loading failed
    FontLoadFailed,
    /// Parsing error
    ParseError,
    /// Invalid UTF-8
    InvalidUtf8,
}

impl From<&IridiumError> for ErrorCode {
    fn from(error: &IridiumError) -> Self {
        match error {
            IridiumError::GpuInitFailed { .. } => Self::GpuInitFailed,
            IridiumError::ShaderCompileFailed { .. } => Self::ShaderCompileFailed,
            IridiumError::FontLoadFailed { .. } => Self::FontLoadFailed,
            IridiumError::ParseError { .. } => Self::ParseError,
            IridiumError::InvalidUtf8 { .. } => Self::InvalidUtf8,
            IridiumError::InvalidPosition { .. } | IridiumError::InvalidRange | IridiumError::NotSupported { .. } => {
                Self::ParseError
            }
        }
    }
}
