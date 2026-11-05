//! Disk detection: which physical disk backs a path, and is it rotational?

pub mod fixed;
#[cfg(target_os = "linux")]
pub mod sysfs;

use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::domain::device::{DeviceId, DiskInfo};

pub use fixed::FixedProbe;
#[cfg(target_os = "linux")]
pub use sysfs::SysfsProbe;

/// The probe used by default on this platform.
#[cfg(target_os = "linux")]
pub type PlatformProbe = SysfsProbe;
/// The probe used by default on this platform.
#[cfg(not(target_os = "linux"))]
pub type PlatformProbe = FixedProbe;

/// Why a disk could not be described.
#[derive(Debug, Error)]
pub enum ProbeError {
    /// The path itself could not be inspected.
    #[error("cannot inspect '{path}': {source}")]
    Stat {
        /// Path being probed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// A sysfs attribute could not be read.
    #[error("cannot read sysfs attribute '{path}': {source}")]
    Sysfs {
        /// Attribute path.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// A sysfs attribute had an unexpected value.
    #[error("unexpected value `{value}` in '{path}'")]
    InvalidValue {
        /// Attribute path.
        path: PathBuf,
        /// What was read.
        value: String,
    },
}

/// Describes the disk behind a path.
pub trait DeviceProbe {
    /// Returns the whole-disk description for an existing `path`.
    fn probe(&self, path: &Path) -> Result<DiskInfo, ProbeError>;
}

impl<D: DeviceProbe + ?Sized> DeviceProbe for &D {
    fn probe(&self, path: &Path) -> Result<DiskInfo, ProbeError> {
        (**self).probe(path)
    }
}

/// The `st_dev` of `path`, decoded.
pub fn device_of(path: &Path) -> Result<DeviceId, ProbeError> {
    fs::metadata(path)
        .map(|meta| DeviceId::from(meta.dev()))
        .map_err(|source| ProbeError::Stat {
            path: path.to_path_buf(),
            source,
        })
}
