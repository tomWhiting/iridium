//! Staged replacement: every fallible preparation step precedes publication.
use super::{Guard, create_temporary, parent_of, resolve, sync_directory};
use crate::{Durability, Publication};
use std::{
    fs::{self, File, Permissions},
    io,
    path::Path,
    path::PathBuf,
};

pub struct Replacement {
    file: File,
    target: PathBuf,
    directory: PathBuf,
    temporary: Guard,
    permissions: Option<Permissions>,
}
/// Prepared bytes; committing consumes the temporary name exactly once.
pub struct PreparedReplacement {
    target: PathBuf,
    directory: PathBuf,
    temporary: Guard,
}
impl Replacement {
    pub fn new(path: &Path) -> io::Result<Self> {
        let target = resolve(path)?;
        let directory = parent_of(&target);
        let permissions = match fs::metadata(&target) {
            Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "save target is not a regular file",
                ));
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let (file, temporary) = create_temporary(&directory, &target)?;
        Ok(Self {
            file,
            target,
            directory,
            temporary,
            permissions,
        })
    }
    pub const fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }
    pub fn prepare(self) -> io::Result<PreparedReplacement> {
        if let Some(permissions) = self.permissions {
            self.file.set_permissions(permissions)?;
        }
        self.file.sync_all()?;
        drop(self.file);
        Ok(PreparedReplacement {
            target: self.target,
            directory: self.directory,
            temporary: self.temporary,
        })
    }
}
impl PreparedReplacement {
    pub fn commit(self) -> io::Result<Publication> {
        self.commit_with(sync_directory)
    }
    pub fn commit_with(
        self,
        synchronize: impl FnOnce(&Path) -> io::Result<bool>,
    ) -> io::Result<Publication> {
        fs::rename(self.temporary.path(), &self.target)?;
        self.temporary.disarm();
        let durability = match synchronize(&self.directory) {
            Ok(true) => Durability::Synced,
            Ok(false) => Durability::Unsupported,
            Err(error) => Durability::Unconfirmed(error),
        };
        Ok(Publication { durability })
    }
}
