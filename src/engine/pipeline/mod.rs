//! Pipeline mode: reader threads → bounded buffer pool → writer threads.
//!
//! ```text
//!  [reader 0..R] ──Chunk/End──► Router ──lane i──► [writer i]
//!        ▲                                            │
//!        └──────── BufferPool (bounded, recycled) ◄───┘
//! ```
//!
//! All threads are scoped, so they borrow the job slice, pool, hints and
//! progress sink directly; lifetimes guarantee nothing outlives the call.

mod buffer;
mod channel;
mod dest_file;
mod reader;
mod writer;

use std::thread;

use tokio_util::sync::CancellationToken;

use self::buffer::BufferPool;
use self::reader::Reader;
use self::writer::Writer;
use super::error::EngineError;
use super::progress::{ProgressSink, Role, WorkerIdx};
use super::queue::JobQueue;
use super::stats::{CopyStats, combine};
use crate::domain::job::FileJob;
use crate::domain::plan::PipelinePlan;
use crate::platform::hints::IoHints;
use crate::util::sync::{Cancellation, first_error, join_scoped};

/// Copies `jobs` through the pipeline described by `plan`.
pub fn run_pipeline<H: IoHints, P: ProgressSink>(
    jobs: &[FileJob],
    plan: &PipelinePlan,
    hints: &H,
    progress: &P,
    cancel: &CancellationToken,
) -> Result<CopyStats, EngineError> {
    let queue = JobQueue::new(jobs);
    let pool = BufferPool::new(plan.chunk, plan.buffers);
    let (router, lanes) = channel::lanes(plan.writers.get(), plan.buffers.get());

    thread::scope(|scope| {
        let spawn_failed = |err| {
            cancel.cancel();
            EngineError::ThreadSpawn(err)
        };

        let writers = lanes
            .into_iter()
            .enumerate()
            .map(|(i, rx)| {
                let writer = Writer {
                    idx: WorkerIdx(i),
                    rx,
                    queue: &queue,
                    hints,
                    progress,
                    cancel,
                    flush_every: plan.flush_every,
                };
                thread::Builder::new()
                    .name(format!("write-{i}"))
                    .spawn_scoped(scope, move || writer.run())
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(spawn_failed)?;

        let readers = (0..plan.readers.get())
            .map(|i| {
                let reader = Reader {
                    idx: WorkerIdx(i),
                    queue: &queue,
                    pool: &pool,
                    router: router.clone(),
                    hints,
                    progress,
                    cancel,
                };
                thread::Builder::new()
                    .name(format!("read-{i}"))
                    .spawn_scoped(scope, move || reader.run())
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(spawn_failed);

        // Writers finish once every Router clone is gone, including this one.
        drop(router);
        let readers = readers?;

        // Join everything before looking at results (unjoined panics abort the scope).
        let reader_results: Vec<_> = readers
            .into_iter()
            .map(|h| join_scoped(h, || EngineError::WorkerPanicked { role: Role::Reader }))
            .collect();
        let writer_results: Vec<_> = writers
            .into_iter()
            .map(|h| join_scoped(h, || EngineError::WorkerPanicked { role: Role::Writer }))
            .collect();

        // A reader's real error is the root cause; otherwise report the writers' outcome.
        match (first_error(reader_results), combine(writer_results)) {
            (Ok(()), written) => written,
            (Err(read), Err(write)) if read.is_cancellation() => Err(write),
            (Err(read), _) => Err(read),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::progress::{CountingProgress, NoProgress};
    use crate::engine::testing::{Fixture, assert_copied};
    use crate::platform::hints::NoHints;
    use crate::{ByteSize, OrderKind};
    use std::num::NonZeroUsize;
    use std::sync::atomic::Ordering;

    const CHUNK: u64 = 64 * 1024;

    fn plan(readers: usize, writers: usize) -> PipelinePlan {
        PipelinePlan {
            readers: NonZeroUsize::new(readers).unwrap(),
            writers: NonZeroUsize::new(writers).unwrap(),
            chunk: ByteSize::new(CHUNK),
            buffers: NonZeroUsize::new(8).unwrap(),
            flush_every: ByteSize::new(4 * CHUNK),
            order: OrderKind::Path,
        }
    }

    fn run(fx: &Fixture, plan: &PipelinePlan) -> Result<CopyStats, EngineError> {
        run_pipeline(
            &fx.jobs,
            plan,
            &NoHints,
            &NoProgress,
            &CancellationToken::new(),
        )
    }

    #[test]
    fn copies_files_at_chunk_boundaries() {
        let fx = Fixture::with_sizes(&[0, 1, CHUNK - 1, CHUNK, CHUNK + 1, 3 * CHUNK + 7]);
        let stats = run(&fx, &plan(1, 1)).unwrap();
        assert_eq!(stats.files, 6);
        assert_copied(&fx);
    }

    #[test]
    fn multi_reader_multi_writer_copies_everything() {
        let sizes: Vec<u64> = (0..40).map(|i| i * 9_973 + (i % 3) * CHUNK).collect();
        let fx = Fixture::with_sizes(&sizes);
        run(&fx, &plan(4, 3)).unwrap();
        assert_copied(&fx);
    }

    #[test]
    fn stats_bytes_equal_total() {
        let fx = Fixture::with_sizes(&[10, 2 * CHUNK + 3]);
        let stats = run(&fx, &plan(2, 2)).unwrap();
        assert_eq!(stats.bytes, 10 + 2 * CHUNK + 3);
    }

    #[test]
    fn progress_counts_written_bytes_once() {
        let fx = Fixture::with_sizes(&[10, 2 * CHUNK + 3]);
        let progress = CountingProgress::default();
        run_pipeline(
            &fx.jobs,
            &plan(1, 1),
            &NoHints,
            &progress,
            &CancellationToken::new(),
        )
        .unwrap();
        let (total, read) = (
            progress.total.load(Ordering::Relaxed),
            progress.read.load(Ordering::Relaxed),
        );
        assert_eq!((total, read), (10 + 2 * CHUNK + 3, 10 + 2 * CHUNK + 3));
    }

    #[test]
    fn cancellation_leaves_no_partial_files() {
        let fx = Fixture::with_sizes(&[5 * CHUNK, 5 * CHUNK]);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = run_pipeline(&fx.jobs, &plan(1, 1), &NoHints, &NoProgress, &cancel);
        assert!(matches!(result, Err(EngineError::Cancelled)));
        assert!(fx.dst_is_empty());
    }

    #[test]
    fn missing_source_file_propagates_error() {
        let fx = Fixture::with_sizes(&[10, 20]);
        std::fs::remove_file(&fx.jobs[1].src).unwrap();
        let result = run(&fx, &plan(1, 1));
        assert!(matches!(result, Err(EngineError::FileCopy { .. })));
    }

    #[test]
    fn unwritable_destination_propagates_error() {
        let mut fx = Fixture::with_sizes(&[10]);
        fx.jobs[0].dst = fx.dst.path().join("missing-dir/file");
        let result = run(&fx, &plan(1, 1));
        assert!(matches!(result, Err(EngineError::FileCopy { .. })));
    }
}
