//! `probe`: show the detected disk behind each path.

use std::num::NonZeroUsize;
use std::process::ExitCode;

use super::{Context, Run};
use crate::cli::args::ProbeArgs;
use crate::cli::report;
use crate::domain::plan::{Tuning, WorkloadStats, plan};
use crate::domain::topology::{KindOverrides, Topology};
use crate::engine::error::EngineError;
use crate::error::Result;
use crate::platform::device::{DeviceProbe, PlatformProbe};
use crate::util::fs::{canonicalize_partial, nearest_existing};

impl Run for ProbeArgs {
    fn run(self, ctx: &Context) -> Result<ExitCode> {
        let probe = PlatformProbe::default();
        let disks = self
            .paths
            .iter()
            .map(|path| {
                let resolved =
                    canonicalize_partial(path).map_err(|source| EngineError::DestUnresolvable {
                        path: path.clone(),
                        source,
                    })?;
                let existing = nearest_existing(&resolved).unwrap_or(&resolved);
                let disk = probe.probe(existing)?;
                report::probe_row(&ctx.out, path, &disk);
                Ok(disk)
            })
            .collect::<Result<Vec<_>>>()?;

        if let [src, dst] = disks.as_slice() {
            let topology = Topology::new(src.clone(), dst.clone(), KindOverrides::default());
            let cores = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
            let plan = plan(
                WorkloadStats::default(),
                topology.transfer(),
                &Tuning::default(),
                cores,
            );
            ctx.out.field("copy", plan);
            ctx.out.detail(format!("why: {}", plan.reason));
        }
        Ok(ExitCode::SUCCESS)
    }
}
