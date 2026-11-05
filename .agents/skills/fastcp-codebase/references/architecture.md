# Architecture deep dive

## Pipeline topology

```
 [Reader 0..R] ──Message::Chunk / Message::End──► Router ──lane = file % W──► [Writer i]
       ▲                                                                     │
       └───────────── BufferPool (bounded, lazily allocated, recycled) ◄────┘ (PooledBuf::drop)
```

Spawn order in `run_pipeline` (`engine/pipeline/mod.rs`):
1. writers (each owns one `Receiver<Message<'s>>`)
2. readers (each gets a `Router` clone)
3. `drop(router)` — the original; writers stop when **every** clone is gone
4. join **all** readers, then **all** writers, *before* inspecting results
   (an unjoined panicked scoped thread would panic the whole `thread::scope`)
5. error merge: a reader's real error wins; else writers' combined result;
   cancellations lose to real errors (`util::sync::first_error`).

### Invariants (break one → corruption or deadlock)

| # | invariant | where it is guaranteed |
|---|-----------|------------------------|
| P1 | each file has exactly one reader | `JobQueue::next` hands every job out once (atomic `fetch_add`) |
| P2 | each file always goes to the same writer lane | `Router::lane_for = file.index() % lanes` |
| P3 | a file's `End` arrives after its last `Chunk` | P1 + P2 + crossbeam channels are FIFO per sender |
| P4 | memory ≤ `chunk × buffers` | `BufferPool` allocates at most `capacity` buffers; `PooledBuf` returns itself on drop |
| P5 | readers blocked on memory still see cancellation | `BufferPool::acquire` polls `cancel` every 50 ms (`recv_timeout`) |
| P6 | a dead writer unblocks readers | its `Receiver` drops → `Router::send` returns `Err(Cancelled)` |
| P7 | a dead reader unblocks writers | its `Router` clone drops; writer loop ends; open files ⇒ `Err(Cancelled)` |
| P8 | partial files never survive errors | `PendingFile` holds `RemoveOnDrop`; only `finish(self)` disarms it; writer's `HashMap<FileId, PendingFile>` drops on early return |
| P9 | `buffers ≥ 2 × (readers + writers)` | `pipeline_plan()` clamps it |
| P10 | lane depth never deadlocks | lanes are `bounded(buffers)`; `End` messages carry no buffer, so a full lane only means back-pressure while the writer is alive |

### Lifetimes

- `'s` (scope) ties `Reader<'s,H,P>`/`Writer<'s,H,P>` to borrowed `JobQueue<'s>`,
  `BufferPool`, hints, progress and `CancellationToken`.
- `PooledBuf<'p>` borrows its pool; `Message<'p>` carries `PooledBuf<'p>`;
  so channels are `Sender<Message<'s>>` — no buffer can outlive the pool.
- `PendingFile<'a, H>` borrows its destination path from the job slice
  (`&'a Path`) and the hints (`&'a H`). `CompletedFile` owns only the `File`.
- `JobQueue::next` returns `&'a FileJob` — workers never clone jobs.

### Writeback pacing (`PendingFile::pace_writeback`)

Windows of `flush_every` (default 64 MiB, ≥ chunk):
- when a window fills: `start_writeback(window)` (async `SYNC_FILE_RANGE_WRITE`)
- and the previous unsettled region: `settle_and_evict` (wait + `POSIX_FADV_DONTNEED`)
- `finish(len)`: `set_len(len)` (trims fallocate excess / shrunken source),
  async writeback of the tail for large files.
Keeps dirty memory ≈ 2 windows per open file and avoids writeback stalls.

## Parallel path

`run_parallel`: N scoped `Worker`s over one `JobQueue`. Files < 1 MiB use
`std::fs::copy` (kernel `copy_file_range`); larger ones stream through a
`plan.chunk` buffer with per-chunk cancel checks. Both use `RemoveOnDrop`
and `finalize::{apply_to_path, apply_to_file}`. Results merged with
`stats::combine`.

## Planning table (`domain/plan`)

| TransferKind | Auto strategy | readers/writers | order | mem |
|---|---|---|---|---|
| SsdToSsd | Parallel | workers from avg size & cores | Largest | — |
| HddToHdd | Pipeline | 1 / 1 | Physical | 1 GiB |
| SameHdd | Pipeline | 1 / 1 | Physical | 4 GiB |
| SsdToHdd | Pipeline | min(cores,8) / 1 | Path | 1 GiB |
| HddToSsd | Pipeline | 1 / min(cores,8,4) | Physical | 1 GiB |

`DiskKind::Unknown` is treated as SSD (`DiskKind::effective`).
`--mode parallel|pipeline` overrides; all `Tuning` fields override defaults.
Chunks are clamped to `MIN_CHUNK` (4 KiB).

## Errors and cancellation

- One `CancellationToken` (tokio-util; works from plain threads) per run.
- Any worker that fails calls `cancel.cancel()` before returning, so peers stop.
- `EngineError::Cancelled` is a *consequence*, never the reported root cause
  when a real error exists (`Cancellation` trait + `first_error`).
- Exit codes: 0 ok, 1 failure, 2 usage (clap), 130 interrupted (`Error::exit_code`).
- Metadata (chmod/utimens) failures are counted in `CopyStats::metadata_failures`,
  not fatal — e.g. NTFS/exFAT mounts may refuse chmod.

## Detection (`platform/device/sysfs.rs`)

`stat(path).st_dev` → `DeviceId` (glibc encoding) → canonicalize
`/sys/dev/block/M:m` → if dir has `partition` file use parent → if `slaves/`
non-empty recurse (any HDD member ⇒ HDD) → read `queue/rotational`,
`queue/max_sectors_kb`, `dev`. Device not in sysfs (tmpfs, NFS, btrfs anon
dev) ⇒ `DiskInfo::unknown`. The destination is probed via `nearest_existing`.
