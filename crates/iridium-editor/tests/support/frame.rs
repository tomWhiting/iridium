//! Offscreen render targets, composing onto one, and reading it back.

use std::sync::mpsc;

use iridium_editor::Editor;
use iridium_editor::render::{FrameCompositor, FrameTarget, HighlightSource};

use super::gpu::{Gpu, HEIGHT, WIDTH};

/// One offscreen render target, standing in for a swapchain frame.
pub struct Target {
    /// The texture, held so the view stays valid and the readback can copy
    /// from it.
    pub texture: wgpu::Texture,
    /// The render target view handed to the compositor.
    pub view: wgpu::TextureView,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
}

/// An offscreen render target at the default frame size.
pub fn target(gpu: &Gpu) -> Target {
    target_sized(gpu, WIDTH, HEIGHT)
}

/// An offscreen render target of the given pixel size.
///
/// # Panics
///
/// If `width * 4` is not a multiple of 256. Every reader here assumes rows
/// come back tightly packed, and wgpu pads copy rows to 256 bytes — so a width
/// that violates this returns a buffer with invisible gaps in it rather than an
/// error, and every pixel past the first row would be read from the wrong
/// place. Asserting is the only way that stays a failure.
pub fn target_sized(gpu: &Gpu, width: u32, height: u32) -> Target {
    assert_eq!(
        (width * 4) % 256,
        0,
        "target widths are chosen so readback rows need no padding"
    );
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&gpu.label("Target")),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Target {
        texture,
        view,
        width,
        height,
    }
}

/// One compose onto `target`, with the blink pinned and the GPU work awaited.
///
/// The wait is not politeness: these tests read the between-frames caches the
/// compositor fills, and a frame still in flight has not filled them.
///
/// The blink reset is what makes two frames of the same document comparable.
/// Without it the caret's phase depends on wall-clock time, so an otherwise
/// identical pair of frames differs in a handful of pixels, intermittently.
pub fn compose(
    compositor: &mut FrameCompositor,
    editor: &Editor,
    scroll_y: f32,
    highlights: &mut dyn HighlightSource,
    gpu: &Gpu,
    target: &Target,
) {
    compositor.reset_blink();
    if let Err(error) = compositor.compose(
        editor,
        editor.fold_state(),
        scroll_y,
        highlights,
        FrameTarget {
            view: &target.view,
            device: &gpu.device,
            queue: &gpu.queue,
            width: target.width,
            height: target.height,
        },
    ) {
        gpu.die(&format!("the frame did not compose: {error}"));
    }
    if let Err(error) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
        gpu.die(&format!("the frame's GPU work did not complete: {error}"));
    }
}

/// The composed frame's pixels, row-major BGRA.
///
/// Rows come back tightly packed because [`target_sized`] refuses any width
/// that would need unpadding.
///
/// # The map result is checked, and it did not used to be
///
/// Two of the three harnesses this replaces passed `|_| {}` as the `map_async`
/// callback and dropped the result on the floor. A failed map there reached
/// `get_mapped_range` with nothing having reported why — the pixels would be
/// wrong or the call would fault, and either way the test's own message would
/// be about geometry rather than about the readback. The channel below is the
/// third harness's version, kept because it is the one that can tell you what
/// went wrong.
pub fn read_pixels(gpu: &Gpu, target: &Target) -> Vec<u8> {
    let bytes_per_row = target.width * 4;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(&gpu.label("Readback")),
        size: u64::from(bytes_per_row) * u64::from(target.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some(&gpu.label("Readback Encoder")),
        });
    encoder.copy_texture_to_buffer(
        target.texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: target.width,
            height: target.height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(std::iter::once(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    if let Err(error) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
        gpu.die(&format!("the readback did not complete: {error}"));
    }
    match receiver.recv() {
        Ok(Ok(())) => {},
        Ok(Err(error)) => gpu.die(&format!("mapping the readback buffer failed: {error}")),
        Err(_) => gpu.die("the map callback never ran"),
    }
    let data = slice.get_mapped_range().to_vec();
    buffer.unmap();
    data
}
