use crate::error::{CopyError, Result};
use indicatif::ProgressBar;
use rayon::prelude::*;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct FileJob {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub size: u64,
    pub permissions: fs::Permissions,
}

#[derive(Debug, Clone)]
pub struct SymlinkJob {
    #[cfg_attr(unix, allow(dead_code))]
    pub src: PathBuf,
    pub dst: PathBuf,
    pub target: PathBuf,
}

#[derive(Default, Debug)]
pub struct ScanManifest {
    pub directories: Vec<PathBuf>,
    pub symlinks: Vec<SymlinkJob>,
    pub files: Vec<FileJob>,
    pub total_bytes: u64,
}

/// Scans `src_root` in parallel using Rayon.
/// Immediately fails and short-circuits if ANY directory or entry cannot be accessed.
pub fn scan_directory_parallel(
    src_root: &Path,
    dst_root: &Path,
    spinner: &ProgressBar,
) -> Result<ScanManifest> {
    if !src_root.exists() && fs::symlink_metadata(src_root).is_err() {
        return Err(CopyError::SourceNotFound(src_root.to_path_buf()));
    }

    let dirs = Arc::new(Mutex::new( vec![dst_root.to_path_buf()] ));
    let symlinks = Arc::new(Mutex::new(Vec::new()));
    let files = Arc::new(Mutex::new(Vec::new()));
    let first_error: Arc<Mutex<Option<CopyError>>> = Arc::new(Mutex::new(None));

    let scanned_files = Arc::new(AtomicUsize::new(0));
    let scanned_bytes = Arc::new(AtomicU64::new(0));

    fn walk_rayon(
        current_src: &Path,
        src_root: &Path,
        dst_root: &Path,
        dirs: &Arc<Mutex<Vec<PathBuf>>>,
        symlinks: &Arc<Mutex<Vec<SymlinkJob>>>,
        files: &Arc<Mutex<Vec<FileJob>>>,
        first_error: &Arc<Mutex<Option<CopyError>>>,
        scanned_files: &Arc<AtomicUsize>,
        scanned_bytes: &Arc<AtomicU64>,
        spinner: &ProgressBar,
    ) {
        // Short-circuit if another Rayon thread hit an access error
        if first_error.lock().unwrap().is_some() {
            return;
        }

        // Strict Requirement #6: Throw error if unable to access any directory
        let read_dir = match fs::read_dir(current_src) {
            Ok(rd) => rd,
            Err(e) => {
                let mut err_guard = first_error.lock().unwrap();
                if err_guard.is_none() {
                    *err_guard = Some(CopyError::DirectoryAccessDenied {
                        path: current_src.to_path_buf(),
                        source: e,
                    });
                }
                return;
            }
        };

        let mut subdirs = Vec::new();
        let mut local_files = Vec::new();
        let mut local_symlinks = Vec::new();
        let mut local_dirs = Vec::new();

        for entry_res in read_dir {
            let entry = match entry_res {
                Ok(e) => e,
                Err(e) => {
                    let mut err_guard = first_error.lock().unwrap();
                    if err_guard.is_none() {
                        *err_guard = Some(CopyError::DirectoryAccessDenied {
                            path: current_src.to_path_buf(),
                            source: e,
                        });
                    }
                    return;
                }
            };

            let src_path = entry.path();
            let rel_path = src_path.strip_prefix(src_root).unwrap();
            let dst_path = dst_root.join(rel_path);

            // Use symlink_metadata so we do NOT follow symlinks during traversal
            let meta: Metadata = match fs::symlink_metadata(&src_path) {
                Ok(m) => m,
                Err(e) => {
                    let mut err_guard = first_error.lock().unwrap();
                    if err_guard.is_none() {
                        *err_guard = Some(CopyError::MetadataError {
                            path: src_path,
                            source: e,
                        });
                    }
                    return;
                }
            };

            let file_type = meta.file_type();

            if file_type.is_symlink() {
                match fs::read_link(&src_path) {
                    Ok(target) => {
                        local_symlinks.push(SymlinkJob {
                            src: src_path,
                            dst: dst_path,
                            target,
                        });
                    }
                    Err(e) => {
                        let mut err_guard = first_error.lock().unwrap();
                        if err_guard.is_none() {
                            *err_guard = Some(CopyError::SymlinkReadError {
                                path: src_path,
                                source: e,
                            });
                        }
                        return;
                    }
                }
            } else if file_type.is_dir() {
                // Verify we have read/execute access to the child directory upfront
                local_dirs.push(dst_path);
                subdirs.push(src_path);
            } else if file_type.is_file() {
                let size = meta.len();
                local_files.push(FileJob {
                    src: src_path,
                    dst: dst_path,
                    size,
                    permissions: meta.permissions(),
                });

                let count = scanned_files.fetch_add(1, Ordering::Relaxed) + 1;
                let bytes = scanned_bytes.fetch_add(size, Ordering::Relaxed) + size;
                if count % 64 == 0 {
                    spinner.set_message(format!(
                        "Scanning {} ... {} files ({:.2} MB)",
                        src_root.display(),
                        count,
                        bytes as f64 / (1024.0 * 1024.0)
                    ));
                }
            }
        }

        if !local_dirs.is_empty() {
            dirs.lock().unwrap().extend(local_dirs);
        }
        if !local_symlinks.is_empty() {
            symlinks.lock().unwrap().extend(local_symlinks);
        }
        if !local_files.is_empty() {
            files.lock().unwrap().extend(local_files);
        }

        // Recurse into subdirectories in parallel via Rayon
        subdirs.par_iter().for_each(|subdir| {
            walk_rayon(
                subdir,
                src_root,
                dst_root,
                dirs,
                symlinks,
                files,
                first_error,
                scanned_files,
                scanned_bytes,
                spinner,
            );
        });
    }

    walk_rayon(
        src_root,
        src_root,
        dst_root,
        &dirs,
        &symlinks,
        &files,
        &first_error,
        &scanned_files,
        &scanned_bytes,
        spinner,
    );

    if let Some(err) = first_error.lock().unwrap().take() {
        return Err(err);
    }

    let mut directories = Arc::try_unwrap(dirs).unwrap().into_inner().unwrap();
    // Sort directories by depth (shortest first) so parent directories are created before children
    directories.sort_by_key(|p| p.components().count());

    let symlinks = Arc::try_unwrap(symlinks).unwrap().into_inner().unwrap();
    let files = Arc::try_unwrap(files).unwrap().into_inner().unwrap();
    let total_bytes = scanned_bytes.load(Ordering::SeqCst);

    Ok(ScanManifest {
        directories,
        symlinks,
        files,
        total_bytes,
    })
}
