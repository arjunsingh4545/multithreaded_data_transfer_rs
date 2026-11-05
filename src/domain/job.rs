//! What gets copied: the scan manifest and its per-file jobs.

use std::fs::{Metadata, Permissions};
use std::path::PathBuf;

use filetime::FileTime;

/// Index of a file job inside the (ordered) job slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(usize);

impl FileId {
    /// Wraps a slice index.
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// The slice index.
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Source metadata that is re-applied to the destination after copying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    /// Mode bits.
    pub permissions: Permissions,
    /// Last modification time.
    pub modified: FileTime,
    /// Last access time.
    pub accessed: FileTime,
}

impl From<&Metadata> for FileMeta {
    fn from(meta: &Metadata) -> Self {
        Self {
            permissions: meta.permissions(),
            modified: FileTime::from_last_modification_time(meta),
            accessed: FileTime::from_last_access_time(meta),
        }
    }
}

/// One regular file to copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileJob {
    /// Source path.
    pub src: PathBuf,
    /// Destination path.
    pub dst: PathBuf,
    /// Size in bytes at scan time.
    pub size: u64,
    /// Source inode number (used for inode ordering).
    pub ino: u64,
    /// Metadata to restore on the destination.
    pub meta: FileMeta,
}

/// One symbolic link to recreate (the link itself, never its target).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymlinkJob {
    /// Source link path.
    pub src: PathBuf,
    /// Destination link path.
    pub dst: PathBuf,
    /// The link's target, stored verbatim.
    pub target: PathBuf,
}

/// Everything found under the source root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanManifest {
    /// Destination directories, parents before children.
    pub directories: Vec<PathBuf>,
    /// Symlinks to recreate.
    pub symlinks: Vec<SymlinkJob>,
    /// Regular files to copy.
    pub files: Vec<FileJob>,
    /// Sum of all file sizes.
    pub total_bytes: u64,
}

impl ScanManifest {
    /// Recomputes [`total_bytes`](Self::total_bytes) after files were filtered out.
    pub fn recount_bytes(&mut self) {
        self.total_bytes = self.files.iter().map(|f| f.size).sum();
    }
}

/// Test-only constructors shared by unit tests across the crate.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A job for `src` → `/dst/<src>` with the given size and inode.
    pub(crate) fn job_at(src: &str, size: u64, ino: u64) -> FileJob {
        FileJob {
            src: PathBuf::from(src),
            dst: PathBuf::from("/dst").join(src.trim_start_matches('/')),
            size,
            ino,
            meta: FileMeta {
                permissions: Permissions::from_mode(0o644),
                modified: FileTime::zero(),
                accessed: FileTime::zero(),
            },
        }
    }

    /// A job that only matters for its size.
    pub(crate) fn job(size: u64) -> FileJob {
        job_at("/s", size, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::job;
    use super::*;

    #[test]
    fn recount_bytes_sums_remaining_files() {
        let mut manifest = ScanManifest {
            files: vec![job(3), job(4)],
            total_bytes: 999,
            ..ScanManifest::default()
        };
        manifest.recount_bytes();
        assert_eq!(manifest.total_bytes, 7);
    }

    #[test]
    fn file_id_roundtrips_index() {
        assert_eq!(FileId::new(42).index(), 42);
    }
}
