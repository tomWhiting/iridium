//! Incremental UTF-8 validation retains at most three bytes between blocks.
use std::io;

#[derive(Default)]
pub struct Validator {
    pending: Vec<u8>,
    validated: u64,
}
impl Validator {
    pub fn feed(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.pending.is_empty() {
            return self.validate(bytes);
        }
        let mut joined = std::mem::take(&mut self.pending);
        joined.extend_from_slice(bytes);
        self.validate(&joined)
    }
    fn validate(&mut self, bytes: &[u8]) -> io::Result<()> {
        match std::str::from_utf8(bytes) {
            Ok(_) => {
                self.validated += bytes.len() as u64;
                Ok(())
            },
            Err(error) if error.error_len().is_none() => {
                self.validated += error.valid_up_to() as u64;
                self.pending
                    .extend_from_slice(&bytes[error.valid_up_to()..]);
                Ok(())
            },
            Err(error) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid UTF-8 at byte {}",
                    self.validated + error.valid_up_to() as u64
                ),
            )),
        }
    }
    pub fn finish(&self) -> io::Result<()> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("incomplete UTF-8 at byte {}", self.validated),
            ))
        }
    }
}
