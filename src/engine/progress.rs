//! How the engine reports progress without knowing about terminals.

use std::fmt;

/// Index of a worker within its role (0-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkerIdx(pub usize);

/// What a worker thread does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Parallel mode: reads and writes whole files.
    Copier,
    /// Pipeline mode: reads source files into buffers.
    Reader,
    /// Pipeline mode: writes buffers to destination files.
    Writer,
}

impl Role {
    /// `true` when bytes reported by this role count toward overall progress
    /// (readers are excluded so bytes are not counted twice).
    pub const fn counts_toward_total(self) -> bool {
        !matches!(self, Self::Reader)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Copier => "copy",
            Self::Reader => "read",
            Self::Writer => "write",
        })
    }
}

/// Receives progress events. Every method defaults to doing nothing.
///
/// Implementations must be `Sync`: events arrive from many threads at once.
pub trait ProgressSink: Sync {
    /// The scanner has found `files` files totalling `bytes` so far.
    fn scanned(&self, _files: usize, _bytes: u64) {}

    /// A worker started on a file.
    fn file_started(&self, _role: Role, _worker: WorkerIdx, _name: &str, _size: u64) {}

    /// A worker moved `bytes` more bytes.
    fn advanced(&self, _role: Role, _worker: WorkerIdx, _bytes: u64) {}

    /// Pipeline buffer occupancy changed.
    fn buffer_level(&self, _in_use: usize, _capacity: usize) {}

    /// A worker has no more work.
    fn worker_done(&self, _role: Role, _worker: WorkerIdx) {}
}

impl<P: ProgressSink + ?Sized> ProgressSink for &P {
    fn scanned(&self, files: usize, bytes: u64) {
        (**self).scanned(files, bytes);
    }
    fn file_started(&self, role: Role, worker: WorkerIdx, name: &str, size: u64) {
        (**self).file_started(role, worker, name, size);
    }
    fn advanced(&self, role: Role, worker: WorkerIdx, bytes: u64) {
        (**self).advanced(role, worker, bytes);
    }
    fn buffer_level(&self, in_use: usize, capacity: usize) {
        (**self).buffer_level(in_use, capacity);
    }
    fn worker_done(&self, role: Role, worker: WorkerIdx) {
        (**self).worker_done(role, worker);
    }
}

/// Discards every event (quiet mode, tests).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProgress;

impl ProgressSink for NoProgress {}

/// Test sink that totals the bytes it sees, per counting rule.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct CountingProgress {
    pub(crate) total: std::sync::atomic::AtomicU64,
    pub(crate) read: std::sync::atomic::AtomicU64,
}

#[cfg(test)]
impl ProgressSink for CountingProgress {
    fn advanced(&self, role: Role, _worker: WorkerIdx, bytes: u64) {
        use std::sync::atomic::Ordering::Relaxed;
        if role.counts_toward_total() {
            self.total.fetch_add(bytes, Relaxed);
        } else {
            self.read.fetch_add(bytes, Relaxed);
        }
    }
}
