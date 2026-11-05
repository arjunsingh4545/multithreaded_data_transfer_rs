//! Physical placement of file data on disk.

#[cfg(target_os = "linux")]
pub mod fiemap;

use std::io;
use std::path::Path;

#[cfg(target_os = "linux")]
pub use fiemap::Fiemap;

/// The locator used by default on this platform.
#[cfg(target_os = "linux")]
pub type PlatformLocator = Fiemap;
/// The locator used by default on this platform.
#[cfg(not(target_os = "linux"))]
pub type PlatformLocator = NoLocator;

/// Finds where a file's data starts on the underlying device.
pub trait ExtentLocator {
    /// Byte offset of the file's first extent on disk, or `None` when unknown
    /// (empty file, inline data, or a filesystem without extent mapping).
    fn first_physical_offset(&self, path: &Path) -> io::Result<Option<u64>>;
}

impl<L: ExtentLocator + ?Sized> ExtentLocator for &L {
    fn first_physical_offset(&self, path: &Path) -> io::Result<Option<u64>> {
        (**self).first_physical_offset(path)
    }
}

/// Never knows anything; physical ordering then degrades to path ordering.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoLocator;

impl ExtentLocator for NoLocator {
    fn first_physical_offset(&self, _path: &Path) -> io::Result<Option<u64>> {
        Ok(None)
    }
}
