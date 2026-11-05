//! `FS_IOC_FIEMAP`: ask the filesystem where a file's first extent lives.

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

use super::ExtentLocator;
use crate::platform::ffi::{cvt, is_unsupported};

/// `_IOWR('f', 11, struct fiemap)` from `<linux/fs.h>`.
const FS_IOC_FIEMAP: u64 = 0xC020_660B;

/// `struct fiemap_extent` from `<linux/fiemap.h>`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
#[expect(clippy::struct_field_names, reason = "mirrors the kernel ABI names")]
struct FiemapExtent {
    fe_logical: u64,
    fe_physical: u64,
    fe_length: u64,
    fe_reserved64: [u64; 2],
    fe_flags: u32,
    fe_reserved: [u32; 3],
}

/// `struct fiemap` followed by room for exactly one extent.
#[repr(C)]
#[derive(Debug, Default)]
#[expect(clippy::struct_field_names, reason = "mirrors the kernel ABI names")]
struct FiemapRequest {
    fm_start: u64,
    fm_length: u64,
    fm_flags: u32,
    fm_mapped_extents: u32,
    fm_extent_count: u32,
    fm_reserved: u32,
    fm_extents: [FiemapExtent; 1],
}

// The kernel ABI is fixed; catch accidental layout changes at compile time.
const _: () = assert!(size_of::<FiemapExtent>() == 56);
const _: () = assert!(size_of::<FiemapRequest>() == 32 + 56);

/// FIEMAP-backed locator (ext4, xfs, btrfs, ntfs3, …).
#[derive(Debug, Clone, Copy, Default)]
pub struct Fiemap;

impl ExtentLocator for Fiemap {
    fn first_physical_offset(&self, path: &Path) -> io::Result<Option<u64>> {
        let file = File::open(path)?;
        let mut request = FiemapRequest {
            fm_length: u64::MAX,
            fm_extent_count: 1,
            ..FiemapRequest::default()
        };

        // SAFETY: `request` is a properly aligned, `#[repr(C)]` `struct fiemap` with
        // `fm_extent_count = 1` and space for exactly one extent, so the kernel writes
        // within its bounds. The fd is owned by `file`, which outlives the call.
        let ret = unsafe { libc::ioctl(file.as_raw_fd(), FS_IOC_FIEMAP as _, &raw mut request) };

        match cvt(ret) {
            Ok(_) if request.fm_mapped_extents > 0 => Ok(Some(request.fm_extents[0].fe_physical)),
            Ok(_) => Ok(None),
            Err(err) if is_unsupported(&err) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiemap_on_tempfile_does_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, vec![1u8; 64 * 1024]).unwrap();
        // tmpfs has no FIEMAP, so `None` is acceptable; an error is not.
        assert!(Fiemap.first_physical_offset(&path).is_ok());
    }

    #[test]
    fn fiemap_on_missing_file_errors() {
        assert!(
            Fiemap
                .first_physical_offset(Path::new("/definitely/missing"))
                .is_err()
        );
    }
}
