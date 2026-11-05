---
name: fastcp-codebase
description: Navigate the multithreaded_data_transfer_rs (fastcp) crate — a disk-aware directory copier with a parallel (SSD) path and a reader→writer pipeline (HDD) path. Use before reading, reviewing, debugging or changing any code in this repo, to find where a concern lives, which layer may depend on which, the end-to-end data flow, the invariants the pipeline relies on, and known pitfalls. Pair with fastcp-extend (making changes) and fastcp-ci (verifying them).
metadata:
  scope: navigation, review, debugging
  crate: multithreaded_data_transfer_rs
  edition: "2024"
  msrv: "1.88"
---

# fastcp codebase guide

A Rust 2024 library + thin binary that copies a directory tree, choosing a
strategy from the kinds of disk on each side:

- **Parallel** (SSD → SSD): N scoped threads each copy whole files.
- **Pipeline** (any HDD involved): reader thread(s) → bounded buffer pool →
  writer thread(s), reading in on-disk order with large (8 MiB) requests.

Library crate: `multithreaded_data_transfer_rs`. Binary: `fastcp` (`[[bin]]` in
`Cargo.toml`; clap `name = "fastcp"`; env vars `FASTCP_*`). Unix only
(`compile_error!` otherwise).

## 1. Orient in 60 seconds

```
src/main.rs          3 lines → cli::main()
src/lib.rs           layer table, public re-exports, crate docs
src/error.rs         crate Error = Probe(ProbeError) | Engine(EngineError); exit_code()
src/util/            helpers that know nothing about copying
src/domain/          pure types + decisions, NO I/O
src/platform/        OS adapters (sysfs, fadvise/fallocate/sync_file_range, FIEMAP); only home of libc + unsafe
src/engine/          prepare → execute; parallel/ and pipeline/ copy paths
src/cli/             clap args, commands, report text, progress bars, Ctrl-C
tests/               integration: copy_modes.rs (library), cli.rs (binary), common/ (fixtures)
```

Read `src/lib.rs` first, then `src/engine/mod.rs` (the `Engine` facade), then
the file for your concern from the lookup table in §4.

## 2. Layer rules (enforced by `fastcp-ci/scripts/check_layers.sh`)

```
cli ──► engine ──► platform ──► domain ──► util
  └────────┴──► error (aggregates engine::error + platform::device::ProbeError)
```

| layer      | may `use crate::…`                 | may use external crates                        |
|------------|------------------------------------|------------------------------------------------|
| `util`     | nothing                            | std, thiserror                                 |
| `domain`   | `util`                             | std, thiserror, filetime (data type only)      |
| `platform` | `util`, `domain`                   | + libc, `unsafe` (each block has `// SAFETY:`) |
| `engine`   | `util`, `domain`, `platform`, `error` | + rayon, crossbeam-channel, tokio-util (CancellationToken), filetime |
| `cli`      | everything                         | + clap, clap_complete, indicatif, console, tokio |

Hard rules:
- `domain` performs no I/O — every function there is testable with plain values.
- `clap`/`indicatif`/`console`/`tokio` (runtime) never appear below `cli`.
  CLI enums (`ModeArg`, `KindArg`, …) mirror domain enums and convert with `From`.
- `unsafe` and non-test `libc` only in `platform/`. (`domain/device.rs` uses
  `libc::makedev` inside `#[cfg(test)]` only.)
- The engine reports progress only through the `ProgressSink` trait.
- Test modules sit at the **end** of each file (`#[cfg(test)] mod tests`); the
  layer checker relies on that to ignore test-only imports.

## 3. End-to-end data flow

```
argv ─Cli::parse_args()─► Cli ─into_command()─► Command::Copy(CopyArgs)
     (rejects copy-only args next to a subcommand; global -q/-v/--color allowed anywhere)
CopyArgs ─From─► CopyRequest {src, dst, tuning: Tuning, overrides: KindOverrides, existing: ExistingPolicy}
Engine::prepare(&req, &ScanSpinner)                       [engine/prepare.rs]
   validate()          canonical src (must be dir), canonicalize_partial(dst), refuse dst ⊆ src
   detect_topology()   DeviceProbe on src and nearest_existing(dst) → Topology{src,dst: DiskInfo}
   scan_tree()         rayon walk → ScanManifest{directories, symlinks, files: Vec<FileJob>, total_bytes}
   apply_policy()      ExistingPolicy filter → FilterOutcome
   plan()              pure: (WorkloadStats, TransferKind, &Tuning, cores) → CopyPlan{transfer, strategy, reason}
   sort_jobs()         OrderKind → LargestFirst | ByPath | ByInode | ByPhysical<L: ExtentLocator>
 → Prepared
report::plan(out, &prepared)                              [cli/report.rs]
Engine::execute(&prepared, &CopyUi|&NoProgress, &cancel)  [engine/mod.rs]
   skeleton::create_dirs / create_symlinks
   Strategy::Parallel → parallel::run_parallel(jobs, plan, progress, cancel)
   Strategy::Pipeline → pipeline::run_pipeline(jobs, plan, hints, progress, cancel)
 → Summary{stats: CopyStats, directories, symlinks, skipped, plan, elapsed}
report::summary(out, &summary); ExitCode
```

`Engine<D = PlatformProbe, H = PlatformHints, L = PlatformLocator>`:
`Engine::new()` = real adapters; `Engine::with_parts(probe, hints, locator)`
injects fakes (`FixedProbe`, `NoHints`, `NoLocator`) in tests.

## 4. "Where is …?" lookup

