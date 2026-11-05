//! Linux disk detection through `/sys`.
//!
//! `/sys/dev/block/<major>:<minor>` links to the device directory. Partitions
//! (directories containing a `partition` file) are resolved to their parent
//! disk, and stacked devices (dm-crypt, LVM, md) are resolved through
//! `slaves/`. This matters because those virtual devices often report
//! `rotational = 0` even when they sit on top of a spinning disk.

use std::fs;
use std::path::{Path, PathBuf};

use super::{DeviceProbe, ProbeError, device_of};
use crate::domain::device::{DeviceId, DiskInfo, DiskKind};
use crate::util::fs::read_trimmed;
use crate::util::units::ByteSize;

/// Guards against pathological `slaves/` cycles.
const MAX_STACK_DEPTH: u8 = 8;

/// Reads disk information from a sysfs tree.
#[derive(Debug, Clone)]
pub struct SysfsProbe {
    root: PathBuf,
}

impl Default for SysfsProbe {
    fn default() -> Self {
        Self::with_root("/sys")
    }
}

impl SysfsProbe {
    /// Uses `root` instead of `/sys` (tests point this at a fake tree).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Describes the disk owning `dev`, or `None` if sysfs does not know it
    /// (tmpfs, NFS, btrfs anonymous devices…).
    pub fn resolve(&self, dev: DeviceId) -> Result<Option<DiskInfo>, ProbeError> {
        let link = self.root.join("dev/block").join(dev.to_string());
        let Ok(dir) = fs::canonicalize(link) else {
            return Ok(None);
        };
        describe(whole_disk(&dir)).map(Some)
    }
}

impl DeviceProbe for SysfsProbe {
    fn probe(&self, path: &Path) -> Result<DiskInfo, ProbeError> {
        let dev = device_of(path)?;
        Ok(self.resolve(dev)?.unwrap_or_else(|| DiskInfo::unknown(dev)))
    }
}

/// Reads the attributes of a whole-disk directory.
fn describe(disk: &Path) -> Result<DiskInfo, ProbeError> {
    let dev_file = disk.join("dev");
    let raw = read_trimmed(&dev_file).map_err(|source| ProbeError::Sysfs {
        path: dev_file.clone(),
        source,
    })?;
    let id = raw.parse().map_err(|_| ProbeError::InvalidValue {
        path: dev_file,
        value: raw,
    })?;

    Ok(DiskInfo {
        id,
        name: disk
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        kind: kind_of(disk, 0)?,
        max_request: read_optional(&disk.join("queue/max_sectors_kb"))
            .and_then(|kb| kb.parse().ok())
            .map(ByteSize::kib),
    })
}

