# Scaling the pipeline

Ordered from "do first" to "do when it hurts".

## 1. Tiered jobs (already in the template)
Tier 1 (fmt + layer check) needs no dependency build and fails in seconds.
Tier 2 (clippy, tests, docs) runs in parallel after Tier 1. Tier 3 (release)
runs only on tags. Keep new gates in the cheapest tier that can run them.

## 2. Caching
- `Swatinem/rust-cache@v2` caches `~/.cargo` and `target/`; key per toolchain
  and per target.
- `CARGO_INCREMENTAL=0` in CI (incremental artifacts bloat caches, no benefit).
- Larger teams: `sccache` with a shared bucket (`RUSTC_WRAPPER=sccache`).

## 3. Faster tests
- `cargo nextest run --locked --all-targets` (+ `cargo test --doc`, nextest
  does not run doctests). Per-test processes isolate the Ctrl-C test.
- Sharding when the suite grows: `cargo nextest run --partition count:${{ matrix.shard }}/4`.
- Keep tests hermetic: every test uses `tempfile`; none may touch real
  `/mnt` or `/media` paths; unit tests inject `FixedProbe`/`NoHints`/`NoLocator`.

## 4. More platforms
- `x86_64-unknown-linux-musl`: the FIEMAP call passes the request with
  `as _`, which adapts to musl's `c_int` ioctl type; verify with
  `cargo build --target x86_64-unknown-linux-musl`.
- macOS/BSD: needs new adapters (`fastcp-extend` R3) because `sysfs`,
  `sync_file_range` and FIEMAP are Linux-only; `PlatformProbe` falls back to
  `FixedProbe` (Unknown ⇒ treated as SSD).
- Non-Unix is rejected by `compile_error!` in `lib.rs`.

## 5. Supply chain (optional; ci.sh runs them only if installed)
- `cargo deny check` — licences, duplicate versions, banned crates
  (add a `deny.toml`).
- `cargo audit` — RustSec advisories. Run on a nightly schedule too, because
  advisories appear without code changes.
- `cargo update --dry-run` weekly job or Dependabot/Renovate for `Cargo.lock`.

## 6. Performance regression tracking
Copy throughput depends on real disks, so GitHub-hosted runners cannot measure
it meaningfully (they are SSD/VM-backed, and caches distort results).
- Use a **self-hosted runner** with a known HDD pair, trigger manually or
  nightly, never on every PR.
- Procedure: `sync; echo 3 | sudo tee /proc/sys/vm/drop_caches` before each run;
  measure raw device speed with `fio --rw=read --bs=8M` / `--rw=write` as the
  ceiling; time `fastcp --mode parallel|pipeline` on a fixed dataset; record
  MB/s as a build artifact; alert on >10 % regression.
- CPU-bound micro-benchmarks (planning, ordering, permutation) can use
  `criterion` on hosted runners if they ever matter.

## 7. MSRV policy
`rust-version` in `Cargo.toml` (currently 1.88, required by let-chains). The
MSRV matrix leg proves it; bump it deliberately in its own PR and update the
template's matrix value.

## 8. Monorepo note
This crate lives in a subdirectory of a larger repo (`system_utils/`). In a
monorepo pipeline, add a path filter so the jobs run only when the crate
changes:

```yaml
on:
  pull_request:
    paths: ["multithreaded_data_transfer_rs/**"]
```
and set `defaults.run.working-directory: multithreaded_data_transfer_rs`
(plus `CRATE_DIR`) in the template.