| concern | file |
|---|---|
| sizes like `8M`, `ByteSize` | `util/units.rs` |
| partial-file cleanup guard `RemoveOnDrop` | `util/fs.rs` |
| `nearest_existing`, `canonicalize_partial`, `is_within`, `rebase`, `display_name`, `read_trimmed` | `util/fs.rs` |
| `read_full_at` (positional read, EINTR retry) | `util/io.rs` |
| `first_error`, `join_scoped`, `LockExt`, `Cancellation` | `util/sync.rs` |
| `DiskKind`, `DeviceId` (dev_t decode), `DiskInfo` | `domain/device.rs` |
| `FileJob`, `FileMeta`, `FileId`, `ScanManifest`, test `fixtures` | `domain/job.rs` |
| `TransferKind::classify`, `Topology`, `KindOverrides` | `domain/topology.rs` |
| `CopyRequest`, `ExistingPolicy` | `domain/request.rs` |
| strategy selection + reasons, `Strategy`, `CopyPlan` | `domain/plan/mod.rs` |
| pipeline defaults table (`profile()`), `DEFAULT_CHUNK/MEM/FLUSH_WINDOW` | `domain/plan/pipeline.rs` |
| parallel worker sizing | `domain/plan/parallel.rs` |
| `Mode`, `OrderKind`, `Tuning` | `domain/plan/tuning.rs` |
| disk detection trait `DeviceProbe`, `ProbeError` | `platform/device/mod.rs` |
| Linux sysfs probe (partitions, dm/md `slaves/`) | `platform/device/sysfs.rs` |
| test/fallback probe with prefix rules | `platform/device/fixed.rs` |
| `IoHints` trait (no-op defaults), `NoHints` | `platform/hints/mod.rs` |
| fadvise / fallocate / sync_file_range | `platform/hints/linux.rs` |
| FIEMAP ioctl | `platform/extent/fiemap.rs` |
| libc return conversions, `is_unsupported` | `platform/ffi.rs` |
| `Engine`, `Summary` | `engine/mod.rs` |
| `Prepared`, validation, topology detection | `engine/prepare.rs` |
| directory walk | `engine/scan.rs` |
| `--if-exists` logic (mtime tolerance 2 s) | `engine/filter.rs` |
| ordering policies + `apply_permutation` | `engine/order.rs` |
| restore perms + times | `engine/finalize.rs` |
| dirs + symlinks creation | `engine/skeleton.rs` |
| `JobQueue<'a>` atomic cursor (shared by both paths) | `engine/queue.rs` |
| `CopyStats`, `combine()` | `engine/stats.rs` |
| `EngineError`, `copy_failed(job)` | `engine/error.rs` |
| `ProgressSink`, `Role`, `WorkerIdx`, `NoProgress`, test `CountingProgress` | `engine/progress.rs` |
| parallel copier | `engine/parallel/{mod,worker}.rs` |
| pipeline orchestration | `engine/pipeline/mod.rs` |
| `BufferPool`, `PooledBuf<'p>` | `engine/pipeline/buffer.rs` |
| `Message<'p>`, `Router<'p>`, `lanes()` | `engine/pipeline/channel.rs` |
| reader / writer threads | `engine/pipeline/{reader,writer}.rs` |
| `PendingFile` → `CompletedFile` type-state, writeback pacing | `engine/pipeline/dest_file.rs` |
| engine test fixture `Fixture`, `assert_copied`, `pattern` | `engine/testing.rs` |
| clap grammar, `Cli::into_command`, globals | `cli/args/mod.rs` |
| copy flags grouped by help heading | `cli/args/copy.rs` |
| CLI enums ↔ domain enums | `cli/args/value.rs` |
| `CopyArgs → CopyRequest`, `FlagsUsed::hint_for` | `cli/args/convert.rs` |
| subcommands behind `trait Run` | `cli/commands/*.rs` |
| text output, verbosity, colour | `cli/output.rs` |
| plan/summary/probe formatting | `cli/report.rs` |
| progress bars, scan spinner | `cli/ui/mod.rs`, `cli/ui/theme.rs`, `cli/ui/layout.rs` |
| Ctrl-C (first cancels, second exits 130) | `cli/signal.rs` |

Deeper material: `references/architecture.md` (pipeline invariants, lifetimes,
error/cancellation model) and `references/pitfalls.md` (bugs already hit and
non-obvious behaviour).

## 5. Conventions you must keep

- **Errors**: each layer owns a `thiserror` enum (`ParseByteSizeError`,
  `ProbeError`, `EngineError`); `crate::Error` aggregates with `#[from]`.
  No `unwrap()` in non-test code (`clippy::unwrap_used = deny`); `expect` needs
  `#[expect(clippy::expect_used, reason = "…")]`.
- **Lint suppression**: `#[expect(lint, reason = "…")]`, never bare `#[allow]`.
- **Best-effort hints**: OS hint failures are ignored (`.ok()`), except real
  errors from `preallocate` (e.g. `ENOSPC`) — filter with `platform::ffi::is_unsupported`.
- **Static dispatch on hot paths** (generics over `IoHints`, `ProgressSink`,
  `ExtentLocator`); runtime choice via enums (`Strategy`, `OrderKind`) that call
  the generic code.
- **Borrow, don't `Arc`**: worker threads are `std::thread::scope`d and borrow
  jobs/pool/hints/progress.
- **Tests**: names describe behaviour (`finish_truncates_to_actual_len`),
  roughly one assertion each, grouped in nested `mod`s; fakes implement the
  same traits (`FakeLocator`, `RecordingHints`, `FakeSysfs`, `FixedProbe`).
- **Docs**: every public item has `///` (`missing_docs = warn`); doctests for
  key APIs.

## 6. Quick commands

```bash
cargo test                       # unit + integration + doc
cargo test --lib engine::pipeline   # one module
cargo run -- probe /mnt/a /mnt/b -v  # detection + planned strategy
cargo run -- -n -v SRC DST           # dry run with reasoning
```
