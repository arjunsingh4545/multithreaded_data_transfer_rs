//! Turning a workload and a topology into a concrete copy plan.

pub mod parallel;
pub mod pipeline;
pub mod tuning;
pub mod workload;

use std::fmt;
use std::num::NonZeroUsize;

pub use parallel::ParallelPlan;
pub use pipeline::PipelinePlan;
pub use tuning::{Mode, OrderKind, Tuning};
pub use workload::WorkloadStats;

use crate::domain::topology::TransferKind;
use crate::util::units::ByteSize;

/// Smallest I/O size the planner will accept.
pub const MIN_CHUNK: ByteSize = ByteSize::kib(4);

/// How the copy will be executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// Many concurrent whole-file copies.
    Parallel(ParallelPlan),
    /// Sequential reader(s) → bounded buffer → writer(s).
    Pipeline(PipelinePlan),
}

impl Strategy {
    /// The copy order this strategy wants.
    pub const fn order(&self) -> OrderKind {
        match self {
            Self::Parallel(p) => p.order,
            Self::Pipeline(p) => p.order,
        }
    }
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parallel(p) => write!(
                f,
                "parallel · {} workers · {} chunks · {}",
                p.workers, p.chunk, p.order
            ),
            Self::Pipeline(p) => write!(
                f,
                "pipeline · {} reader{} → {} writer{} · {} chunks · {} buffer · {}",
                p.readers,
                plural(p.readers),
                p.writers,
                plural(p.writers),
                p.chunk,
                p.memory(),
                p.order
            ),
        }
    }
}

fn plural(n: NonZeroUsize) -> &'static str {
    if n.get() == 1 { "" } else { "s" }
}

/// The final decision, with a human explanation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyPlan {
    /// Detected kind of copy.
    pub transfer: TransferKind,
    /// Chosen strategy and its parameters.
    pub strategy: Strategy,
    /// Why this strategy was chosen.
    pub reason: &'static str,
}

impl fmt::Display for CopyPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} · {}", self.transfer, self.strategy)
    }
}

const fn auto_reason(transfer: TransferKind) -> &'static str {
    match transfer {
        TransferKind::SsdToSsd => {
            "solid-state on both sides: many concurrent copies saturate the drives"
        }
        TransferKind::HddToHdd => {
            "two separate rotational disks: one sequential reader feeds one sequential writer, so both heads keep streaming"
        }
        TransferKind::SameHdd => {
            "one rotational disk holds both sides: large buffered batches minimise head travel"
        }
        TransferKind::SsdToHdd => {
            "fast source, rotational destination: parallel readers keep one sequential writer busy"
        }
        TransferKind::HddToSsd => {
            "rotational source: one reader streams in on-disk order while writers drain to the SSD"
        }
    }
}

/// Chooses a strategy. Pure: the core count is passed in so results are reproducible.
pub fn plan(
    stats: WorkloadStats,
    transfer: TransferKind,
    tuning: &Tuning,
    cores: NonZeroUsize,
) -> CopyPlan {
    let (pipelined, reason) = match tuning.mode {
        Mode::Auto => (transfer.involves_hdd(), auto_reason(transfer)),
        Mode::Parallel => (false, "parallel mode forced with --mode"),
        Mode::Pipeline => (true, "pipeline mode forced with --mode"),
    };
    let strategy = if pipelined {
        Strategy::Pipeline(pipeline::pipeline_plan(transfer, tuning, cores))
    } else {
        Strategy::Parallel(parallel::parallel_plan(stats, tuning, cores))
    };
    CopyPlan {
        transfer,
        strategy,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORES: NonZeroUsize = NonZeroUsize::new(8).unwrap();

    fn stats() -> WorkloadStats {
        WorkloadStats {
            files: 100,
            total: ByteSize::gib(1),
        }
    }

    #[test]
    fn auto_mode_picks_expected_strategy_per_transfer() {
        let cases = [
            (TransferKind::SsdToSsd, false),
            (TransferKind::SsdToHdd, true),
            (TransferKind::HddToSsd, true),
            (TransferKind::HddToHdd, true),
            (TransferKind::SameHdd, true),
        ];
        for (transfer, pipelined) in cases {
            let plan = plan(stats(), transfer, &Tuning::default(), CORES);
            assert_eq!(
                matches!(plan.strategy, Strategy::Pipeline(_)),
                pipelined,
                "{transfer}"
            );
        }
    }

    #[test]
    fn forced_parallel_ignores_disk_kinds() {
        let tuning = Tuning {
            mode: Mode::Parallel,
            ..Tuning::default()
        };
        let plan = plan(stats(), TransferKind::HddToHdd, &tuning, CORES);
        assert!(matches!(plan.strategy, Strategy::Parallel(_)));
    }

    #[test]
    fn forced_pipeline_on_ssd_is_honoured() {
        let tuning = Tuning {
            mode: Mode::Pipeline,
            ..Tuning::default()
        };
        let plan = plan(stats(), TransferKind::SsdToSsd, &tuning, CORES);
        assert!(matches!(plan.strategy, Strategy::Pipeline(_)));
    }

    #[test]
    fn order_override_reaches_strategy() {
        let tuning = Tuning {
            order: Some(OrderKind::Inode),
            ..Tuning::default()
        };
        let plan = plan(stats(), TransferKind::HddToHdd, &tuning, CORES);
        assert_eq!(plan.strategy.order(), OrderKind::Inode);
    }

    #[test]
    fn display_summarises_pipeline() {
        let plan = plan(stats(), TransferKind::HddToHdd, &Tuning::default(), CORES);
        assert_eq!(
            plan.to_string(),
            "HDD → HDD · pipeline · 1 reader → 1 writer · 8 MiB chunks · 1 GiB buffer · physical order"
        );
    }
}
