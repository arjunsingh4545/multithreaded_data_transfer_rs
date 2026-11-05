//! Parallel directory walk producing a [`ScanManifest`].

use std::fs::{self, DirEntry};
use std::os::unix::fs::{DirEntryExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use rayon::prelude::*;

use super::error::EngineError;
use super::progress::ProgressSink;
use crate::domain::job::{FileJob, FileMeta, ScanManifest, SymlinkJob};
use crate::util::fs::rebase;

/// Report scan progress every this many files.
const REPORT_EVERY: usize = 256;

/// Shared, read-only walk state. Borrowed by every rayon task.
struct ScanCtx<'a, P> {
    src_root: &'a Path,
    dst_root: &'a Path,
    progress: &'a P,
    files: AtomicUsize,
    bytes: AtomicU64,
}

/// One directory's worth of results; merged up the tree.
#[derive(Default)]
struct Partial {
    dirs: Vec<PathBuf>,
    symlinks: Vec<SymlinkJob>,
    files: Vec<FileJob>,
}

impl Partial {
    fn merge(mut self, mut other: Self) -> Self {
        self.dirs.append(&mut other.dirs);
        self.symlinks.append(&mut other.symlinks);
        self.files.append(&mut other.files);
        self
    }
}

/// Walks `src_root`, mapping every entry to its place under `dst_root`.
///
/// Fails fast on the first unreadable directory or entry. Entries other than
/// directories, regular files and symlinks (sockets, FIFOs, devices) are skipped.
pub fn scan_tree<P: ProgressSink>(
    src_root: &Path,
    dst_root: &Path,
    progress: &P,
) -> Result<ScanManifest, EngineError> {
    let ctx = ScanCtx {
        src_root,
        dst_root,
        progress,
        files: AtomicUsize::new(0),
        bytes: AtomicU64::new(0),
    };

    let found = walk(&ctx, src_root)?;
    let mut directories = Vec::with_capacity(found.dirs.len() + 1);
    directories.push(dst_root.to_path_buf());
    directories.extend(found.dirs);
    // Parents before children so creation never needs to recurse.
    directories.sort_by_key(|p| p.components().count());

    let total_bytes = ctx.bytes.into_inner();
    progress.scanned(found.files.len(), total_bytes);
    Ok(ScanManifest {
        directories,
        symlinks: found.symlinks,
        files: found.files,
        total_bytes,
    })
}

fn walk<P: ProgressSink>(ctx: &ScanCtx<'_, P>, dir: &Path) -> Result<Partial, EngineError> {
    let dir_err = |source| EngineError::DirAccess {
        path: dir.to_path_buf(),
        source,
    };
    let mut entries: Vec<DirEntry> = fs::read_dir(dir)
        .map_err(dir_err)?
        .collect::<Result<_, _>>()
        .map_err(dir_err)?;
    // Inode order makes the following `stat` calls far less seeky on HDDs.
    entries.sort_unstable_by_key(DirEntryExt::ino);

    let mut here = Partial::default();
    let mut subdirs = Vec::new();
    for entry in entries {
        let src = entry.path();
        let meta = fs::symlink_metadata(&src).map_err(|source| EngineError::Metadata {
            path: src.clone(),
            source,
        })?;
        let Some(dst) = rebase(&src, ctx.src_root, ctx.dst_root) else {
            continue;
        };
        let kind = meta.file_type();

        if kind.is_symlink() {
            let target = fs::read_link(&src).map_err(|source| EngineError::Symlink {
                path: src.clone(),
                source,
            })?;
            here.symlinks.push(SymlinkJob { src, dst, target });
        } else if kind.is_dir() {
            here.dirs.push(dst);
            subdirs.push(src);
        } else if kind.is_file() {
            ctx.record(meta.len());
            here.files.push(FileJob {
                size: meta.len(),
                ino: meta.ino(),
                meta: FileMeta::from(&meta),
                src,
                dst,
            });
        }
    }

    let below = subdirs
        .par_iter()
        .map(|sub| walk(ctx, sub))
        .try_reduce(Partial::default, |a, b| Ok(a.merge(b)))?;
    Ok(here.merge(below))
}

impl<P: ProgressSink> ScanCtx<'_, P> {
    fn record(&self, size: u64) {
        let files = self.files.fetch_add(1, Ordering::Relaxed) + 1;
        let bytes = self.bytes.fetch_add(size, Ordering::Relaxed) + size;
        if files.is_multiple_of(REPORT_EVERY) {
            self.progress.scanned(files, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::progress::NoProgress;
    use std::os::unix::fs::symlink;

    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let p = root.path();
        fs::create_dir_all(p.join("a/b")).unwrap();
        fs::create_dir_all(p.join("empty")).unwrap();
        fs::write(p.join("top.txt"), b"12345").unwrap();
        fs::write(p.join("a/b/deep.bin"), vec![0u8; 100]).unwrap();
        symlink("top.txt", p.join("link")).unwrap();
        root
    }

    #[test]
    fn finds_all_files_and_bytes() {
        let src = tree();
        let manifest = scan_tree(src.path(), Path::new("/out"), &NoProgress).unwrap();
        assert_eq!((manifest.files.len(), manifest.total_bytes), (2, 105));
    }

    #[test]
    fn records_symlinks_without_following() {
        let src = tree();
        let manifest = scan_tree(src.path(), Path::new("/out"), &NoProgress).unwrap();
        assert_eq!(manifest.symlinks[0].target, PathBuf::from("top.txt"));
    }

    #[test]
    fn maps_paths_under_destination() {
        let src = tree();
        let manifest = scan_tree(src.path(), Path::new("/out"), &NoProgress).unwrap();
        let deep = manifest.files.iter().find(|f| f.size == 100).unwrap();
        assert_eq!(deep.dst, PathBuf::from("/out/a/b/deep.bin"));
    }

    #[test]
    fn directories_include_root_and_empty_dirs_parents_first() {
        let src = tree();
        let manifest = scan_tree(src.path(), Path::new("/out"), &NoProgress).unwrap();
        assert_eq!(manifest.directories[0], PathBuf::from("/out"));
        assert!(manifest.directories.contains(&PathBuf::from("/out/empty")));
    }

    #[test]
    fn missing_source_is_dir_access_error() {
        let result = scan_tree(
            Path::new("/definitely/missing"),
            Path::new("/out"),
            &NoProgress,
        );
        assert!(matches!(result, Err(EngineError::DirAccess { .. })));
    }
}
