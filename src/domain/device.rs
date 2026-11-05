//! Block-device descriptions (data only; detection lives in `platform::device`).

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::util::units::ByteSize;

/// What kind of storage backs a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DiskKind {
    /// Solid-state (`NVMe`, SATA SSD, RAM-backed…): seeks are cheap.
    Ssd,
    /// Rotational hard disk: seeks are expensive, sequential I/O is king.
    Hdd,
    /// Could not be determined (network FS, tmpfs, btrfs anonymous devices…).
    #[default]
    Unknown,
}

impl DiskKind {
    /// `true` for spinning disks.
    pub const fn is_rotational(self) -> bool {
        matches!(self, Self::Hdd)
    }

    /// The kind used for planning: unknown storage is treated like an SSD,
    /// which matches the tool's historical (fully parallel) behaviour.
    #[must_use]
    pub const fn effective(self) -> Self {
        match self {
            Self::Unknown => Self::Ssd,
            known => known,
        }
    }
}

impl fmt::Display for DiskKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ssd => "SSD",
            Self::Hdd => "HDD",
            Self::Unknown => "unknown",
        })
    }
}

/// A sysfs `queue/rotational` value that is neither `0` nor `1`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unexpected rotational flag `{0}`")]
pub struct ParseDiskKindError(String);

impl TryFrom<&str> for DiskKind {
    type Error = ParseDiskKindError;

    /// Parses a sysfs `queue/rotational` value.
    fn try_from(flag: &str) -> Result<Self, Self::Error> {
        match flag.trim() {
            "0" => Ok(Self::Ssd),
            "1" => Ok(Self::Hdd),
            other => Err(ParseDiskKindError(other.to_owned())),
        }
    }
}

/// A Linux `major:minor` device number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DeviceId {
    major: u32,
    minor: u32,
}

impl DeviceId {
    /// Builds an id from its parts.
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    /// Major number (driver).
    pub const fn major(self) -> u32 {
        self.major
    }

    /// Minor number (instance / partition).
    pub const fn minor(self) -> u32 {
        self.minor
    }
}

impl From<u64> for DeviceId {
    /// Decodes a `st_dev` value using the glibc encoding from `<sys/sysmacros.h>`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "both values are masked to 32 bits"
    )]
    fn from(dev: u64) -> Self {
        let major = ((dev >> 8) & 0x0fff) | ((dev >> 32) & 0xffff_f000);
        let minor = (dev & 0x00ff) | ((dev >> 12) & 0xffff_ff00);
        Self::new(major as u32, minor as u32)
    }
}

/// A string that is not of the form `major:minor`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("expected `major:minor`, got `{0}`")]
pub struct ParseDeviceIdError(String);

impl FromStr for DeviceId {
    type Err = ParseDeviceIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseDeviceIdError(s.to_owned());
        let (major, minor) = s.trim().split_once(':').ok_or_else(err)?;
        Ok(Self::new(
            major.parse().map_err(|_| err())?,
            minor.parse().map_err(|_| err())?,
        ))
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.major, self.minor)
    }
}

/// A whole physical (or virtual) disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskInfo {
    /// Id of the whole disk, not the partition.
    pub id: DeviceId,
    /// Kernel name such as `sda` or `nvme0n1`.
    pub name: String,
    /// Storage kind.
    pub kind: DiskKind,
    /// Largest single request the device accepts (`queue/max_sectors_kb`).
    pub max_request: Option<ByteSize>,
}

impl DiskInfo {
    /// A disk with no request-size information.
    pub fn new(id: DeviceId, name: impl Into<String>, kind: DiskKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            max_request: None,
        }
    }

    /// A device that could not be identified.
    pub fn unknown(id: DeviceId) -> Self {
        Self::new(id, "unknown", DiskKind::Unknown)
    }

    /// Replaces the kind (used for user overrides).
    #[must_use]
    pub fn with_kind(mut self, kind: DiskKind) -> Self {
        self.kind = kind;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod disk_kind {
        use super::*;

        #[test]
        fn rotational_one_is_hdd() {
            assert_eq!(DiskKind::try_from("1\n"), Ok(DiskKind::Hdd));
        }

        #[test]
        fn rotational_zero_is_ssd() {
            assert_eq!(DiskKind::try_from("0"), Ok(DiskKind::Ssd));
        }

        #[test]
        fn rotational_garbage_errors() {
            assert!(DiskKind::try_from("yes").is_err());
        }

        #[test]
        fn unknown_is_effectively_ssd() {
            assert_eq!(DiskKind::Unknown.effective(), DiskKind::Ssd);
        }
    }

    mod device_id {
        use super::*;

        #[test]
        fn parses_major_minor() {
            assert_eq!("8:17".parse(), Ok(DeviceId::new(8, 17)));
        }

        #[test]
        fn rejects_missing_colon() {
            assert!("817".parse::<DeviceId>().is_err());
        }

        #[test]
        fn displays_as_major_minor() {
            assert_eq!(DeviceId::new(259, 3).to_string(), "259:3");
        }

        #[test]
        fn from_dev_t_matches_libc() {
            let dev = libc::makedev(259, 70_000);
            let id = DeviceId::from(dev);
            assert_eq!(
                (id.major(), id.minor()),
                (libc::major(dev), libc::minor(dev))
            );
        }
    }

    #[test]
    fn with_kind_overrides_kind() {
        let info = DiskInfo::unknown(DeviceId::default()).with_kind(DiskKind::Hdd);
        assert_eq!(info.kind, DiskKind::Hdd);
    }
}
