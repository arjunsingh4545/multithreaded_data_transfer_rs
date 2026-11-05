//! Restoring source metadata on copied files (shared by both copy paths).

use std::fs::{self, File};
use std::io;
use std::path::Path;

use crate::domain::job::FileMeta;

/// Applies times, then permissions, through an open handle.
///
/// Times go first: a restrictive mode could otherwise block the time update.
pub fn apply_to_file(file: &File, meta: &FileMeta) -> io::Result<()> {
    filetime::set_file_handle_times(file, Some(meta.accessed), Some(meta.modified))?;
    file.set_permissions(meta.permissions.clone())
}

/// Applies times, then permissions, by path.
pub fn apply_to_path(path: &Path, meta: &FileMeta) -> io::Result<()> {
    filetime::set_file_times(path, meta.accessed, meta.modified)?;
    fs::set_permissions(path, meta.permissions.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use filetime::FileTime;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    fn meta() -> FileMeta {
        FileMeta {
            permissions: Permissions::from_mode(0o600),
            modified: FileTime::from_unix_time(1_000_000, 0),
            accessed: FileTime::from_unix_time(1_000_000, 0),
        }
    }

    fn mode_and_mtime(path: &Path) -> (u32, FileTime) {
        let m = fs::metadata(path).unwrap();
        (
            m.permissions().mode() & 0o777,
            FileTime::from_last_modification_time(&m),
        )
    }

    #[test]
    fn apply_to_path_sets_permissions_and_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        fs::write(&path, b"x").unwrap();
        apply_to_path(&path, &meta()).unwrap();
        assert_eq!(mode_and_mtime(&path), (0o600, meta().modified));
    }

    #[test]
    fn apply_to_file_sets_permissions_and_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        let file = File::create(&path).unwrap();
        apply_to_file(&file, &meta()).unwrap();
        assert_eq!(mode_and_mtime(&path), (0o600, meta().modified));
    }
}
