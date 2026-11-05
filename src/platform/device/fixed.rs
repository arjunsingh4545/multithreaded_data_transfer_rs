//! A probe with pre-set answers: for tests and platforms without sysfs.

use std::path::{Path, PathBuf};

use super::{DeviceProbe, ProbeError};
use crate::domain::device::{DeviceId, DiskInfo, DiskKind};

/// Answers from a list of path-prefix rules, falling back to one fixed disk.
///
/// The longest matching prefix wins.
///
/// ```
/// use multithreaded_data_transfer_rs::DiskKind;
/// use multithreaded_data_transfer_rs::domain::device::{DeviceId, DiskInfo};
/// use multithreaded_data_transfer_rs::platform::device::{DeviceProbe, FixedProbe};
///
/// let probe = FixedProbe::uniform(DiskKind::Ssd)
///     .with_rule("/mnt/usb", DiskInfo::new(DeviceId::new(8, 16), "sdb", DiskKind::Hdd));
/// assert_eq!(probe.probe("/mnt/usb/photos".as_ref())?.kind, DiskKind::Hdd);
/// assert_eq!(probe.probe("/home".as_ref())?.kind, DiskKind::Ssd);
/// # Ok::<(), multithreaded_data_transfer_rs::platform::device::ProbeError>(())
/// ```
#[derive(Debug, Clone)]
pub struct FixedProbe {
    fallback: DiskInfo,
    rules: Vec<(PathBuf, DiskInfo)>,
}

impl FixedProbe {
    /// Every path reports the same disk of `kind`.
    pub fn uniform(kind: DiskKind) -> Self {
        Self {
            fallback: DiskInfo::new(DeviceId::default(), "fixed", kind),
            rules: Vec::new(),
        }
    }

    /// Paths under `prefix` report `info`.
    #[must_use]
    pub fn with_rule(mut self, prefix: impl Into<PathBuf>, info: DiskInfo) -> Self {
        self.rules.push((prefix.into(), info));
        self
    }
}

impl Default for FixedProbe {
    fn default() -> Self {
        Self::uniform(DiskKind::Unknown)
    }
}

impl DeviceProbe for FixedProbe {
    fn probe(&self, path: &Path) -> Result<DiskInfo, ProbeError> {
        Ok(self
            .rules
            .iter()
            .filter(|(prefix, _)| path.starts_with(prefix))
            .max_by_key(|(prefix, _)| prefix.components().count())
            .map_or_else(|| self.fallback.clone(), |(_, info)| info.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_prefix_wins() {
        let probe = FixedProbe::default()
            .with_rule(
                "/a",
                DiskInfo::new(DeviceId::new(1, 0), "outer", DiskKind::Ssd),
            )
            .with_rule(
                "/a/b",
                DiskInfo::new(DeviceId::new(2, 0), "inner", DiskKind::Hdd),
            );
        assert_eq!(probe.probe(Path::new("/a/b/c")).unwrap().name, "inner");
    }

    #[test]
    fn unmatched_path_uses_fallback() {
        let probe = FixedProbe::uniform(DiskKind::Hdd);
        assert_eq!(probe.probe(Path::new("/x")).unwrap().kind, DiskKind::Hdd);
    }
}
