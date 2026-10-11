//! Stable disk backing and bounded I/O for source pieces and appended edits.
use super::{
    Cancellation,
    pieces::{Piece, Store},
    utf8::Validator,
};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const BLOCK_BYTES: usize = 64 * 1024;

pub struct Source {
    original: File,
    added: File,
}
impl Source {
    pub fn capture(
        path: &Path,
        cancel: &Cancellation,
        mut progress: impl FnMut(u64),
    ) -> io::Result<(Self, u64)> {
        cancel.check()?;
        let mut input = File::open(path)?;
        let before = stamp(&input.metadata()?)?;
        if !input.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "paged source must be a regular file",
            ));
        }
        let mut original = tempfile::tempfile()?;
        let mut block = vec![0; BLOCK_BYTES];
        let mut validator = Validator::default();
        let mut copied = 0_u64;
        loop {
            cancel.check()?;
            let count = input.read(&mut block)?;
            if count == 0 {
                break;
            }
            let bytes = &block[..count];
            validator.feed(bytes)?;
            original.write_all(bytes)?;
            copied = copied
                .checked_add(count as u64)
                .ok_or_else(|| io::Error::other("source size exceeds u64"))?;
            progress(copied);
        }
        validator.finish()?;
        if stamp(&input.metadata()?)? != before || copied != before.len {
            return Err(io::Error::other(
                "source changed while its snapshot was being captured",
            ));
        }
        cancel.check()?;
        Ok((
            Self {
                original,
                added: tempfile::tempfile()?,
            },
            copied,
        ))
    }
    const fn file(&mut self, store: Store) -> &mut File {
        match store {
            Store::Original => &mut self.original,
            Store::Added => &mut self.added,
        }
    }
    pub fn append(&mut self, text: &str) -> io::Result<Piece> {
        let offset = self.added.seek(SeekFrom::End(0))?;
        offset
            .checked_add(text.len() as u64)
            .ok_or_else(|| io::Error::other("edit store exceeds u64"))?;
        self.added.write_all(text.as_bytes())?;
        Ok(Piece {
            store: Store::Added,
            offset,
            len: text.len() as u64,
        })
    }
    pub fn read(&mut self, piece: Piece, bytes: &mut [u8]) -> io::Result<()> {
        if bytes.len() as u64 != piece.len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "read buffer differs from piece length",
            ));
        }
        let file = self.file(piece.store);
        file.seek(SeekFrom::Start(piece.offset))?;
        file.read_exact(bytes)
    }
    pub fn stream(
        &mut self,
        piece: Piece,
        output: &mut impl Write,
        cancel: &Cancellation,
        block: &mut [u8],
    ) -> io::Result<()> {
        if block.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "stream buffer is empty",
            ));
        }
        let file = self.file(piece.store);
        file.seek(SeekFrom::Start(piece.offset))?;
        let mut remaining = piece.len;
        while remaining > 0 {
            cancel.check()?;
            let count =
                usize::try_from(remaining.min(block.len() as u64)).map_err(io::Error::other)?;
            let chunk = &mut block[..count];
            file.read_exact(chunk)?;
            output.write_all(chunk)?;
            remaining -= count as u64;
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}
fn stamp(metadata: &std::fs::Metadata) -> io::Result<Stamp> {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt as _;
    Ok(Stamp {
        len: metadata.len(),
        modified: metadata.modified()?,
        #[cfg(unix)]
        identity: (
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        ),
    })
}
