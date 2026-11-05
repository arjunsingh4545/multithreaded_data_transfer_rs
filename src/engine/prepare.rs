//! validate → detect topology → scan → filter → plan → order.

use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use super::error::EngineError;
use super::filter::{FilterOutcome, apply_policy};
use super::order::sort_jobs;
use super::progress::ProgressSink;
use super::scan::scan_tree;
use crate::domain::job::ScanManifest;
use crate::domain::plan::{CopyPlan, WorkloadStats, plan};
use crate::domain::request::CopyRequest;
use crate::domain::topology::{KindOverrides, Topology};
use crate::error::Result;
use crate::platform::device::{DeviceProbe, ProbeError};
use crate::platform::extent::ExtentLocator;
use crate::util::fs::{canonicalize_partial, is_within, nearest_existing};

/// Everything decided before a single byte is copied.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub(crate) manifest: ScanManifest,
    pub(crate) topology: Topology,
    pub(crate) plan: CopyPlan,
    pub(crate) filtered: FilterOutcome,
    pub(crate) src: PathBuf,
    pub(crate) dst: PathBuf,
}

impl Prepared {
    /// The chosen plan.
    pub const fn plan(&self) -> &CopyPlan {
        &self.plan
    }

    /// The scanned (and filtered) work.
    pub const fn manifest(&self) -> &ScanManifest {
        &self.manifest
    }

    /// The detected disks.
    pub const fn topology(&self) -> &Topology {
        &self.topology
    }

    /// What `--if-exists` removed.
    pub const fn filtered(&self) -> FilterOutcome {
        self.filtered
    }

    /// Canonical source root.
    pub fn src(&self) -> &Path {
        &self.src
    }

    /// Canonical destination root.
    pub fn dst(&self) -> &Path {
        &self.dst
    }
}

pub(crate) fn prepare<D, L, P>(
    probe: &D,
    locator: &L,
    cores: NonZeroUsize,
    request: &CopyRequest,
    progress: &P,
) -> Result<Prepared>
where
    D: DeviceProbe,
    L: ExtentLocator + Sync,
    P: ProgressSink,
{
    let (src, dst) = validate(request)?;
    let topology = detect_topology(probe, &src, &dst, request.overrides)?;

    let mut manifest = scan_tree(&src, &dst, progress)?;
    let filtered = apply_policy(&mut manifest, request.existing)?;

    let plan = plan(
        WorkloadStats::from(&manifest),
        topology.transfer(),
        &request.tuning,
        cores,
    );
    sort_jobs(plan.strategy.order(), &mut manifest.files, locator);

    Ok(Prepared {
        manifest,
        topology,
        plan,
        filtered,
        src,
        dst,
    })
}

/// Canonical `(src, dst)`, refusing anything that cannot work.
fn validate(request: &CopyRequest) -> Result<(PathBuf, PathBuf), EngineError> {
    let src = fs::canonicalize(&request.src)
        .map_err(|_| EngineError::SourceNotFound(request.src.clone()))?;
    if !src.is_dir() {
        return Err(EngineError::SourceNotDir(request.src.clone()));
    }
    let dst =
        canonicalize_partial(&request.dst).map_err(|source| EngineError::DestUnresolvable {
            path: request.dst.clone(),
            source,
        })?;
    if is_within(&dst, &src) {
        return Err(EngineError::DestInsideSource {
            src: request.src.clone(),
            dst: request.dst.clone(),
        });
    }
    Ok((src, dst))
}

/// Probes both sides; a missing destination is probed through its nearest parent.
fn detect_topology<D: DeviceProbe>(
    probe: &D,
    src: &Path,
    dst: &Path,
    overrides: KindOverrides,
) -> Result<Topology, ProbeError> {
    let dst_existing = nearest_existing(dst).unwrap_or(dst);
    Ok(Topology::new(
        probe.probe(src)?,
        probe.probe(dst_existing)?,
        overrides,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::progress::NoProgress;
    use crate::error::Error;
    use crate::platform::device::FixedProbe;
    use crate::platform::extent::NoLocator;
    use crate::{DiskKind, Strategy, TransferKind};

    const CORES: NonZeroUsize = NonZeroUsize::new(4).unwrap();

    fn run(request: &CopyRequest, probe: &FixedProbe) -> Result<Prepared> {
        prepare(probe, &NoLocator, CORES, request, &NoProgress)
    }

    #[test]
    fn rejects_dst_inside_src() {
        let src = tempfile::tempdir().unwrap();
        let req = CopyRequest::new(src.path(), src.path().join("backup"));
        let result = run(&req, &FixedProbe::default());
        assert!(matches!(
            result,
            Err(Error::Engine(EngineError::DestInsideSource { .. }))
        ));
    }

    #[test]
    fn rejects_missing_source() {
        let req = CopyRequest::new("/definitely/missing", "/tmp/x");
        let result = run(&req, &FixedProbe::default());
        assert!(matches!(
            result,
            Err(Error::Engine(EngineError::SourceNotFound(_)))
        ));
    }

    #[test]
    fn rejects_file_as_source() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        fs::write(&file, b"x").unwrap();
        let result = run(
            &CopyRequest::new(&file, dir.path().join("out")),
            &FixedProbe::default(),
        );
        assert!(matches!(
            result,
            Err(Error::Engine(EngineError::SourceNotDir(_)))
        ));
    }

    #[test]
    fn hdd_probe_yields_pipeline_plan() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(src.path().join("f"), b"x").unwrap();
        let prepared = run(
            &CopyRequest::new(src.path(), dst.path().join("new")),
            &FixedProbe::uniform(DiskKind::Hdd),
        )
        .unwrap();
        assert_eq!(prepared.plan().transfer, TransferKind::SameHdd);
        assert!(matches!(prepared.plan().strategy, Strategy::Pipeline(_)));
    }
}
