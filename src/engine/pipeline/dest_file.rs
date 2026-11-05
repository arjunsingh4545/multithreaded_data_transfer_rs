//! Destination files with a type-state lifecycle:
//! [`PendingFile`] (being written, deleted if dropped) → [`CompletedFile`] (kept).
//!
//! `finish(self)` consumes the pending file, so writing after finishing or
//! restoring metadata before finishing does not compile.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::Path;

use crate::domain::job::FileMeta;
use crate::engine::finalize;
use crate::platform::ffi::is_unsupported;
use crate::platform::hints::IoHints;
use crate::util::fs::RemoveOnDrop;
use crate::util::units::ByteSize;

/// A destination file still being written. Dropping it deletes the partial file.
#[derive(Debug)]
pub struct PendingFile<'a, H: IoHints> {
    file: File,
    guard: RemoveOnDrop<'a>,
    hints: &'a H,
    flush_every: u64,
    /// Highest byte offset written so far.
    written_to: u64,
    /// Writeback has been started for everything below this offset.
    flushed_to: u64,
    /// Everything below this offset is on disk and evicted from cache.
    settled_to: u64,
}

impl<'a, H: IoHints> PendingFile<'a, H> {
    /// Creates (truncating) `path` and preallocates `size_hint` bytes.
    pub fn create(
        path: &'a Path,
        size_hint: u64,
        flush_every: ByteSize,
        hints: &'a H,
    ) -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        let guard = RemoveOnDrop::new(path);
        // Real errors (e.g. ENOSPC) abort early; "not supported" is fine.
        if let Err(err) = hints.preallocate(&file, size_hint)
            && !is_unsupported(&err)
        {
            return Err(err);
        }
        Ok(Self {
            file,
            guard,
            hints,
            flush_every: flush_every.get().max(1),
            written_to: 0,
            flushed_to: 0,
            settled_to: 0,
        })
    }

    /// Writes `data` at `offset`, then paces writeback.
    pub fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.file.write_all_at(data, offset)?;
        self.written_to = self.written_to.max(offset + data.len() as u64);
        self.pace_writeback();
        Ok(())
    }

    /// Rolling window: start writing back the newest full window, and wait for
    /// (then evict) the previous one. Dirty memory stays bounded at ~2 windows
    /// per file, which avoids the "fast for 30 s, then frozen" writeback stall.
    fn pace_writeback(&mut self) {
        while self.written_to - self.flushed_to >= self.flush_every {
            let window = self.flushed_to..self.flushed_to + self.flush_every;
            self.hints.start_writeback(&self.file, window.clone()).ok();
            if self.settled_to < window.start {
                self.hints
                    .settle_and_evict(&self.file, self.settled_to..window.start)
                    .ok();
                self.settled_to = window.start;
            }
            self.flushed_to = window.end;
        }
    }

    /// Trims the file to `len` (dropping unused preallocation) and keeps it.
    pub fn finish(self, len: u64) -> io::Result<CompletedFile> {
        let Self {
            file,
            mut guard,
            hints,
            flushed_to,
            ..
        } = self;
        file.set_len(len)?;
        if flushed_to > 0 && len > flushed_to {
            // Large file: push the tail out too, without waiting.
            hints.start_writeback(&file, flushed_to..len).ok();
        }
        guard.disarm();
        Ok(CompletedFile { file })
    }
}

/// A fully written destination file.
#[derive(Debug)]
pub struct CompletedFile {
    file: File,
}

impl CompletedFile {
    /// Restores the source's times and permissions.
    pub fn apply_metadata(self, meta: &FileMeta) -> io::Result<()> {
        finalize::apply_to_file(&self.file, meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::hints::NoHints;
    use crate::util::sync::LockExt;
    use std::fs;
    use std::ops::Range;
    use std::sync::Mutex;

    #[derive(Debug, PartialEq, Eq)]
    enum Call {
        Start(Range<u64>),
        Settle(Range<u64>),
    }

    /// Records writeback calls so pacing can be asserted.
    #[derive(Default)]
    struct RecordingHints(Mutex<Vec<Call>>);

    impl IoHints for RecordingHints {
        fn start_writeback(&self, _f: &File, range: Range<u64>) -> io::Result<()> {
            self.0.lock_or_recover().push(Call::Start(range));
            Ok(())
        }
        fn settle_and_evict(&self, _f: &File, range: Range<u64>) -> io::Result<()> {
            self.0.lock_or_recover().push(Call::Settle(range));
            Ok(())
        }
    }

    #[test]
    fn dropping_pending_file_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial");
        let mut pending = PendingFile::create(&path, 10, ByteSize::kib(4), &NoHints).unwrap();
        pending.write_at(0, b"half").unwrap();
        drop(pending);
        assert!(!path.exists());
    }

    #[test]
    fn finish_truncates_to_actual_len() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        let mut pending = PendingFile::create(&path, 100, ByteSize::kib(4), &NoHints).unwrap();
        pending.write_at(0, b"abc").unwrap();
        pending.finish(3).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"abc");
    }

    #[test]
    fn out_of_order_chunks_land_at_their_offsets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        let mut pending = PendingFile::create(&path, 6, ByteSize::kib(4), &NoHints).unwrap();
        pending.write_at(3, b"def").unwrap();
        pending.write_at(0, b"abc").unwrap();
        pending.finish(6).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"abcdef");
    }

    #[test]
    fn hints_receive_writeback_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        let hints = RecordingHints::default();
        let mut pending = PendingFile::create(&path, 0, ByteSize::new(4), &hints).unwrap();
        pending.write_at(0, &[0; 4]).unwrap();
        pending.write_at(4, &[0; 4]).unwrap();
        drop(pending.finish(8));
        assert_eq!(
            *hints.0.lock_or_recover(),
            [Call::Start(0..4), Call::Start(4..8), Call::Settle(0..4)]
        );
    }

    #[test]
    fn apply_metadata_sets_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        let source = dir.path().join("src");
        fs::write(&source, b"").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        let meta = FileMeta::from(&fs::metadata(&source).unwrap());

        let pending = PendingFile::create(&path, 0, ByteSize::kib(4), &NoHints).unwrap();
        pending.finish(0).unwrap().apply_metadata(&meta).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}
