//! Shared helpers for integration tests.

#![allow(dead_code, reason = "each test binary uses a different subset")]
#![expect(clippy::unwrap_used, reason = "fixtures should fail loudly")]

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use filetime::FileTime;
use tempfile::TempDir;

/// Deterministic pseudo-random bytes (xorshift), so no `rand` dependency is needed.
pub fn content(len: usize, seed: u64) -> Vec<u8> {
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

/// Builds a source tree inside a temporary directory.
#[derive(Default)]
pub struct TreeBuilder {
    files: Vec<(PathBuf, usize, u32)>,
    dirs: Vec<PathBuf>,
    links: Vec<(PathBuf, PathBuf)>,
}

impl TreeBuilder {
    pub fn file(mut self, path: &str, size: usize) -> Self {
        self.files.push((path.into(), size, 0o644));
        self
    }

    pub fn file_with_mode(mut self, path: &str, size: usize, mode: u32) -> Self {
        self.files.push((path.into(), size, mode));
        self
    }

    pub fn dir(mut self, path: &str) -> Self {
        self.dirs.push(path.into());
        self
    }

    pub fn symlink(mut self, path: &str, target: &str) -> Self {
        self.links.push((path.into(), target.into()));
        self
    }

    pub fn build(self) -> TempDir {
        let root = tempfile::tempdir().unwrap();
        for dir in &self.dirs {
            fs::create_dir_all(root.path().join(dir)).unwrap();
        }
        for (seed, (path, size, mode)) in self.files.iter().enumerate() {
            let full = root.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(&full, content(*size, seed as u64)).unwrap();
            fs::set_permissions(&full, fs::Permissions::from_mode(*mode)).unwrap();
            filetime::set_file_mtime(
                &full,
                FileTime::from_unix_time(1_600_000_000 + i64::try_from(seed).unwrap(), 0),
            )
            .unwrap();
        }
        for (path, target) in &self.links {
            let full = root.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            symlink(target, full).unwrap();
        }
        root
    }
}

/// A varied tree: nesting, empty dirs, unicode names, zero-byte and multi-chunk files.
pub fn sample_tree() -> TempDir {
    TreeBuilder::default()
        .file("top.txt", 1234)
        .file("zero.bin", 0)
        .file("nested/a/b/deep.dat", 300_000)
        .file("nested/a/sibling.dat", 65_536)
        .file("unicode/fïlé ünïcødé.txt", 77)
        .file_with_mode("exec.sh", 42, 0o755)
        .file("big/large.bin", 3 * 1024 * 1024 + 11)
        .dir("empty/inner")
        .symlink("link-to-top", "top.txt")
        .symlink("nested/dangling", "does/not/exist")
        .build()
}

/// Asserts `dst` mirrors `src`: same entries, bytes, modes, mtimes and link targets.
pub fn assert_trees_equal(src: &Path, dst: &Path) {
    let mut src_entries = walk(src);
    let mut dst_entries = walk(dst);
    src_entries.sort();
    dst_entries.sort();
    assert_eq!(src_entries, dst_entries, "entry lists differ");

    for rel in &src_entries {
        let (s, d) = (src.join(rel), dst.join(rel));
        let (sm, dm) = (
            fs::symlink_metadata(&s).unwrap(),
            fs::symlink_metadata(&d).unwrap(),
        );
        assert_eq!(sm.file_type(), dm.file_type(), "type of {}", rel.display());
        if sm.file_type().is_symlink() {
            assert_eq!(
                fs::read_link(&s).unwrap(),
                fs::read_link(&d).unwrap(),
                "link {}",
                rel.display()
            );
        } else if sm.is_file() {
            assert!(
                fs::read(&s).unwrap() == fs::read(&d).unwrap(),
                "content of {}",
                rel.display()
            );
            assert_eq!(
                sm.permissions().mode() & 0o777,
                dm.permissions().mode() & 0o777,
                "mode of {}",
                rel.display()
            );
            assert_eq!(
                FileTime::from_last_modification_time(&sm),
                FileTime::from_last_modification_time(&dm),
                "mtime of {}",
                rel.display()
            );
        }
    }
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
            if fs::symlink_metadata(&path).unwrap().is_dir() {
                stack.push(path);
            }
        }
    }
    out
}
