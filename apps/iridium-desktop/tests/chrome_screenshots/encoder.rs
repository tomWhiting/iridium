//! The encoder's GPU-free proof: a synthetic frame, and the two tests that can
//! run against it on any machine.
//!
//! What these do *not* police is the filter choice — a small synthetic frame is
//! regular enough vertically that `Adaptive` beats `NoFilter` on it, the
//! opposite of the real frames. That is held by the parent's `MAX_RUN_BYTES`,
//! which is measured on the real set.

use super::{frame_byte_count, png_bytes};

/// A frame shaped like the ones the harness really writes — a flat page, a
/// band across it, and a scatter of glyph-like marks — without a GPU.
fn synthetic_frame(width: u32, height: u32) -> Vec<u8> {
    // A capacity hint only; the loop below is what fixes the real length.
    let mut pixels = Vec::with_capacity(frame_byte_count(width, height).unwrap_or_default());
    for y in 0..height {
        for x in 0..width {
            let band = y % 64 < 3;
            let mark = !band && x % 11 < 4 && y % 19 < 12;
            let pixel = match (band, mark) {
                (true, _) => [0x2A, 0x2E, 0x36, 0xFF],
                (_, true) => [0xD4, 0xC8, 0x9A, 0xFF],
                _ => [0x1E, 0x21, 0x27, 0xFF],
            };
            pixels.extend_from_slice(&pixel);
        }
    }
    pixels
}

/// The encoder's oracle, without a GPU: encode a frame, decode it with the
/// `png` crate's decoder, and hold the result to both claims the module doc
/// makes — that the bytes are a real RGBA PNG of the stated size carrying the
/// stated pixels, and that they are actually compressed.
///
/// The second assertion is the one that matters. The defect this replaced was
/// a valid PNG in every respect except that its compression ratio was
/// 1.00008, and no test in the tree could tell.
///
/// The threshold is deliberately loose. This frame is far more compressible
/// than a real one — and it is the wrong shape to judge the *filter* on, so
/// it does not try; see the module doc.
#[test]
fn png_round_trip_is_compressed_and_decodable() {
    let width = 512_u32;
    let height = 384_u32;
    let pixels = synthetic_frame(width, height);
    let encoded = match png_bytes(width, height, &pixels) {
        Ok(bytes) => bytes,
        Err(message) => panic!("{message}"),
    };

    // Stated as integers rather than a ratio so no cast is needed: at least
    // tenfold. The measured frames manage sixtyfold; the stored-block encoder
    // this replaced managed 1.00008 and would fail here.
    assert!(
        encoded.len() * 10 < pixels.len(),
        "the encoder turned {} bytes into {} — under tenfold, which is not \
         the compression this harness depends on",
        pixels.len(),
        encoded.len()
    );

    let mut reader = match png::Decoder::new(std::io::Cursor::new(&encoded)).read_info() {
        Ok(reader) => reader,
        Err(error) => panic!("the encoded bytes are not a readable PNG: {error}"),
    };
    let capacity = reader
        .output_buffer_size()
        .expect("the decoder reports an unrepresentable frame size");
    let mut decoded = vec![0_u8; capacity];
    let info = match reader.next_frame(&mut decoded) {
        Ok(info) => info,
        Err(error) => panic!("the encoded bytes did not decode: {error}"),
    };

    assert_eq!(info.width, width, "the decoded width");
    assert_eq!(info.height, height, "the decoded height");
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "the decoded colour type"
    );
    assert_eq!(
        info.bit_depth,
        png::BitDepth::Eight,
        "the decoded bit depth"
    );
    assert_eq!(
        &decoded[..info.buffer_size()],
        &pixels[..],
        "the decoded pixels"
    );

    // Two named pixels, so the whole-buffer comparison above cannot pass by
    // both sides being wrong the same way: the band that crosses the origin,
    // and the page colour on the first row below it.
    let row_bytes =
        usize::try_from(width).expect("the test frame width does not fit this host") * 4;
    assert_eq!(
        &decoded[..4],
        &[0x2A, 0x2E, 0x36, 0xFF],
        "the band at the origin"
    );
    let page = 3 * row_bytes + 5 * 4;
    assert_eq!(
        &decoded[page..page + 4],
        &[0x1E, 0x21, 0x27, 0xFF],
        "the page below the band"
    );
}

/// Rejects a pixel buffer that does not match the dimensions it is encoded
/// under, rather than writing a frame that decodes to something else.
#[test]
fn png_bytes_refuses_a_mismatched_buffer() {
    let short = vec![0_u8; 4 * 4 * 4];
    assert!(png_bytes(8, 8, &short).is_err(), "an undersized buffer");
    assert!(png_bytes(0, 0, &short).is_err(), "an oversized buffer");
}
