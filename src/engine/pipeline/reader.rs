//! Pipeline reader: streams source files into pooled buffers.

use std::fs::File;

use tokio_util::sync::CancellationToken;

use super::buffer::BufferPool;
use super::channel::{Message, Router};
use crate::domain::job::{FileId, FileJob};
use crate::engine::error::EngineError;
use crate::engine::progress::{ProgressSink, Role, WorkerIdx};
use crate::engine::queue::JobQueue;
use crate::platform::hints::IoHints;
use crate::util::fs::display_name;
use crate::util::io::read_full_at;

/// Borrowed state for one reader thread.
pub struct Reader<'s, H, P> {
    /// Position among readers.
    pub idx: WorkerIdx,
    /// Shared job cursor (planned order).
    pub queue: &'s JobQueue<'s>,
    /// Buffer pool (back-pressure).
    pub pool: &'s BufferPool,
    /// Where filled buffers go.
    pub router: Router<'s>,
    /// Kernel hints.
    pub hints: &'s H,
    /// Progress sink.
    pub progress: &'s P,
    /// Stop signal.
    pub cancel: &'s CancellationToken,
}

impl<H: IoHints, P: ProgressSink> Reader<'_, H, P> {
    /// Reads jobs until the queue is drained. Cancels everyone else on failure.
    pub fn run(self) -> Result<(), EngineError> {
        let result = self.drain();
        if result.is_err() {
            self.cancel.cancel();
        }
        self.progress.worker_done(Role::Reader, self.idx);
        result
    }

    fn drain(&self) -> Result<(), EngineError> {
        while let Some((id, job)) = self.queue.next() {
            if self.cancel.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            self.read_one(id, job)?;
        }
        Ok(())
    }

    fn read_one(&self, id: FileId, job: &FileJob) -> Result<(), EngineError> {
        let fail = EngineError::copy_failed(job);
        let file = File::open(&job.src).map_err(&fail)?;
        self.hints.advise_sequential(&file).ok();
        self.progress
            .file_started(Role::Reader, self.idx, &display_name(&job.src), job.size);

        let chunk = self.pool.chunk_len();
        let mut offset = 0u64;
        loop {
            let mut buf = self.pool.acquire(self.cancel)?;
            let n = read_full_at(&file, buf.spare_mut(), offset).map_err(&fail)?;
            if n == 0 {
                break;
            }
            buf.set_filled(n);
            self.router.send(Message::Chunk {
                file: id,
                offset,
                data: buf,
            })?;
            offset += n as u64;
            self.progress.advanced(Role::Reader, self.idx, n as u64);
            self.progress
                .buffer_level(self.pool.in_use(), self.pool.capacity());
            if n < chunk {
                // `read_full_at` only returns short at end of file.
                break;
            }
        }

        // Source pages will not be read again; don't let them evict useful cache.
        self.hints.evict(&file, 0..offset).ok();
        self.router.send(Message::End {
            file: id,
            len: offset,
        })
    }
}
