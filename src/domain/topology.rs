//! Which disks a copy runs between, and what that implies.

use std::fmt;

use super::device::{DiskInfo, DiskKind};

/// User-supplied disk kinds that replace detection results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindOverrides {
    /// Override for the source disk.
    pub src: Option<DiskKind>,
    /// Override for the destination disk.
    pub dst: Option<DiskKind>,
}

/// The disks on each side of a copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topology {
    /// Disk holding the source tree.
    pub src: DiskInfo,
    /// Disk holding (the nearest existing parent of) the destination.
    pub dst: DiskInfo,
}

impl Topology {
    /// Combines probe results with user overrides.
    pub fn new(src: DiskInfo, dst: DiskInfo, overrides: KindOverrides) -> Self {
        let apply = |info: DiskInfo, kind: Option<DiskKind>| match kind {
            Some(kind) => info.with_kind(kind),
            None => info,
        };
        Self {
            src: apply(src, overrides.src),
            dst: apply(dst, overrides.dst),
        }
    }

    /// `true` when both paths live on the same physical disk.
    pub fn same_disk(&self) -> bool {
        self.src.id == self.dst.id
    }

    /// Classifies the copy.
    pub fn transfer(&self) -> TransferKind {
        TransferKind::classify(self.src.kind, self.dst.kind, self.same_disk())
    }
}

/// The kind of copy, which drives strategy selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferKind {
    /// Solid-state on both sides.
    SsdToSsd,
    /// Fast source, rotational destination.
    SsdToHdd,
    /// Rotational source, fast destination.
    HddToSsd,
    /// Two different rotational disks.
    HddToHdd,
    /// Source and destination on the same rotational disk.
    SameHdd,
}

impl TransferKind {
    /// Classifies a pair of disk kinds. `Unknown` is treated as `Ssd`.
    ///
    /// ```
    /// use multithreaded_data_transfer_rs::{DiskKind, TransferKind};
    ///
    /// assert_eq!(
    ///     TransferKind::classify(DiskKind::Hdd, DiskKind::Hdd, false),
    ///     TransferKind::HddToHdd,
    /// );
    /// assert_eq!(
    ///     TransferKind::classify(DiskKind::Unknown, DiskKind::Hdd, false),
    ///     TransferKind::SsdToHdd,
    /// );
    /// ```
    pub const fn classify(src: DiskKind, dst: DiskKind, same_disk: bool) -> Self {
        match (src.effective(), dst.effective(), same_disk) {
            (DiskKind::Hdd, DiskKind::Hdd, true) => Self::SameHdd,
            (DiskKind::Hdd, DiskKind::Hdd, false) => Self::HddToHdd,
            (DiskKind::Hdd, _, _) => Self::HddToSsd,
            (_, DiskKind::Hdd, _) => Self::SsdToHdd,
            _ => Self::SsdToSsd,
        }
    }

    /// `true` when a rotational disk is on either side.
    pub const fn involves_hdd(self) -> bool {
        !matches!(self, Self::SsdToSsd)
    }
}

impl fmt::Display for TransferKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SsdToSsd => "SSD → SSD",
            Self::SsdToHdd => "SSD → HDD",
            Self::HddToSsd => "HDD → SSD",
            Self::HddToHdd => "HDD → HDD",
            Self::SameHdd => "HDD → same HDD",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::device::DeviceId;

    fn disk(major: u32, kind: DiskKind) -> DiskInfo {
        DiskInfo::new(DeviceId::new(major, 0), "d", kind)
    }

    #[test]
    fn classify_covers_every_kind_pair() {
        use DiskKind::{Hdd, Ssd, Unknown};
        let cases = [
            (Ssd, Ssd, false, TransferKind::SsdToSsd),
            (Ssd, Hdd, false, TransferKind::SsdToHdd),
            (Hdd, Ssd, false, TransferKind::HddToSsd),
            (Hdd, Hdd, false, TransferKind::HddToHdd),
            (Hdd, Hdd, true, TransferKind::SameHdd),
            (Ssd, Ssd, true, TransferKind::SsdToSsd),
            (Unknown, Unknown, false, TransferKind::SsdToSsd),
            (Unknown, Hdd, false, TransferKind::SsdToHdd),
            (Hdd, Unknown, false, TransferKind::HddToSsd),
        ];
        for (src, dst, same, expected) in cases {
            assert_eq!(
                TransferKind::classify(src, dst, same),
                expected,
                "{src} → {dst} (same disk: {same})"
            );
        }
    }

    #[test]
    fn same_disk_hdd_is_same_hdd() {
        let topo = Topology::new(
            disk(8, DiskKind::Hdd),
            disk(8, DiskKind::Hdd),
            KindOverrides::default(),
        );
        assert_eq!(topo.transfer(), TransferKind::SameHdd);
    }

    #[test]
    fn overrides_replace_probed_kind() {
        let overrides = KindOverrides {
            src: Some(DiskKind::Hdd),
            dst: None,
        };
        let topo = Topology::new(disk(8, DiskKind::Ssd), disk(9, DiskKind::Ssd), overrides);
        assert_eq!(topo.transfer(), TransferKind::HddToSsd);
    }

    #[test]
    fn only_ssd_pair_does_not_involve_hdd() {
        assert!(!TransferKind::SsdToSsd.involves_hdd());
    }
}
