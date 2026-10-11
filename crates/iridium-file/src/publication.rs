//! A completed replacement and the separately observed durability outcome.
use std::io;

/// What happened after a replacement became visible at its destination.
#[derive(Debug)]
#[must_use = "inspect the durability outcome of the published file"]
pub struct Publication {
    /// Directory durability after the file was atomically replaced.
    pub durability: Durability,
}

/// Whether directory metadata was synchronized after publication.
#[derive(Debug)]
pub enum Durability {
    /// The file and directory synchronization operations succeeded.
    Synced,
    /// This platform/filesystem cannot synchronize directories this way.
    Unsupported,
    /// Bytes were published, but the subsequent directory operation failed.
    Unconfirmed(io::Error),
}
impl Publication {
    /// Compatibility result for callers without a durability UI. An error here
    /// means the file **was published**; it must not be interpreted as an
    /// unchanged destination. Unsupported directory sync remains accepted.
    pub fn into_result(self) -> io::Result<()> {
        match self.durability {
            Durability::Synced | Durability::Unsupported => Ok(()),
            Durability::Unconfirmed(error) => Err(error),
        }
    }
}
