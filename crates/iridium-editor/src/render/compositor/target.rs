//! The surface seam: the handful of wgpu values one frame is composed onto.

use wgpu::{Device, Queue, TextureView};

/// The wgpu objects one frame is composed onto.
///
/// A frame needs exactly these five values from a surface, whatever the
/// surface is — a browser canvas today, a native window tomorrow. Passing
/// them as plain values (rather than as a surface trait) is deliberate: a
/// generic surface abstraction here would buy nothing and would fight
/// `wasm-bindgen` on the web face.
#[derive(Clone, Copy)]
pub struct FrameTarget<'a> {
    /// The texture view the render pass draws into.
    pub view: &'a TextureView,
    /// The device that records the frame's command encoder.
    pub device: &'a Device,
    /// The queue the frame's commands and buffer writes are submitted to.
    pub queue: &'a Queue,
    /// Surface width in physical pixels.
    pub width: u32,
    /// Surface height in physical pixels.
    pub height: u32,
}

impl std::fmt::Debug for FrameTarget<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameTarget")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}
