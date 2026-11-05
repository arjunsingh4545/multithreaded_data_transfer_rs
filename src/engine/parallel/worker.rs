//! One parallel-mode worker: pulls whole files off the queue and copies them.

use std::fs::{self, File};
use std::io::{Read, Write};

use tokio_util::sync::CancellationToken;

use crate::domain::job::FileJob;
use crate::engine::error::EngineError;
use crate::engine::finalize;
use crate::engine::progress::{ProgressSink, Role, WorkerIdx};
use crate::engine::queue::JobQueue;
use crate::engine::stats::CopyStats;
use crate::util::fs::{RemoveOnDrop, display_name};

/// Below this size the kernel copy path (`copy_file_range`) is used directly.
const SMALL_FILE: u64 = 1 << 20;

/// Borrowed state for one worker thread.
pub struct Worker<'s, P> {
    /// Position among copiers.
    pub idx: WorkerIdx,
    /// Shared job cursor.
    pub queue: &'s JobQueue<'s>,
    /// Buffer size for streamed files.
    pub chunk: usize,
    /// Progress sink.
    pub progress: &'s P,
    /// Stop signal.
    pub cancel: &'s CancellationToken,
}

impl<P: ProgressSink> Worker<'_, P> {
    /// Copies jobs until the queue is drained. Cancels everyone else on failure.
    pub fn run(self) -> Result<CopyStats, EngineError> {
        let result = self.drain();
        if result.is_err() {
            self.cancel.cancel();
        }
        self.progress.worker_done(Role::Copier, self.idx);
        result
    }

    fn drain(&self) -> Result<CopyStats, EngineError> {
        let mut buffer = vec![0u8; self.chunk];
        let mut stats = CopyStats::default();
        while let Some((_, job)) = self.queue.next() {
            if self.cancel.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            self.progress
                .file_started(Role::Copier, self.idx, &display_name(&job.src), job.size);
            let metadata_ok = if job.size < SMALL_FILE {
                self.copy_small(job)?
            } else {
                self.copy_streamed(job, &mut buffer)?
            };
            stats.record_file(job.size, metadata_ok);
        }
        Ok(stats)
    }

    /// Kernel-side copy; returns whether metadata could be restored.
    fn copy_small(&self, job: &FileJob) -> Result<bool, EngineError> {
        let mut guard = RemoveOnDrop::new(&job.dst);
        let copied = fs::copy(&job.src, &job.dst).map_err(EngineError::copy_failed(job))?;
        guard.disarm();
        self.progress.advanced(Role::Copier, self.idx, copied);
        Ok(finalize::apply_to_path(&job.dst, &job.meta).is_ok())
    }

    /// Chunked copy with per-chunk progress and cancellation checks.
    fn copy_streamed(&self, job: &FileJob, buffer: &mut [u8]) -> Result<bool, EngineError> {
        let fail = EngineError::copy_failed(job);
        let mut reader = File::open(&job.src).map_err(&fail)?;
        let mut guard = RemoveOnDrop::new(&job.dst);
        let mut writer = File::create(&job.dst).map_err(&fail)?;

        loop {
            if self.cancel.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            let n = match reader.read(buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(fail(e)),
            };
            writer.write_all(&buffer[..n]).map_err(&fail)?;
            self.progress.advanced(Role::Copier, self.idx, n as u64);
        }

        guard.disarm();
        Ok(finalize::apply_to_file(&writer, &job.meta).is_ok())
    }
}
