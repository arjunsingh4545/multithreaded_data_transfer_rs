//! End-to-end library tests: every strategy must produce an identical tree.
#![expect(clippy::unwrap_used, reason = "test helpers should fail loudly")]

mod common;

use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;

use multithreaded_data_transfer_rs::domain::device::{DeviceId, DiskInfo};
use multithreaded_data_transfer_rs::platform::device::FixedProbe;
use multithreaded_data_transfer_rs::platform::extent::NoLocator;
use multithreaded_data_transfer_rs::platform::hints::{LinuxHints, NoHints};
use multithreaded_data_transfer_rs::{
    ByteSize, CopyRequest, DiskKind, Engine, ExistingPolicy, Mode, NoProgress, Strategy,
    TransferKind, Tuning,
};
use tokio_util::sync::CancellationToken;

use common::{TreeBuilder, assert_trees_equal, sample_tree};

/// A probe that reports `src_kind` under `src` and `dst_kind` (on a different disk) elsewhere.
fn probe(src: &Path, src_kind: DiskKind, dst_kind: DiskKind) -> FixedProbe {
    FixedProbe::uniform(dst_kind).with_rule(
        fs::canonicalize(src).unwrap(),
        DiskInfo::new(DeviceId::new(8, 16), "src-disk", src_kind),
    )
}

fn small_chunks(mode: Mode) -> Tuning {
    Tuning {
        mode,
        chunk: Some(ByteSize::kib(64)),
        mem: Some(ByteSize::mib(1)),
        ..Tuning::default()
    }
}

/// Copies `src` into a fresh destination and returns (tempdir guard, dst path, transfer kind).
fn copy_with(
    src: &Path,
    probe: FixedProbe,
    tuning: Tuning,
) -> (tempfile::TempDir, std::path::PathBuf, TransferKind) {
    let out = tempfile::tempdir().unwrap();
    let dst = out.path().join("copy");
    let engine = Engine::with_parts(probe, NoHints, NoLocator);
    let prepared = engine
        .prepare(
            &CopyRequest::new(src, &dst).with_tuning(tuning),
            &NoProgress,
        )
        .unwrap();
    let transfer = prepared.plan().transfer;
    engine
        .execute(&prepared, &NoProgress, &CancellationToken::new())
        .unwrap();
    (out, dst, transfer)
}

#[test]
fn parallel_mode_copies_tree() {
    let src = sample_tree();
    let (_out, dst, _) = copy_with(
        src.path(),
        FixedProbe::uniform(DiskKind::Ssd),
        small_chunks(Mode::Parallel),
    );
    assert_trees_equal(src.path(), &dst);
}

#[test]
fn pipeline_hdd_to_hdd_copies_tree() {
    let src = sample_tree();
    let probe = probe(src.path(), DiskKind::Hdd, DiskKind::Hdd);
    let (_out, dst, transfer) = copy_with(src.path(), probe, small_chunks(Mode::Auto));
    assert_eq!(transfer, TransferKind::HddToHdd);
    assert_trees_equal(src.path(), &dst);
}

#[test]
fn pipeline_ssd_to_hdd_with_many_readers_copies_tree() {
    let src = sample_tree();
    let probe = probe(src.path(), DiskKind::Ssd, DiskKind::Hdd);
    let tuning = Tuning {
        readers: NonZeroUsize::new(4),
        ..small_chunks(Mode::Auto)
    };
    let (_out, dst, transfer) = copy_with(src.path(), probe, tuning);
    assert_eq!(transfer, TransferKind::SsdToHdd);
    assert_trees_equal(src.path(), &dst);
}

#[test]
fn pipeline_hdd_to_ssd_with_many_writers_copies_tree() {
    let src = sample_tree();
    let probe = probe(src.path(), DiskKind::Hdd, DiskKind::Ssd);
    let tuning = Tuning {
        writers: NonZeroUsize::new(3),
        ..small_chunks(Mode::Auto)
    };
    let (_out, dst, transfer) = copy_with(src.path(), probe, tuning);
    assert_eq!(transfer, TransferKind::HddToSsd);
    assert_trees_equal(src.path(), &dst);
}

#[test]
fn pipeline_with_real_linux_hints_copies_tree() {
    let src = sample_tree();
    let out = tempfile::tempdir().unwrap();
    let dst = out.path().join("copy");
    let engine = Engine::with_parts(FixedProbe::uniform(DiskKind::Hdd), LinuxHints, NoLocator);
    let request = CopyRequest::new(src.path(), &dst).with_tuning(small_chunks(Mode::Pipeline));
    let prepared = engine.prepare(&request, &NoProgress).unwrap();
    engine
        .execute(&prepared, &NoProgress, &CancellationToken::new())
        .unwrap();
    assert_trees_equal(src.path(), &dst);
}

#[test]
fn empty_source_creates_empty_destination() {
    let src = TreeBuilder::default().dir("only/dirs").build();
    let (_out, dst, _) = copy_with(
        src.path(),
        FixedProbe::uniform(DiskKind::Hdd),
        small_chunks(Mode::Auto),
    );
    assert!(dst.join("only/dirs").is_dir());
}

#[test]
fn update_policy_skips_unchanged_files_on_second_run() {
    let src = sample_tree();
    let (_out, dst, _) = copy_with(
        src.path(),
        FixedProbe::uniform(DiskKind::Ssd),
        small_chunks(Mode::Parallel),
    );

    let engine = Engine::with_parts(FixedProbe::uniform(DiskKind::Ssd), NoHints, NoLocator);
    let request = CopyRequest::new(src.path(), &dst).with_existing(ExistingPolicy::Update);
    let prepared = engine.prepare(&request, &NoProgress).unwrap();
    assert!(prepared.manifest().files.is_empty());
}

#[test]
fn auto_mode_on_ssds_runs_in_parallel() {
    let src = sample_tree();
    let engine = Engine::with_parts(FixedProbe::uniform(DiskKind::Ssd), NoHints, NoLocator);
    let prepared = engine
        .prepare(
            &CopyRequest::new(src.path(), src.path().with_extension("out")),
            &NoProgress,
        )
        .unwrap();
    assert!(matches!(prepared.plan().strategy, Strategy::Parallel(_)));
}
