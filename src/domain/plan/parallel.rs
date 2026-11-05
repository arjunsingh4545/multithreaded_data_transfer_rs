//! Planning for the many-concurrent-copies strategy (SSD-friendly).

use std::num::NonZeroUsize;

use super::MIN_CHUNK;
use super::tuning::{OrderKind, Tuning};
use super::workload::WorkloadStats;
use crate::util::units::ByteSize;

/// Parameters for parallel mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParallelPlan {
    /// Concurrent copy threads.
    pub workers: NonZeroUsize,
    /// Buffer size for streamed (large) files.
    pub chunk: ByteSize,
    /// Copy order.
    pub order: OrderKind,
}

/// Sizes the worker pool from the workload shape and core count.
pub fn parallel_plan(stats: WorkloadStats, tuning: &Tuning, cores: NonZeroUsize) -> ParallelPlan {
    let cores = cores.get();
    let avg = stats.average();

    let auto_workers = if avg < ByteSize::kib(64) {
        // Metadata/syscall heavy: oversubscribe cores.
        (cores * 2).min(32)
    } else if avg > ByteSize::mib(16) {
        // Bandwidth heavy: more workers only add contention.
        cores.clamp(2, 8)
    } else {
        cores.clamp(4, 16)
    }
    .min(stats.files.max(1));

    let auto_chunk = if avg > ByteSize::mib(8) {
        ByteSize::kib(256)
    } else {
        ByteSize::kib(64)
    };

    ParallelPlan {
        workers: tuning
            .workers
            .unwrap_or_else(|| NonZeroUsize::new(auto_workers).unwrap_or(NonZeroUsize::MIN)),
        chunk: tuning.chunk.unwrap_or(auto_chunk).max(MIN_CHUNK),
        order: tuning.order.unwrap_or(OrderKind::Largest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EIGHT: NonZeroUsize = NonZeroUsize::new(8).unwrap();

    fn stats(files: usize, total: ByteSize) -> WorkloadStats {
        WorkloadStats { files, total }
    }

    #[test]
    fn tiny_files_oversubscribe_cores() {
        let plan = parallel_plan(stats(10_000, ByteSize::mib(10)), &Tuning::default(), EIGHT);
        assert_eq!(plan.workers.get(), 16);
    }

    #[test]
    fn workers_never_exceed_file_count() {
        let plan = parallel_plan(stats(3, ByteSize::gib(3)), &Tuning::default(), EIGHT);
        assert_eq!(plan.workers.get(), 3);
    }

    #[test]
    fn zero_files_yields_single_worker() {
        let plan = parallel_plan(WorkloadStats::default(), &Tuning::default(), EIGHT);
        assert_eq!(plan.workers.get(), 1);
    }

    #[test]
    fn worker_override_wins() {
        let tuning = Tuning {
            workers: NonZeroUsize::new(2),
            ..Tuning::default()
        };
        let plan = parallel_plan(stats(10_000, ByteSize::mib(10)), &tuning, EIGHT);
        assert_eq!(plan.workers.get(), 2);
    }

    #[test]
    fn chunk_is_clamped_to_minimum() {
        let tuning = Tuning {
            chunk: Some(ByteSize::new(1)),
            ..Tuning::default()
        };
        let plan = parallel_plan(stats(1, ByteSize::kib(1)), &tuning, EIGHT);
        assert_eq!(plan.chunk, MIN_CHUNK);
    }
}
