//! Shared fixtures for engine unit tests.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use crate::domain::job::{FileJob, FileMeta};

/// Deterministic, position-dependent bytes so misplaced chunks are detected.
pub(crate) fn pattern(len: u64, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

/// Source files plus matching jobs targeting an empty destination directory.
pub(crate) struct Fixture {
    pub(crate) _src: TempDir,
    pub(crate) dst: TempDir,
    pub(crate) jobs: Vec<FileJob>,
}

impl Fixture {
    pub(crate) fn with_sizes(sizes: &[u64]) -> Self {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let jobs = sizes
            .iter()
            .enumerate()
            .map(|(i, &size)| {
                let name = format!("file-{i}.bin");
                let src_path = src.path().join(&name);
                fs::write(&src_path, pattern(size, i as u64)).unwrap();
                let meta = fs::metadata(&src_path).unwrap();
                FileJob {
                    dst: dst.path().join(&name),
                    src: src_path,
                    size,
                    ino: 0,
                    meta: FileMeta::from(&meta),
                }
            })
            .collect();
        Self {
            _src: src,
            dst,
            jobs,
        }
    }

    pub(crate) fn dst_is_empty(&self) -> bool {
        is_empty_dir(self.dst.path())
    }
}

fn is_empty_dir(dir: &Path) -> bool {
    fs::read_dir(dir).unwrap().next().is_none()
}

/// Every destination file matches its source byte for byte.
pub(crate) fn assert_copied(fx: &Fixture) {
    for job in &fx.jobs {
        let src = fs::read(&job.src).unwrap();
        let dst = fs::read(&job.dst).unwrap_or_else(|e| panic!("{}: {e}", job.dst.display()));
        assert!(src == dst, "content mismatch for {}", job.dst.display());
    }
}
