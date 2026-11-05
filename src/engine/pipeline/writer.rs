//! Pipeline writer: drains one lane into destination files.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crossbeam_channel::Receiver;
use tokio_util::sync::CancellationToken;

use super::channel::Message;
use super::dest_file::PendingFile;
use crate::domain::job::{FileId, FileJob};
use crate::engine::error::EngineError;
use crate::engine::progress::{ProgressSink, Role, WorkerIdx};
use crate::engine::queue::JobQueue;
use crate::engine::stats::CopyStats;
use crate::platform::hints::IoHints;
use crate::util::fs::display_name;
use crate::util::units::ByteSize;

/// Borrowed state for one writer thread.
pub struct Writer<'s, H: IoHints, P> {
    /// Position among writers.
    pub idx: WorkerIdx,
    /// This writer's lane.
    pub rx: Receiver<Message<'s>>,
    /// Job lookup by id.
    pub queue: &'s JobQueue<'s>,
    /// Kernel hints.
    pub hints: &'s H,
    /// Progress sink.
    pub progress: &'s P,
    /// Stop signal.
    pub cancel: &'s CancellationToken,
    /// Writeback window.
    pub flush_every: ByteSize,
}

impl<'s, H: IoHints, P: ProgressSink> Writer<'s, H, P> {
    /// Writes until every reader has hung up. Cancels everyone else on failure.
    pub fn run(self) -> Result<CopyStats, EngineError> {
        let result = self.drain();
        if result.is_err() {
            self.cancel.cancel();
        }
        self.progress.worker_done(Role::Writer, self.idx);
        result
    }

    fn drain(&self) -> Result<CopyStats, EngineError> {
        // Dropping this map (on any early return) deletes every partial file.
        let mut open: HashMap<FileId, PendingFile<'s, H>> = HashMap::new();
        let mut stats = CopyStats::default();

        for msg in &self.rx {
            if self.cancel.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            match msg {
                Message::Chunk { file, offset, data } => {
                    let job = self.queue.get(file);
                    let pending = match open.entry(file) {
                        Entry::Occupied(entry) => entry.into_mut(),
                        Entry::Vacant(entry) => entry.insert(self.open(job)?),
                    };
                    pending
                        .write_at(offset, &data)
                        .map_err(EngineError::copy_failed(job))?;
                    self.progress
                        .advanced(Role::Writer, self.idx, data.len() as u64);
                }
                Message::End { file, len } => {
                    let job = self.queue.get(file);
                    // Empty files never send a chunk, so they are created here.
                    let pending = match open.remove(&file) {
                        Some(pending) => pending,
                        None => self.open(job)?,
                    };
                    let done = pending.finish(len).map_err(EngineError::copy_failed(job))?;
                    stats.record_file(len, done.apply_metadata(&job.meta).is_ok());
                }
            }
        }

        if open.is_empty() {
            Ok(stats)
        } else {
            // A reader stopped mid-file; its own error is the one worth reporting.
            Err(EngineError::Cancelled)
        }
    }

    fn open(&self, job: &'s FileJob) -> Result<PendingFile<'s, H>, EngineError> {
        self.progress
            .file_started(Role::Writer, self.idx, &display_name(&job.dst), job.size);
        PendingFile::create(&job.dst, job.size, self.flush_every, self.hints)
            .map_err(EngineError::copy_failed(job))
    }
}
