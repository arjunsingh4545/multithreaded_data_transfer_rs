//! Reader → writer messages and lane routing.

use crossbeam_channel::{Receiver, Sender};

use super::buffer::PooledBuf;
use crate::domain::job::FileId;
use crate::engine::error::EngineError;

/// What travels from readers to writers. Borrows its buffer from the pool `'p`.
#[derive(Debug)]
pub enum Message<'p> {
    /// `data` belongs at `offset` in file `file`.
    Chunk {
        /// Which file.
        file: FileId,
        /// Byte offset within the file.
        offset: u64,
        /// The bytes.
        data: PooledBuf<'p>,
    },
    /// The reader finished `file`; `len` bytes were read in total.
    End {
        /// Which file.
        file: FileId,
        /// Final length (may differ from the scanned size if the file changed).
        len: u64,
    },
}

impl Message<'_> {
    /// The file this message concerns.
    pub const fn file(&self) -> FileId {
        match self {
            Self::Chunk { file, .. } | Self::End { file, .. } => *file,
        }
    }
}

/// Sends each file's messages to a fixed writer lane.
///
/// Because a file always goes to the same lane and has exactly one reader,
/// its `End` is always received after its last `Chunk`.
#[derive(Debug, Clone)]
pub struct Router<'p> {
    lanes: Vec<Sender<Message<'p>>>,
}

impl<'p> Router<'p> {
    /// The lane index for `file`.
    pub fn lane_for(&self, file: FileId) -> usize {
        file.index() % self.lanes.len()
    }

    /// Routes `msg` to its lane. Fails with `Cancelled` if that writer is gone.
    pub fn send(&self, msg: Message<'p>) -> Result<(), EngineError> {
        self.lanes[self.lane_for(msg.file())]
            .send(msg)
            .map_err(|_| EngineError::Cancelled)
    }
}

/// Creates `writers` bounded lanes of `depth` messages each.
pub fn lanes<'p>(writers: usize, depth: usize) -> (Router<'p>, Vec<Receiver<Message<'p>>>) {
    let (senders, receivers) = (0..writers.max(1))
        .map(|_| crossbeam_channel::bounded(depth))
        .unzip();
    (Router { lanes: senders }, receivers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_sends_same_file_to_same_lane() {
        let (router, _rx) = lanes(3, 4);
        let file = FileId::new(7);
        assert_eq!(router.lane_for(file), router.lane_for(file));
    }

    #[test]
    fn router_spreads_files_over_lanes() {
        let (router, _rx) = lanes(2, 4);
        assert_ne!(
            router.lane_for(FileId::new(0)),
            router.lane_for(FileId::new(1))
        );
    }

    #[test]
    fn send_to_dropped_writer_is_cancelled() {
        let (router, receivers) = lanes(1, 4);
        drop(receivers);
        let msg = Message::End {
            file: FileId::new(0),
            len: 0,
        };
        assert!(matches!(router.send(msg), Err(EngineError::Cancelled)));
    }
}
