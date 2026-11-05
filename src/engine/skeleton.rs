//! Creating the destination directory tree and symlinks before file copies start.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use super::error::EngineError;
use crate::domain::job::SymlinkJob;

/// Creates every directory (parents are listed before children).
pub fn create_dirs(dirs: &[PathBuf]) -> Result<(), EngineError> {
    dirs.iter().try_for_each(|dir| {
        fs::create_dir_all(dir).map_err(|source| EngineError::DirCreate {
            path: dir.clone(),
            source,
        })
    })
}

/// Recreates symlinks verbatim, replacing anything already at the destination.
pub fn create_symlinks(links: &[SymlinkJob]) -> Result<(), EngineError> {
    links.iter().try_for_each(|link| {
        if fs::symlink_metadata(&link.dst).is_ok() {
            fs::remove_file(&link.dst).ok();
        }
        symlink(&link.target, &link.dst).map_err(|source| EngineError::Symlink {
            path: link.dst.clone(),
            source,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_nested_dirs() {
        let root = tempfile::tempdir().unwrap();
        let deep = root.path().join("a/b/c");
        create_dirs(&[root.path().join("a"), deep.clone()]).unwrap();
        assert!(deep.is_dir());
    }

    #[test]
    fn replaces_existing_symlink() {
        let root = tempfile::tempdir().unwrap();
        let dst = root.path().join("link");
        symlink("old", &dst).unwrap();
        let job = SymlinkJob {
            src: PathBuf::new(),
            dst: dst.clone(),
            target: PathBuf::from("new"),
        };
        create_symlinks(&[job]).unwrap();
        assert_eq!(fs::read_link(&dst).unwrap(), PathBuf::from("new"));
    }
}
