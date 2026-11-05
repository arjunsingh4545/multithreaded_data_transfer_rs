//! Running a copy: [`Engine::prepare`] decides, [`Engine::execute`] copies.
//!
//! The engine is generic over its OS seams (device probe, I/O hints, extent
//! locator). Default type parameters give production behaviour with
//! `Engine::new()`, while tests inject fakes through [`Engine::with_parts`].

pub mod error;
pub mod filter;
pub mod finalize;
pub mod order;
pub mod parallel;
pub mod pipeline;
pub mod progress;
pub mod queue;
pub mod scan;
pub mod skeleton;
pub mod stats;

mod prepare;
#[cfg(test)]
mod testing;

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

pub use self::filter::FilterOutcome;
pub use self::prepare::Prepared;
pub use self::stats::CopyStats;

use self::error::EngineError;
use self::progress::ProgressSink;
use crate::domain::plan::{CopyPlan, Strategy};
use crate::error::Result;
use crate::platform::device::{DeviceProbe, PlatformProbe};
use crate::platform::extent::{ExtentLocator, PlatformLocator};
use crate::platform::hints::{IoHints, PlatformHints};

/// The copy engine, parameterised over its OS adapters.
#[derive(Debug, Clone)]
pub struct Engine<D = PlatformProbe, H = PlatformHints, L = PlatformLocator> {
    probe: D,
    hints: H,
    locator: L,
    cores: NonZeroUsize,
}

impl Engine {
    /// An engine using the real platform adapters.
    pub fn new() -> Self {
        Self::with_parts(
            PlatformProbe::default(),
            PlatformHints::default(),
            PlatformLocator::default(),
        )
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl<D, H, L> Engine<D, H, L>
where
    D: DeviceProbe,
    H: IoHints,
    L: ExtentLocator + Sync,
{
    /// An engine with explicit adapters (tests, other platforms).
    pub fn with_parts(probe: D, hints: H, locator: L) -> Self {
        Self {
            probe,
            hints,
            locator,
            cores: std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN),
        }
    }

    /// The device probe in use.
    pub const fn probe(&self) -> &D {
        &self.probe
    }

    /// Validates the request, detects disks, scans, filters, plans and orders.
    ///
    /// Blocking: performs filesystem I/O.
    pub fn prepare<P: ProgressSink>(
        &self,
        request: &crate::CopyRequest,
        progress: &P,
    ) -> Result<Prepared> {
        prepare::prepare(&self.probe, &self.locator, self.cores, request, progress)
    }

    /// Creates the directory skeleton and symlinks, then copies every file.
    ///
    /// Blocking. Stops early (removing partial files) when `cancel` fires.
    pub fn execute<P: ProgressSink>(
        &self,
        prepared: &Prepared,
        progress: &P,
        cancel: &CancellationToken,
    ) -> Result<Summary> {
        let started = Instant::now();
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled.into());
        }
        let manifest = &prepared.manifest;
        skeleton::create_dirs(&manifest.directories)?;
        skeleton::create_symlinks(&manifest.symlinks)?;

        let stats = match &prepared.plan.strategy {
            Strategy::Parallel(plan) => {
                parallel::run_parallel(&manifest.files, plan, progress, cancel)
            }
            Strategy::Pipeline(plan) => {
                pipeline::run_pipeline(&manifest.files, plan, &self.hints, progress, cancel)
            }
        }?;

        Ok(Summary {
            stats,
            directories: manifest.directories.len(),
            symlinks: manifest.symlinks.len(),
            skipped: prepared.filtered,
            plan: prepared.plan,
            elapsed: started.elapsed(),
        })
    }
}

/// What a finished copy did.
#[derive(Debug, Clone, Copy)]
pub struct Summary {
    /// File counters.
    pub stats: CopyStats,
    /// Directories created (including the root).
    pub directories: usize,
    /// Symlinks recreated.
    pub symlinks: usize,
    /// Entries skipped by `--if-exists`.
    pub skipped: FilterOutcome,
    /// The plan that ran.
    pub plan: CopyPlan,
    /// Wall-clock time of the copy phase.
    pub elapsed: Duration,
}

impl Summary {
    /// Average throughput in bytes per second.
    pub fn throughput(&self) -> u64 {
        let secs = self.elapsed.as_secs_f64();
        if secs > 0.0 {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss,
                reason = "approximate rate for display"
            )]
            let rate = (self.stats.bytes as f64 / secs) as u64;
            rate
        } else {
            0
        }
    }
}
