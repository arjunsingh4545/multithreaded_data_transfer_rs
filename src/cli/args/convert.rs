//! CLI arguments → domain request, plus hints for flags that won't apply.

use super::copy::CopyArgs;
use crate::domain::plan::{Strategy, Tuning};
use crate::domain::request::CopyRequest;
use crate::domain::topology::KindOverrides;

impl From<CopyArgs> for CopyRequest {
    fn from(args: CopyArgs) -> Self {
        let perf = args.perf;
        Self::new(args.paths.src, args.paths.dst)
            .with_tuning(Tuning {
                mode: perf.mode.into(),
                workers: perf.workers,
                readers: perf.readers,
                writers: perf.writers,
                chunk: perf.chunk_size,
                mem: perf.mem,
                order: perf.order.map(Into::into),
            })
            .with_overrides(KindOverrides {
                src: args.detect.src_disk.map(Into::into),
                dst: args.detect.dst_disk.map(Into::into),
            })
            .with_existing(args.safety.if_exists.into())
    }
}

/// Which strategy-specific flags the user set (captured before the args are consumed).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlagsUsed {
    workers: bool,
    pipeline_knobs: bool,
}

impl From<&CopyArgs> for FlagsUsed {
    fn from(args: &CopyArgs) -> Self {
        let perf = &args.perf;
        Self {
            workers: perf.workers.is_some(),
            pipeline_knobs: perf.readers.is_some() || perf.writers.is_some() || perf.mem.is_some(),
        }
    }
}

impl FlagsUsed {
    /// A hint when a flag was given that the chosen strategy ignores.
    pub const fn hint_for(self, strategy: &Strategy) -> Option<&'static str> {
        match strategy {
            Strategy::Pipeline(_) if self.workers => {
                Some("--workers has no effect in pipeline mode; use --readers / --writers")
            }
            Strategy::Parallel(_) if self.pipeline_knobs => Some(
                "--readers / --writers / --mem only apply in pipeline mode (try --mode pipeline)",
            ),
            _ => None,
        }
    }
}
