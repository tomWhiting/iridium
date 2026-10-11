//! Bounded OSC 52 copies using the terminal stack's encoding.

use std::io::{self, Write};
use termina::escape::osc::{Osc, Selection};

/// Maximum raw UTF-8 bytes per terminal copy; terminal limits can be smaller.
pub const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

/// Sends a clipboard copy and flushes it. Success means delivery to the tty,
/// not acceptance by the terminal. No clipboard query or read is sent.
///
/// # Errors
/// Rejects oversized copies before writing anything, and propagates IO failures.
pub fn write_clipboard(sink: &mut impl Write, text: &str) -> io::Result<()> {
    if text.len() > MAX_CLIPBOARD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Terminal copy exceeds 1 MiB; use system or internal clipboard mode",
        ));
    }
    write!(sink, "{}", Osc::SetSelection(Selection::CLIPBOARD, text))?;
    sink.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_utf8_and_controls_as_data_without_requesting_clipboard_contents() -> io::Result<()> {
        let mut bytes = Vec::new();
        write_clipboard(&mut bytes, "猫\r\n\x1b]52;c;?\x07")?;
        assert_eq!(bytes, b"\x1b]52;c;54yrDQobXTUyO2M7Pwc=\x1b\\");
        assert!(!bytes.contains(&b'?'));
        assert!(!bytes.contains(&7));
        Ok(())
    }

    #[test]
    fn oversized_copy_writes_nothing() {
        let mut bytes = Vec::new();
        assert!(write_clipboard(&mut bytes, &"x".repeat(MAX_CLIPBOARD_BYTES + 1)).is_err());
        assert!(bytes.is_empty());
    }

    struct FailingWriter {
        fail_flush: bool,
    }
    impl Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_flush {
                Ok(bytes.len())
            } else {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "write failed"))
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "flush failed"))
        }
    }

    #[test]
    fn both_write_and_flush_errors_propagate() {
        for fail_flush in [false, true] {
            let result = write_clipboard(&mut FailingWriter { fail_flush }, "copy");
            assert!(result.is_err_and(|error| error.kind() == io::ErrorKind::BrokenPipe));
        }
    }
}
