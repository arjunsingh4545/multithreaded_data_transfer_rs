//! Applying [`ExistingPolicy`] to a scanned manifest.

use std::fs::{self, Metadata};

use filetime::FileTime;

use super::error::EngineError;
use crate::domain::job::{FileJob, ScanManifest};
use crate::domain::request::ExistingPolicy;

/// Coarsest mtime resolution among supported filesystems (FAT/exFAT use 2 s).
const MTIME_TOLERANCE_SECS: i64 = 2;

/// What the filter removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FilterOutcome {
    /// Files and symlinks left untouched.
    pub skipped: usize,
    /// Bytes of skipped files.
    pub skipped_bytes: u64,
}

/// Removes jobs the policy says to leave alone, or fails for [`ExistingPolicy::Fail`].
pub fn apply_policy(
    manifest: &mut ScanManifest,
    policy: ExistingPolicy,
) -> Result<FilterOutcome, EngineError> {
    let before_files = manifest.files.len();
    let before_links = manifest.symlinks.len();
    let before_bytes = manifest.total_bytes;

    match policy {
        ExistingPolicy::Overwrite => return Ok(FilterOutcome::default()),
        ExistingPolicy::Fail => {
            let existing = manifest
                .files
                .iter()
                .map(|f| &f.dst)
                .chain(manifest.symlinks.iter().map(|l| &l.dst))
                .find(|dst| dst.symlink_metadata().is_ok());
            return match existing {
                Some(dst) => Err(EngineError::DestExists(dst.clone())),
                None => Ok(FilterOutcome::default()),
            };
        }
        ExistingPolicy::Skip => manifest
            .files
            .retain(|job| fs::symlink_metadata(&job.dst).is_err()),
        ExistingPolicy::Update => manifest.files.retain(|job| !is_up_to_date(job)),
    }
    manifest
        .symlinks
        .retain(|link| link.dst.symlink_metadata().is_err());
    manifest.recount_bytes();

    Ok(FilterOutcome {
        skipped: (before_files - manifest.files.len()) + (before_links - manifest.symlinks.len()),
        skipped_bytes: before_bytes - manifest.total_bytes,
    })
}

/// Same size and (within tolerance) same modification time.
fn is_up_to_date(job: &FileJob) -> bool {
    fs::metadata(&job.dst)
        .is_ok_and(|dst| dst.len() == job.size && same_mtime(&dst, job.meta.modified))
}

fn same_mtime(dst: &Metadata, src: FileTime) -> bool {
    let dst = FileTime::from_last_modification_time(dst);
    (dst.unix_seconds() - src.unix_seconds()).abs() <= MTIME_TOLERANCE_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::job::FileMeta;
    use std::path::Path;

    /// One file at `dst/name`, optionally pre-existing with `existing` contents.
    fn setup(existing: Option<&[u8]>, src_size: u64) -> (tempfile::TempDir, ScanManifest) {
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("f");
        if let Some(bytes) = existing {
            fs::write(&dst, bytes).unwrap();
        }
        let meta = fs::metadata(dir.path()).unwrap();
        let modified = if existing.is_some() {
            FileTime::from_last_modification_time(&fs::metadata(&dst).unwrap())
        } else {
            FileTime::zero()
        };
        let job = FileJob {
            src: Path::new("/src/f").to_path_buf(),
            dst,
            size: src_size,
            ino: 0,
            meta: FileMeta {
                modified,
                ..FileMeta::from(&meta)
            },
        };
        let manifest = ScanManifest {
            total_bytes: src_size,
            files: vec![job],
            ..ScanManifest::default()
        };
        (dir, manifest)
    }

    #[test]
    fn overwrite_keeps_everything() {
        let (_dir, mut manifest) = setup(Some(b"old"), 3);
        apply_policy(&mut manifest, ExistingPolicy::Overwrite).unwrap();
        assert_eq!(manifest.files.len(), 1);
    }

    #[test]
    fn skip_keeps_existing_files_untouched() {
        let (_dir, mut manifest) = setup(Some(b"old"), 99);
        let outcome = apply_policy(&mut manifest, ExistingPolicy::Skip).unwrap();
        assert_eq!((manifest.files.len(), outcome.skipped_bytes), (0, 99));
    }

    #[test]
    fn update_skips_matching_size_and_mtime() {
        let (_dir, mut manifest) = setup(Some(b"abc"), 3);
        apply_policy(&mut manifest, ExistingPolicy::Update).unwrap();
        assert!(manifest.files.is_empty());
    }

    #[test]
    fn update_copies_when_size_differs() {
        let (_dir, mut manifest) = setup(Some(b"abc"), 4);
        apply_policy(&mut manifest, ExistingPolicy::Update).unwrap();
        assert_eq!(manifest.files.len(), 1);
    }

    #[test]
    fn fail_errors_on_first_existing() {
        let (_dir, mut manifest) = setup(Some(b"x"), 1);
        let result = apply_policy(&mut manifest, ExistingPolicy::Fail);
        assert!(matches!(result, Err(EngineError::DestExists(_))));
    }

    #[test]
    fn fail_passes_when_nothing_exists() {
        let (_dir, mut manifest) = setup(None, 1);
        assert!(apply_policy(&mut manifest, ExistingPolicy::Fail).is_ok());
    }
}
