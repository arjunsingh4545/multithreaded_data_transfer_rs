//! Linux implementation: `posix_fadvise`, `fallocate`, `sync_file_range`.

use std::fs::File;
use std::io;
use std::ops::Range;
use std::os::fd::AsRawFd;

use super::IoHints;
use crate::platform::ffi::{cvt, cvt_errno, to_off};

/// Hints backed by real Linux syscalls.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinuxHints;

fn fadvise(file: &File, range: &Range<u64>, advice: libc::c_int) -> io::Result<()> {
    let offset = to_off(range.start)?;
    let len = to_off(range.end.saturating_sub(range.start))?;
    // SAFETY: the fd is owned by `file`, which stays borrowed (and therefore open)
    // for the duration of the call. posix_fadvise does not touch user memory.
    cvt_errno(unsafe { libc::posix_fadvise(file.as_raw_fd(), offset, len, advice) })
}

fn sync_range(file: &File, range: &Range<u64>, flags: libc::c_uint) -> io::Result<()> {
    let offset = to_off(range.start)?;
    let len = to_off(range.end.saturating_sub(range.start))?;
    // SAFETY: as above — `file` keeps the fd valid; no user memory is passed.
    cvt(unsafe { libc::sync_file_range(file.as_raw_fd(), offset, len, flags) }).map(drop)
}

impl IoHints for LinuxHints {
    fn advise_sequential(&self, file: &File) -> io::Result<()> {
        // A zero length means "to the end of the file".
        fadvise(file, &(0..0), libc::POSIX_FADV_SEQUENTIAL)
    }

    fn preallocate(&self, file: &File, len: u64) -> io::Result<()> {
        if len == 0 {
            return Ok(());
        }
        let len = to_off(len)?;
        // SAFETY: `file` keeps the fd valid; mode 0 allocates and extends the file size,
        // which `PendingFile::finish` later trims to the bytes actually written.
        cvt(unsafe { libc::fallocate(file.as_raw_fd(), 0, 0, len) }).map(drop)
    }

    fn start_writeback(&self, file: &File, range: Range<u64>) -> io::Result<()> {
        sync_range(file, &range, libc::SYNC_FILE_RANGE_WRITE)
    }

    fn settle_and_evict(&self, file: &File, range: Range<u64>) -> io::Result<()> {
        sync_range(
            file,
            &range,
            libc::SYNC_FILE_RANGE_WAIT_BEFORE
                | libc::SYNC_FILE_RANGE_WRITE
                | libc::SYNC_FILE_RANGE_WAIT_AFTER,
        )?;
        fadvise(file, &range, libc::POSIX_FADV_DONTNEED)
    }

    fn evict(&self, file: &File, range: Range<u64>) -> io::Result<()> {
        fadvise(file, &range, libc::POSIX_FADV_DONTNEED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::ffi::is_unsupported;
    use std::os::unix::fs::FileExt;

    /// Hints are best-effort; on tmpfs some calls are unsupported, which is fine.
    fn ok_or_unsupported(result: &io::Result<()>) -> bool {
        result.as_ref().map_or_else(is_unsupported, |()| true)
    }

    #[test]
    fn advise_sequential_succeeds_on_tempfile() {
        let file = tempfile::tempfile().unwrap();
        assert!(ok_or_unsupported(&LinuxHints.advise_sequential(&file)));
    }

    #[test]
    fn preallocate_extends_file() {
        let file = tempfile::tempfile().unwrap();
        let result = LinuxHints.preallocate(&file, 1 << 20);
        if result.is_ok() {
            assert_eq!(file.metadata().unwrap().len(), 1 << 20);
        } else {
            assert!(ok_or_unsupported(&result));
        }
    }

    #[test]
    fn writeback_cycle_succeeds() {
        let file = tempfile::tempfile().unwrap();
        file.write_all_at(&[7u8; 8192], 0).unwrap();
        assert!(ok_or_unsupported(
            &LinuxHints.start_writeback(&file, 0..8192)
        ));
        assert!(ok_or_unsupported(
            &LinuxHints.settle_and_evict(&file, 0..8192)
        ));
    }
}
