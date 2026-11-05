//! Aggregate workload numbers the planner reasons about.

use crate::domain::job::ScanManifest;
use crate::util::units::ByteSize;

/// File count and volume of a copy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkloadStats {
    /// Number of regular files.
    pub files: usize,
    /// Total bytes.
    pub total: ByteSize,
}

impl WorkloadStats {
    /// Average file size (zero for an empty workload).
    pub fn average(&self) -> ByteSize {
        ByteSize::new(self.total.get() / (self.files.max(1) as u64))
    }
}

impl From<&ScanManifest> for WorkloadStats {
    fn from(manifest: &ScanManifest) -> Self {
        Self {
            files: manifest.files.len(),
            total: ByteSize::new(manifest.total_bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_handles_zero_files() {
        assert_eq!(WorkloadStats::default().average(), ByteSize::new(0));
    }

    #[test]
    fn average_divides_total_by_count() {
        let stats = WorkloadStats {
            files: 4,
            total: ByteSize::kib(4),
        };
        assert_eq!(stats.average(), ByteSize::kib(1));
    }
}
