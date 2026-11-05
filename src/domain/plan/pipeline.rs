//! Planning for the reader → writer pipeline (HDD-friendly).

use std::num::NonZeroUsize;

use super::MIN_CHUNK;
use super::tuning::{OrderKind, Tuning};
use crate::domain::topology::TransferKind;
use crate::util::units::ByteSize;

/// Default I/O size per request: large enough to amortise a seek.
pub const DEFAULT_CHUNK: ByteSize = ByteSize::mib(8);
/// Default buffer memory between readers and writers.
pub const DEFAULT_MEM: ByteSize = ByteSize::gib(1);
/// Writeback window: dirty data is pushed to disk every this many bytes per file.
pub const DEFAULT_FLUSH_WINDOW: ByteSize = ByteSize::mib(64);

/// Parameters for pipeline mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelinePlan {
    /// Reader threads.
    pub readers: NonZeroUsize,
    /// Writer threads (each owns a lane of files).
    pub writers: NonZeroUsize,
    /// Bytes per read/write request.
    pub chunk: ByteSize,
    /// Number of `chunk`-sized buffers in flight.
    pub buffers: NonZeroUsize,
    /// Writeback window per file.
    pub flush_every: ByteSize,
    /// Copy order.
    pub order: OrderKind,
}

impl PipelinePlan {
    /// Upper bound on buffer memory.
    pub fn memory(&self) -> ByteSize {
        ByteSize::new(self.chunk.get().saturating_mul(self.buffers.get() as u64))
    }
}

/// Per-transfer defaults before user overrides.
struct Profile {
    readers: usize,
    writers: usize,
    order: OrderKind,
    mem_factor: u64,
}

fn profile(transfer: TransferKind, cores: usize) -> Profile {
    let wide = cores.clamp(1, 8);
    let (readers, writers, order, mem_factor) = match transfer {
        TransferKind::HddToHdd => (1, 1, OrderKind::Physical, 1),
        // One disk: bigger batches mean fewer head switches between the two areas.
        TransferKind::SameHdd => (1, 1, OrderKind::Physical, 4),
        // Cheap parallel reads keep a single sequential writer saturated.
        TransferKind::SsdToHdd => (wide, 1, OrderKind::Path, 1),
        TransferKind::HddToSsd => (1, wide.min(4), OrderKind::Physical, 1),
        TransferKind::SsdToSsd => (wide, wide, OrderKind::Largest, 1),
    };
    Profile {
        readers,
        writers,
        order,
        mem_factor,
    }
}

/// Builds a pipeline plan for `transfer`, applying any user overrides.
pub fn pipeline_plan(transfer: TransferKind, tuning: &Tuning, cores: NonZeroUsize) -> PipelinePlan {
    let profile = profile(transfer, cores.get());
    let nz = |n: usize| NonZeroUsize::new(n).unwrap_or(NonZeroUsize::MIN);

    let readers = tuning.readers.unwrap_or_else(|| nz(profile.readers));
    let writers = tuning.writers.unwrap_or_else(|| nz(profile.writers));
    let chunk = tuning.chunk.unwrap_or(DEFAULT_CHUNK).max(MIN_CHUNK);
    let mem = tuning
        .mem
        .unwrap_or_else(|| ByteSize::new(DEFAULT_MEM.get().saturating_mul(profile.mem_factor)));

    // Every thread must be able to hold a couple of buffers or the pipeline stalls.
    let min_buffers = 2 * (readers.get() + writers.get());
    let buffers = (mem.get() / chunk.get())
        .try_into()
        .unwrap_or(usize::MAX)
        .max(min_buffers);

    PipelinePlan {
        readers,
        writers,
        chunk,
        buffers: nz(buffers),
        flush_every: DEFAULT_FLUSH_WINDOW.max(chunk),
        order: tuning.order.unwrap_or(profile.order),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EIGHT: NonZeroUsize = NonZeroUsize::new(8).unwrap();

    fn auto(transfer: TransferKind) -> PipelinePlan {
        pipeline_plan(transfer, &Tuning::default(), EIGHT)
    }

    #[test]
    fn hdd_to_hdd_uses_one_reader_and_one_writer() {
        let plan = auto(TransferKind::HddToHdd);
        assert_eq!((plan.readers.get(), plan.writers.get()), (1, 1));
    }

    #[test]
    fn ssd_to_hdd_fans_in_to_one_writer() {
        let plan = auto(TransferKind::SsdToHdd);
        assert_eq!((plan.readers.get(), plan.writers.get()), (8, 1));
    }

    #[test]
    fn hdd_source_reads_in_physical_order() {
        assert_eq!(auto(TransferKind::HddToSsd).order, OrderKind::Physical);
    }

    #[test]
    fn same_disk_gets_four_times_the_memory() {
        assert_eq!(auto(TransferKind::SameHdd).memory(), ByteSize::gib(4));
    }

    #[test]
    fn default_memory_is_one_gib() {
        assert_eq!(auto(TransferKind::HddToHdd).memory(), DEFAULT_MEM);
    }

    #[test]
    fn buffers_at_least_twice_thread_count() {
        let tuning = Tuning {
            mem: Some(ByteSize::mib(8)),
            ..Tuning::default()
        };
        let plan = pipeline_plan(TransferKind::SsdToHdd, &tuning, EIGHT);
        assert_eq!(plan.buffers.get(), 2 * (8 + 1));
    }

    #[test]
    fn cli_readers_override_auto() {
        let tuning = Tuning {
            readers: NonZeroUsize::new(3),
            ..Tuning::default()
        };
        let plan = pipeline_plan(TransferKind::HddToHdd, &tuning, EIGHT);
        assert_eq!(plan.readers.get(), 3);
    }

    #[test]
    fn chunk_override_applies() {
        let tuning = Tuning {
            chunk: Some(ByteSize::mib(16)),
            ..Tuning::default()
        };
        assert_eq!(
            pipeline_plan(TransferKind::HddToHdd, &tuning, EIGHT).chunk,
            ByteSize::mib(16)
        );
    }

    #[test]
    fn flush_window_never_smaller_than_chunk() {
        let tuning = Tuning {
            chunk: Some(ByteSize::mib(128)),
            ..Tuning::default()
        };
        let plan = pipeline_plan(TransferKind::HddToHdd, &tuning, EIGHT);
        assert_eq!(plan.flush_every, ByteSize::mib(128));
    }
}
