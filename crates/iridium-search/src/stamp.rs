//! Observable filesystem revision used alongside exact source/baseline checks.
use std::{fs::Metadata, io, path::Path, time::SystemTime};
/// Metadata captured from the opened file. Not a transactional content snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskStamp {
    size: u64,
    modified: SystemTime,
    identity: Option<(u64, u64)>,
}
impl DiskStamp {
    /// Capture length, modification time and Unix device/inode when available.
    pub fn from_metadata(metadata: &Metadata) -> io::Result<Self> {
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt as _;
            Some((metadata.dev(), metadata.ino()))
        };
        #[cfg(not(unix))]
        let identity = None;
        Ok(Self {
            size: metadata.len(),
            modified: metadata.modified()?,
            identity,
        })
    }
    /// Whether the path still names the observed revision. I/O errors are explicit.
    pub fn agrees(&self, path: &Path) -> io::Result<bool> {
        Self::from_metadata(&std::fs::metadata(path)?).map(|current| &current == self)
    }
}
