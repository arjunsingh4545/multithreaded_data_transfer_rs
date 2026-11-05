//! Kernel I/O hints: readahead, preallocation and paced writeback.
//!
//! Every method has a no-op default, so a "no hints" implementation is a
//! one-liner and tests can override only what they observe.

#[cfg(target_os = "linux")]
pub mod linux;

use std::fs::File;
use std::io;
use std::ops::Range;

#[cfg(target_os = "linux")]
pub use linux::LinuxHints;

/// The hints implementation used by default on this platform.
#[cfg(target_os = "linux")]
pub type PlatformHints = LinuxHints;
/// The hints implementation used by default on this platform.
#[cfg(not(target_os = "linux"))]
pub type PlatformHints = NoHints;

/// Best-effort performance hints. Callers ignore "unsupported" errors.
pub trait IoHints: Sync {
    /// The file will be read sequentially: enlarge readahead.
    fn advise_sequential(&self, _file: &File) -> io::Result<()> {
        Ok(())
    }

    /// Reserve `len` bytes up front so the filesystem can allocate contiguously.
    fn preallocate(&self, _file: &File, _len: u64) -> io::Result<()> {
        Ok(())
    }

    /// Start writing back dirty pages in `range` without waiting.
    fn start_writeback(&self, _file: &File, _range: Range<u64>) -> io::Result<()> {
        Ok(())
    }

    /// Wait for `range` to reach the disk, then drop it from the page cache.
    fn settle_and_evict(&self, _file: &File, _range: Range<u64>) -> io::Result<()> {
        Ok(())
    }

    /// Drop clean pages in `range` from the page cache.
    fn evict(&self, _file: &File, _range: Range<u64>) -> io::Result<()> {
        Ok(())
    }
}

impl<H: IoHints + ?Sized> IoHints for &H {
    fn advise_sequential(&self, file: &File) -> io::Result<()> {
        (**self).advise_sequential(file)
    }
    fn preallocate(&self, file: &File, len: u64) -> io::Result<()> {
        (**self).preallocate(file, len)
    }
    fn start_writeback(&self, file: &File, range: Range<u64>) -> io::Result<()> {
        (**self).start_writeback(file, range)
    }
    fn settle_and_evict(&self, file: &File, range: Range<u64>) -> io::Result<()> {
        (**self).settle_and_evict(file, range)
    }
    fn evict(&self, file: &File, range: Range<u64>) -> io::Result<()> {
        (**self).evict(file, range)
    }
}

/// Issues no hints at all.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHints;

impl IoHints for NoHints {}
