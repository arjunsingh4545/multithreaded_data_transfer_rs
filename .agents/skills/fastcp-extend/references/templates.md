# Code templates

Copy, rename, delete what you do not need. All templates follow the crate's
lint set (clippy pedantic, `unwrap_used = deny`, `missing_docs = warn`).

## Util helper (`src/util/fs.rs`)

```rust
/// One-line summary of what it returns.
///
/// ```
/// use multithreaded_data_transfer_rs::util::fs::my_helper;
/// assert!(my_helper(std::path::Path::new("/")).is_some());
/// ```
pub fn my_helper(path: &Path) -> Option<&Path> {
    path.ancestors().find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn my_helper_finds_root() {
        assert_eq!(my_helper(Path::new("/")), Some(Path::new("/")));
    }
}
```

## Platform adapter (`src/platform/hints/<os>.rs`)

```rust
//! <OS> implementation of `IoHints`.

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;

use super::IoHints;
use crate::platform::ffi::cvt;

/// Hints backed by <OS> syscalls.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsHints;

impl IoHints for OsHints {
    fn advise_sequential(&self, file: &File) -> io::Result<()> {
        // SAFETY: `file` is borrowed for the whole call, so the fd is open;
        // no user memory is passed to the kernel.
        cvt(unsafe { libc::fcntl(file.as_raw_fd(), libc::F_RDAHEAD, 1) }).map(drop)
    }
    // Everything else inherits the no-op defaults.
}
```

Parent `mod.rs`:

```rust
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub type PlatformHints = macos::OsHints;
```

## Ordering policy (`src/engine/order.rs`)

```rust
/// Smallest files first.
#[derive(Debug, Clone, Copy, Default)]
pub struct SmallestFirst;

impl OrderingPolicy for SmallestFirst {
    fn order(&self, jobs: &mut [FileJob]) {
        jobs.sort_by_key(|job| job.size);
    }
}
// sort_jobs:  OrderKind::Smallest => SmallestFirst.order(jobs),
```

## Strategy runner (`src/engine/<name>/mod.rs`)

```rust
//! <Name> mode: one-line description.

use std::thread;

use tokio_util::sync::CancellationToken;

use super::error::EngineError;
use super::progress::{ProgressSink, Role};
use super::queue::JobQueue;
use super::stats::{CopyStats, combine};
use crate::domain::job::FileJob;
use crate::domain::plan::NamePlan;
use crate::util::sync::join_scoped;

/// Copies `jobs` using <name> mode.
pub fn run_name<P: ProgressSink>(
    jobs: &[FileJob],
    plan: &NamePlan,
    progress: &P,
    cancel: &CancellationToken,
) -> Result<CopyStats, EngineError> {
    let queue = JobQueue::new(jobs);
    thread::scope(|scope| {
        let handles = (0..plan.threads.get())
            .map(|i| {
                let queue = &queue;
                thread::Builder::new()
                    .name(format!("name-{i}"))
                    .spawn_scoped(scope, move || -> Result<CopyStats, EngineError> {
                        let mut stats = CopyStats::default();
                        while let Some((_, job)) = queue.next() {
                            if cancel.is_cancelled() {
                                return Err(EngineError::Cancelled);
                            }
                            // copy `job` here; use RemoveOnDrop + finalize::apply_to_*
                            stats.record_file(job.size, true);
                        }
                        Ok(stats)
                    })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(EngineError::ThreadSpawn)?;
        // Join everything before inspecting results.
        let results: Vec<_> = handles
            .into_iter()
            .map(|h| join_scoped(h, || EngineError::WorkerPanicked { role: Role::Copier }))
            .collect();
        combine(results)
    })
}
```

## CLI flag (`src/cli/args/copy.rs`, inside `PerfArgs`)

```rust
    /// Limit throughput, e.g. 100M (per second)
    #[arg(long, value_name = "SIZE", env = "FASTCP_BWLIMIT")]
    pub bwlimit: Option<ByteSize>,
```

Test (`src/cli/args/mod.rs` tests):

```rust
#[test]
fn bwlimit_accepts_human_units() {
    assert_eq!(copy_args(&["a", "b", "--bwlimit", "100M"]).perf.bwlimit, Some(ByteSize::mib(100)));
}
```

## Subcommand (`src/cli/commands/<name>.rs`)

```rust
//! `<name>`: one-line description.

use std::process::ExitCode;

use super::{Context, Run};
use crate::cli::args::NameArgs;
use crate::error::Result;

impl Run for NameArgs {
    fn run(self, ctx: &Context) -> Result<ExitCode> {
        ctx.out.success("did the thing");
        Ok(ExitCode::SUCCESS)
    }
}
```

## Engine unit test using the shared fixture

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::progress::NoProgress;
    use crate::engine::testing::{Fixture, assert_copied};

    #[test]
    fn copies_files_at_chunk_boundaries() {
        let fx = Fixture::with_sizes(&[0, 1, 65_535, 65_536, 65_537]);
        run_name(&fx.jobs, &plan(), &NoProgress, &CancellationToken::new()).unwrap();
        assert_copied(&fx);
    }

    #[test]
    fn cancellation_leaves_no_partial_files() {
        let fx = Fixture::with_sizes(&[1 << 20]);
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(run_name(&fx.jobs, &plan(), &NoProgress, &cancel).is_err());
        assert!(fx.dst_is_empty());
    }
}
```

## Integration test (`tests/copy_modes.rs`)

```rust
#[test]
fn name_mode_copies_tree() {
    let src = sample_tree();
    let (_out, dst, _) = copy_with(src.path(), FixedProbe::uniform(DiskKind::Ssd), small_chunks(Mode::Name));
    assert_trees_equal(src.path(), &dst);
}
```