/// Rotational flag of `disk`, resolving stacked devices through their members.
fn kind_of(disk: &Path, depth: u8) -> Result<DiskKind, ProbeError> {
    let slaves = slaves(disk);
    if !slaves.is_empty() && depth < MAX_STACK_DEPTH {
        let kinds = slaves
            .iter()
            .map(|slave| kind_of(whole_disk(slave), depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(combine(&kinds));
    }

    let flag_file = disk.join("queue/rotational");
    match read_optional(&flag_file) {
        None => Ok(DiskKind::Unknown),
        Some(flag) => DiskKind::try_from(flag.as_str()).map_err(|_| ProbeError::InvalidValue {
            path: flag_file,
            value: flag,
        }),
    }
}

/// A partition directory resolves to its parent disk.
fn whole_disk(dir: &Path) -> &Path {
    if dir.join("partition").is_file() {
        dir.parent().unwrap_or(dir)
    } else {
        dir
    }
}

/// Canonical directories of the devices under `disk/slaves`.
fn slaves(disk: &Path) -> Vec<PathBuf> {
    fs::read_dir(disk.join("slaves"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| fs::canonicalize(entry.path()).ok())
        .collect()
}

/// A stacked device is rotational if any member is.
fn combine(kinds: &[DiskKind]) -> DiskKind {
    if kinds.contains(&DiskKind::Hdd) {
        DiskKind::Hdd
    } else if !kinds.is_empty() && kinds.iter().all(|k| *k == DiskKind::Ssd) {
        DiskKind::Ssd
    } else {
        DiskKind::Unknown
    }
}

fn read_optional(path: &Path) -> Option<String> {
    read_trimmed(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    /// Builds a miniature `/sys` with the same shape as the real one.
    struct FakeSysfs {
        root: TempDir,
    }

    impl FakeSysfs {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir_all(root.path().join("dev/block")).unwrap();
            Self { root }
        }

        fn probe(&self) -> SysfsProbe {
            SysfsProbe::with_root(self.root.path())
        }

        fn write(&self, rel: &str, contents: &str) {
            let path = self.root.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }

        fn link(&self, dev: &str, rel_dir: &str) {
            symlink(
                self.root.path().join(rel_dir),
                self.root.path().join("dev/block").join(dev),
            )
            .unwrap();
        }

        /// A whole disk with `rotational` set (or absent when `None`).
        fn disk(&self, rel_dir: &str, dev: &str, rotational: Option<&str>) {
            self.write(&format!("{rel_dir}/dev"), &format!("{dev}\n"));
            if let Some(flag) = rotational {
                self.write(&format!("{rel_dir}/queue/rotational"), &format!("{flag}\n"));
            }
            self.link(dev, rel_dir);
        }

        fn partition(&self, rel_dir: &str, dev: &str) {
            self.write(&format!("{rel_dir}/dev"), dev);
            self.write(&format!("{rel_dir}/partition"), "1");
            self.link(dev, rel_dir);
        }

        fn slave(&self, holder_dir: &str, name: &str, target_dir: &str) {
            let slaves = self.root.path().join(holder_dir).join("slaves");
            fs::create_dir_all(&slaves).unwrap();
            symlink(self.root.path().join(target_dir), slaves.join(name)).unwrap();
        }

        fn resolve(&self, major: u32, minor: u32) -> Option<DiskInfo> {
            self.probe().resolve(DeviceId::new(major, minor)).unwrap()
        }
    }

    #[test]
    fn resolves_whole_disk() {
        let sys = FakeSysfs::new();
        sys.disk("devices/pci/sda", "8:0", Some("1"));
        assert_eq!(sys.resolve(8, 0).unwrap().kind, DiskKind::Hdd);
    }

    #[test]
    fn resolves_partition_to_parent_disk() {
        let sys = FakeSysfs::new();
        sys.disk("devices/pci/sda", "8:0", Some("1"));
        sys.partition("devices/pci/sda/sda1", "8:1");
        let info = sys.resolve(8, 1).unwrap();
        assert_eq!((info.name.as_str(), info.id), ("sda", DeviceId::new(8, 0)));
    }

    #[test]
    fn resolves_nvme_partition() {
        let sys = FakeSysfs::new();
        sys.disk("devices/pci/nvme/nvme0n1", "259:0", Some("0"));
        sys.partition("devices/pci/nvme/nvme0n1/nvme0n1p2", "259:2");
        assert_eq!(sys.resolve(259, 2).unwrap().kind, DiskKind::Ssd);
    }

    #[test]
    fn dm_device_with_hdd_slave_is_hdd() {
        let sys = FakeSysfs::new();
        sys.disk("devices/pci/sda", "8:0", Some("1"));
        sys.partition("devices/pci/sda/sda2", "8:2");
        // dm devices lie: rotational=0 even on top of a spinning disk.
        sys.disk("devices/virtual/block/dm-0", "253:0", Some("0"));
        sys.slave("devices/virtual/block/dm-0", "sda2", "devices/pci/sda/sda2");
        assert_eq!(sys.resolve(253, 0).unwrap().kind, DiskKind::Hdd);
    }

    #[test]
    fn missing_rotational_is_unknown() {
        let sys = FakeSysfs::new();
        sys.disk("devices/virtual/block/loop0", "7:0", None);
        assert_eq!(sys.resolve(7, 0).unwrap().kind, DiskKind::Unknown);
    }

    #[test]
    fn reads_max_request_size() {
        let sys = FakeSysfs::new();
        sys.disk("devices/pci/sda", "8:0", Some("1"));
        sys.write("devices/pci/sda/queue/max_sectors_kb", "4096\n");
        assert_eq!(
            sys.resolve(8, 0).unwrap().max_request,
            Some(ByteSize::mib(4))
        );
    }

    #[test]
    fn unknown_device_resolves_to_none() {
        let sys = FakeSysfs::new();
        assert_eq!(sys.resolve(0, 42), None);
    }

    #[test]
    fn garbage_rotational_is_an_error() {
        let sys = FakeSysfs::new();
        sys.disk("devices/pci/sda", "8:0", Some("maybe"));
        assert!(sys.probe().resolve(DeviceId::new(8, 0)).is_err());
    }

    #[test]
    fn probing_a_real_path_does_not_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(SysfsProbe::default().probe(dir.path()).is_ok());
    }
}
