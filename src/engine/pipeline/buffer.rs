//! A bounded pool of reusable I/O buffers.
//!
//! The pool caps pipeline memory: readers block when every buffer is in
//! flight, which is the back-pressure that keeps a fast source from racing
//! ahead of a slow destination.

use std::num::NonZeroUsize;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use tokio_util::sync::CancellationToken;

use crate::engine::error::EngineError;
use crate::util::units::ByteSize;

/// How often a blocked `acquire` re-checks cancellation.
const CANCEL_POLL: Duration = Duration::from_millis(50);

/// Fixed-capacity pool of `chunk`-sized buffers, allocated lazily.
#[derive(Debug)]
pub struct BufferPool {
    free_tx: Sender<Vec<u8>>,
    free_rx: Receiver<Vec<u8>>,
    chunk: usize,
    capacity: usize,
    allocated: AtomicUsize,
}

impl BufferPool {
    /// A pool of at most `capacity` buffers of `chunk` bytes each.
    pub fn new(chunk: ByteSize, capacity: NonZeroUsize) -> Self {
        let (free_tx, free_rx) = crossbeam_channel::bounded(capacity.get());
        Self {
            free_tx,
            free_rx,
            chunk: chunk.as_usize(),
            capacity: capacity.get(),
            allocated: AtomicUsize::new(0),
        }
    }

    /// Takes a free buffer, allocating one if the pool is not yet full, or
    /// waits for one to be returned. Gives up with `Cancelled` when `cancel` fires.
    pub fn acquire(&self, cancel: &CancellationToken) -> Result<PooledBuf<'_>, EngineError> {
        loop {
            if cancel.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            if let Ok(buf) = self.free_rx.try_recv() {
                return Ok(self.wrap(buf));
            }
            if self.reserve_allocation() {
                return Ok(self.wrap(vec![0; self.chunk]));
            }
            match self.free_rx.recv_timeout(CANCEL_POLL) {
                Ok(buf) => return Ok(self.wrap(buf)),
                // The pool owns a sender, so the channel never disconnects.
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
            }
        }
    }

    /// Buffers currently handed out.
    pub fn in_use(&self) -> usize {
        self.allocated
            .load(Ordering::Acquire)
            .saturating_sub(self.free_rx.len())
    }

    /// Maximum number of buffers.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Size of each buffer.
    pub const fn chunk_len(&self) -> usize {
        self.chunk
    }

    fn reserve_allocation(&self) -> bool {
        self.allocated
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.capacity).then_some(n + 1)
            })
            .is_ok()
    }

    const fn wrap(&self, buf: Vec<u8>) -> PooledBuf<'_> {
        PooledBuf {
            buf,
            len: 0,
            pool: self,
        }
    }
}

/// A buffer on loan from a [`BufferPool`]; returns itself on drop.
///
/// Dereferences to the *filled* bytes only.
#[derive(Debug)]
pub struct PooledBuf<'p> {
    buf: Vec<u8>,
    len: usize,
    pool: &'p BufferPool,
}

impl PooledBuf<'_> {
    /// The whole buffer, for filling.
    pub fn spare_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    /// Marks the first `len` bytes as valid.
    ///
    /// # Panics
    ///
    /// If `len` exceeds the buffer size.
    pub fn set_filled(&mut self, len: usize) {
        assert!(len <= self.buf.len(), "filled length exceeds buffer size");
        self.len = len;
    }
}

impl Deref for PooledBuf<'_> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl Drop for PooledBuf<'_> {
    fn drop(&mut self) {
        // Never full: at most `capacity` buffers exist and this one is not in the channel.
        self.pool
            .free_tx
            .try_send(std::mem::take(&mut self.buf))
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(capacity: usize) -> BufferPool {
        BufferPool::new(ByteSize::kib(4), NonZeroUsize::new(capacity).unwrap())
    }

    #[test]
    fn released_buffer_is_reused() {
        let pool = pool(1);
        let cancel = CancellationToken::new();
        let first = pool.acquire(&cancel).unwrap().buf.as_ptr();
        let second = pool.acquire(&cancel).unwrap();
        assert_eq!(second.buf.as_ptr(), first);
    }

    #[test]
    fn allocates_lazily_up_to_capacity() {
        let pool = pool(4);
        let cancel = CancellationToken::new();
        let _a = pool.acquire(&cancel).unwrap();
        let _b = pool.acquire(&cancel).unwrap();
        assert_eq!(pool.allocated.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn in_use_tracks_outstanding_buffers() {
        let pool = pool(4);
        let cancel = CancellationToken::new();
        let a = pool.acquire(&cancel).unwrap();
        let _b = pool.acquire(&cancel).unwrap();
        drop(a);
        assert_eq!(pool.in_use(), 1);
    }

    #[test]
    fn acquire_returns_cancelled_when_exhausted_and_cancelled() {
        let pool = pool(1);
        let cancel = CancellationToken::new();
        let _held = pool.acquire(&cancel).unwrap();
        cancel.cancel();
        assert!(matches!(pool.acquire(&cancel), Err(EngineError::Cancelled)));
    }

    #[test]
    fn acquire_waits_for_a_returned_buffer() {
        let pool = pool(1);
        let cancel = CancellationToken::new();
        let held = pool.acquire(&cancel).unwrap();
        std::thread::scope(|s| {
            s.spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                drop(held);
            });
            assert!(pool.acquire(&cancel).is_ok());
        });
    }

    #[test]
    fn deref_exposes_only_filled_bytes() {
        let pool = pool(1);
        let mut buf = pool.acquire(&CancellationToken::new()).unwrap();
        buf.spare_mut()[..3].copy_from_slice(b"abc");
        buf.set_filled(3);
        assert_eq!(&*buf, b"abc");
    }
}
