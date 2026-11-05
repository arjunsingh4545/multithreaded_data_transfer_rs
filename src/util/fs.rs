//! Path and filesystem helpers.

use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Returns the closest ancestor of `path` (including `path` itself) that exists.
///
/// The result borrows from `path`, so no allocation happens.
///
/// ```
/// use multithreaded_data_transfer_rs::util::fs::nearest_existing;
///
/// let tmp = std::env::temp_dir();
/// let missing = tmp.join("fastcp-doc-missing/a/b");
/// assert_eq!(nearest_existing(&missing), Some(tmp.as_path()));
/// ```
pub fn nearest_existing(path: &Path) -> Option<&Path> {
    path.ancestors()
        .filter(|p| !p.as_os_str().is_empty())
        .find(|p| p.symlink_metadata().is_ok())
}

/// Canonicalizes the existing prefix of `path` and re-appends the missing tail.
///
/// Unlike [`fs::canonicalize`] this works for destinations that do not exist yet.
pub fn canonicalize_partial(path: &Path) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let existing = nearest_existing(&absolute)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no existing ancestor"))?;
    let tail = absolute.strip_prefix(existing).map_err(io::Error::other)?;
    Ok(fs::canonicalize(existing)?.join(tail))
}

/// `true` when `child` equals `parent` or lies below it. Both paths should be canonical.
pub fn is_within(child: &Path, parent: &Path) -> bool {
    child.starts_with(parent)
}

/// Maps `path` (somewhere under `from`) to the same relative location under `to`.
pub fn rebase(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|rel| to.join(rel))
}

/// The file name for progress output, falling back to the full path.
pub fn display_name(path: &Path) -> Cow<'_, str> {
    path.file_name()
        .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
}

/// Reads a small text file (sysfs attribute style) and trims surrounding whitespace.
pub fn read_trimmed(path: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_owned())
}

/// Deletes a file when dropped, unless [`disarm`](Self::disarm) was called first.
///
/// Used to clean up partially written destination files on error or cancellation.
#[derive(Debug)]
#[must_use = "dropping the guard immediately deletes the file"]
pub struct RemoveOnDrop<'a> {
    path: &'a Path,
    armed: bool,
}

impl<'a> RemoveOnDrop<'a> {
    /// Arms a guard for `path`.
    pub const fn new(path: &'a Path) -> Self {
        Self { path, armed: true }
    }

    /// Keeps the file: the guard will no longer delete it.
    pub fn disarm(&mut self) {
        self.armed = false;
    }

    /// The guarded path.
    pub const fn path(&self) -> &'a Path {
        self.path
    }
}

impl Drop for RemoveOnDrop<'_> {
    fn drop(&mut self) {
        if self.armed {
            // Best effort: the file may never have been created.
            fs::remove_file(self.path).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_existing_walks_up_to_parent() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("a/b/c");
        assert_eq!(nearest_existing(&missing), Some(dir.path()));
    }

    #[test]
    fn canonicalize_partial_keeps_missing_tail() {
        let dir = tempfile::tempdir().unwrap();
        let resolved = canonicalize_partial(&dir.path().join("new/leaf")).unwrap();
        let expected = fs::canonicalize(dir.path()).unwrap().join("new/leaf");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn is_within_detects_nested_dst() {
        assert!(is_within(
            Path::new("/data/src/backup"),
            Path::new("/data/src")
        ));
    }

    #[test]
    fn is_within_rejects_sibling_with_common_prefix() {
        assert!(!is_within(Path::new("/data/src2"), Path::new("/data/src")));
    }

    #[test]
    fn rebase_maps_relative_location() {
        let out = rebase(Path::new("/a/x/y.txt"), Path::new("/a"), Path::new("/b"));
        assert_eq!(out, Some(PathBuf::from("/b/x/y.txt")));
    }

    #[test]
    fn display_name_uses_file_name() {
        assert_eq!(display_name(Path::new("/a/b/photo.jpg")), "photo.jpg");
    }

    #[test]
    fn read_trimmed_strips_newline() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("rotational");
        fs::write(&file, "1\n").unwrap();
        assert_eq!(read_trimmed(&file).unwrap(), "1");
    }

    #[test]
    fn remove_on_drop_deletes_when_armed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("partial");
        fs::write(&file, b"x").unwrap();
        drop(RemoveOnDrop::new(&file));
        assert!(!file.exists());
    }

    #[test]
    fn remove_on_drop_keeps_file_when_disarmed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("done");
        fs::write(&file, b"x").unwrap();
        let mut guard = RemoveOnDrop::new(&file);
        guard.disarm();
        drop(guard);
        assert!(file.exists());
    }
}
