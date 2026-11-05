//! Parallel mode: N workers each copy whole files (best for SSDs).

mod worker;

use std::thread;

use tokio_util::sync::CancellationToken;

use self::worker::Worker;
use super::error::EngineError;
use super::progress::{ProgressSink, Role, WorkerIdx};
use super::queue::JobQueue;
use super::stats::{CopyStats, combine};
use crate::domain::job::FileJob;
use crate::domain::plan::ParallelPlan;
use crate::util::sync::join_scoped;

/// Copies `jobs` with `plan.workers` scoped threads.
pub fn run_parallel<P: ProgressSink>(
    jobs: &[FileJob],
    plan: &ParallelPlan,
    progress: &P,
    cancel: &CancellationToken,
) -> Result<CopyStats, EngineError> {
    let queue = JobQueue::new(jobs);

    thread::scope(|scope| {
        let handles = (0..plan.workers.get())
            .map(|i| {
                let worker = Worker {
                    idx: WorkerIdx(i),
                    queue: &queue,
                    chunk: plan.chunk.as_usize(),
                    progress,
                    cancel,
                };
                thread::Builder::new()
                    .name(format!("copy-{i}"))
                    .spawn_scoped(scope, move || worker.run())
            })
            .collect::<Result<Vec<_>, _>>();

        let handles = match handles {
            Ok(handles) => handles,
            Err(err) => {
                cancel.cancel();
                return Err(EngineError::ThreadSpawn(err));
            }
        };

        // Join every handle before inspecting results (unjoined panics would abort the scope).
        let results: Vec<_> = handles
            .into_iter()
            .map(|h| join_scoped(h, || EngineError::WorkerPanicked { role: Role::Copier }))
            .collect();
        combine(results)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::progress::{CountingProgress, NoProgress};
    use crate::engine::testing::{Fixture, assert_copied};
    use std::num::NonZeroUsize;
    use std::sync::atomic::Ordering;

    fn plan(workers: usize) -> ParallelPlan {
        ParallelPlan {
            workers: NonZeroUsize::new(workers).unwrap(),
            chunk: crate::ByteSize::kib(64),
            order: crate::OrderKind::Largest,
        }
    }

    #[test]
    fn copies_small_and_streamed_files() {
        let fx = Fixture::with_sizes(&[0, 1, 4096, (1 << 20) + 17, 3 << 20]);
        let stats =
            run_parallel(&fx.jobs, &plan(3), &NoProgress, &CancellationToken::new()).unwrap();
        assert_eq!(stats.files, 5);
        assert_copied(&fx);
    }

    #[test]
    fn progress_bytes_equal_total() {
        let fx = Fixture::with_sizes(&[10, (2 << 20) + 5]);
        let progress = CountingProgress::default();
        run_parallel(&fx.jobs, &plan(2), &progress, &CancellationToken::new()).unwrap();
        assert_eq!(progress.total.load(Ordering::Relaxed), 10 + (2 << 20) + 5);
    }

    #[test]
    fn pre_cancelled_run_copies_nothing() {
        let fx = Fixture::with_sizes(&[10, 20]);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = run_parallel(&fx.jobs, &plan(2), &NoProgress, &cancel);
        assert!(matches!(result, Err(EngineError::Cancelled)));
    }

    #[test]
    fn missing_source_file_propagates_error() {
        let fx = Fixture::with_sizes(&[10]);
        std::fs::remove_file(&fx.jobs[0].src).unwrap();
        let result = run_parallel(&fx.jobs, &plan(1), &NoProgress, &CancellationToken::new());
        assert!(matches!(result, Err(EngineError::FileCopy { .. })));
    }
}
