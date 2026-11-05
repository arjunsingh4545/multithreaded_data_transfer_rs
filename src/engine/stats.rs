//! Copy counters, shared by the parallel and pipeline paths.

use std::ops::AddAssign;

use super::error::EngineError;
use crate::util::sync::first_error;

/// What a copy (or one worker) accomplished.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CopyStats {
    /// Files fully copied.
    pub files: usize,
    /// Bytes written.
    pub bytes: u64,
    /// Files whose permissions/times the destination filesystem refused.
    pub metadata_failures: usize,
}

impl CopyStats {
    /// Counts one finished file.
    pub fn record_file(&mut self, bytes: u64, metadata_ok: bool) {
        self.files += 1;
        self.bytes += bytes;
        self.metadata_failures += usize::from(!metadata_ok);
    }
}

impl AddAssign for CopyStats {
    fn add_assign(&mut self, other: Self) {
        self.files += other.files;
        self.bytes += other.bytes;
        self.metadata_failures += other.metadata_failures;
    }
}

/// Sums successful workers' stats, or returns the most relevant error.
pub(crate) fn combine(
    results: impl IntoIterator<Item = Result<CopyStats, EngineError>>,
) -> Result<CopyStats, EngineError> {
    let mut total = CopyStats::default();
    let outcomes: Vec<_> = results
        .into_iter()
        .map(|result| result.map(|stats| total += stats))
        .collect();
    first_error(outcomes).map(|()| total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(files: usize) -> CopyStats {
        CopyStats {
            files,
            bytes: files as u64 * 10,
            metadata_failures: 0,
        }
    }

    #[test]
    fn combine_sums_successes() {
        assert_eq!(combine([Ok(stats(1)), Ok(stats(2))]).unwrap(), stats(3));
    }

    #[test]
    fn combine_prefers_real_error_over_cancelled() {
        let result = combine([
            Err(EngineError::Cancelled),
            Err(EngineError::SourceNotFound("/x".into())),
            Ok(stats(1)),
        ]);
        assert!(matches!(result, Err(EngineError::SourceNotFound(_))));
    }

    #[test]
    fn record_file_counts_metadata_failures() {
        let mut s = CopyStats::default();
        s.record_file(5, false);
        assert_eq!(s.metadata_failures, 1);
    }
}
