---
name: fastcp-ci
description: Quality gates and CI/CD for the multithreaded_data_transfer_rs (fastcp) crate — the exact commands every change must pass (fmt, layer-boundary check, clippy pedantic with -D warnings, unit/integration/doc tests, rustdoc, release build), runnable scripts (scripts/ci.sh, scripts/check_layers.sh), a GitHub Actions template, and guidance for scaling the pipeline (caching, sharding, MSRV, cross-targets, supply-chain checks, hardware benchmarks). Use before committing, when a gate fails, or when creating or changing a CI pipeline for this repo.
metadata:
  scope: verification, ci-cd
  crate: multithreaded_data_transfer_rs
---

# fastcp CI/CD

## 1. Run the gates

```bash
bash .agents/skills/fastcp-ci/scripts/ci.sh            # everything, CI order
bash .agents/skills/fastcp-ci/scripts/ci.sh --quick    # pre-commit: fmt, layers, clippy, unit tests
bash .agents/skills/fastcp-ci/scripts/check_layers.sh  # layer rules only (no compile, ~instant)
```

`ci.sh` stops at the first failing gate and prints a per-gate timing summary.
It unsets `FASTCP_CHUNK`/`FASTCP_MEM` so a developer's tuning cannot change
test behaviour. `CARGO_FLAGS` (default `--locked`) is forwarded to cargo.

## 2. The gates, and how to fix each

| # | gate | command | typical failure → fix |
|---|------|---------|-----------------------|
| 1 | format | `cargo fmt --all -- --check` | run `cargo fmt` |
| 2 | layers | `scripts/check_layers.sh` | move the code to the right layer or invert the dependency with a trait (fastcp-extend R4); `unsafe` missing `// SAFETY:` → document the invariant |
| 3 | clippy | `cargo clippy --locked --all-targets --all-features -- -D warnings` | fix the lint; if a suppression is justified use `#[expect(clippy::x, reason = "…")]` on the smallest item, never a bare `#[allow]` |
| 4 | tests | `cargo test --locked --all-targets` | see §4 |
| 5 | doc tests | `cargo test --locked --doc` | doctests import via `multithreaded_data_transfer_rs::…`; end with `# Ok::<(), E>(())` when using `?` |
| 6 | rustdoc | `cargo doc --locked --no-deps` with `RUSTDOCFLAGS=-D warnings` | broken intra-doc links: use full paths or `[`Name`](crate::path::Name)`; ambiguous names (e.g. `Error` is also thiserror's derive) need a disambiguator: `[`Error`](enum@Error)` |
| 7 | release | `cargo build --locked --release` | LTO/codegen issues are rare; check `[profile.release]` |
| 8 | optional | `cargo deny check`, `cargo audit` (only if installed) | update or replace the flagged crate |

`--locked` means `Cargo.lock` must already match `Cargo.toml`: after editing
dependencies run `cargo update -p <crate>` (or a plain `cargo build`) and
commit the lock file.

## 3. Lint configuration (where it lives)

- `Cargo.toml` `[lints.rust]`: `unsafe_op_in_unsafe_fn = deny`, `missing_docs = warn`.
- `Cargo.toml` `[lints.clippy]`: `pedantic = warn` (priority -1),
  `unwrap_used = deny`, `expect_used = warn`, `undocumented_unsafe_blocks = deny`;
  allowed with a comment: `module_name_repetitions`, `must_use_candidate`,
  `missing_errors_doc`.
- `clippy.toml`: `allow-unwrap-in-tests`, `allow-expect-in-tests` — these cover
  `#[test]` fns and `#[cfg(test)]` modules only. Helper functions in
  `tests/*.rs` and `tests/common/` are **not** covered: each integration-test
  crate root needs `#![expect(clippy::unwrap_used, reason = "test helpers should fail loudly")]`.
- Under `-D warnings`, every `warn` above is a hard failure in CI.

## 4. Test suite map

| suite | location | notes |
|---|---|---|
| unit | `#[cfg(test)] mod tests` at the end of each `src/**.rs` | fakes: `FixedProbe`, `NoHints`, `NoLocator`, `FakeSysfs` (sysfs.rs), `FakeLocator` (order.rs), `RecordingHints` (dest_file.rs), `CountingProgress` (progress.rs) |
| engine fixtures | `src/engine/testing.rs` | `Fixture::with_sizes`, `assert_copied`, `pattern` |
| domain fixtures | `src/domain/job.rs::fixtures` | `job`, `job_at` |
| property | `src/util/units.rs` (`proptest!`) | ByteSize display/parse round-trip, parse never panics |
| library e2e | `tests/copy_modes.rs` | every strategy/transfer kind must produce `assert_trees_equal` |
| binary e2e | `tests/cli.rs` | runs `env!("CARGO_BIN_EXE_fastcp")`; exit codes 0/1/2/130 |
| shared | `tests/common/mod.rs` | `TreeBuilder`, `sample_tree`, `assert_trees_equal`, `content` |
| doc | `///` examples in `lib.rs`, `units.rs`, `topology.rs`, `fs.rs`, `fixed.rs`, `order.rs` | |

Environment assumptions (keep CI runners compatible):
- Linux with `/sys` mounted; temp dirs on ext4/tmpfs/overlayfs. Tests accept
  "unsupported" for FIEMAP/fallocate/sync_file_range on tmpfs.
- `kill` binary on `PATH` (Ctrl-C test). The test copies a 2 GiB *sparse*
  file in 4 KiB chunks; it is interrupted early, so real disk use is small.
- Tests must never touch real mounts (`/mnt`, `/media`).

Debugging a failure:
```bash
cargo test --locked <test_name> -- --nocapture       # one test with output
cargo test --locked --test cli                         # one integration crate
RUST_BACKTRACE=1 cargo test --locked --lib engine::pipeline
```
A hang in pipeline tests usually means an invariant from
fastcp-codebase/references/architecture.md (P5–P7: cancellation/unblocking)
was broken.

## 5. Adding a gate

1. Add it to `scripts/ci.sh` with `step "<name>" <command>` in cost order
   (cheap before expensive).
2. Mirror it as a job (or step) in `references/github-actions.yml` in the
   matching tier.
3. Optional tools: guard with `command -v <tool> >/dev/null` so local runs
   without the tool still pass.
4. Document it in the §2 table.

## 5b. Developer scripts

`scripts/*.sh` (aliased by the `Makefile`; `make help` lists them) wrap the
same commands for humans: `make quick` / `make check` call `ci.sh`,
`make lint` adds shellcheck, `make build` checks prerequisites and smoke-tests
the binary. Conventions for new scripts: `scripts/README.md`; start from
`scripts/_template.sh`, which sources `scripts/lib/common.sh`.

## 6. Setting up hosted CI

`references/github-actions.yml` is a ready template (tiered jobs, caching,
stable + MSRV 1.88 matrix, tag-triggered multi-target release artifacts).
Copy it to `.github/workflows/ci.yml`. This crate sits inside a larger repo
(`system_utils/`); follow `references/scaling.md` §8 to add a path filter
and working directory. Scaling guidance (nextest, sharding, sccache,
musl/aarch64, cargo-deny/audit, self-hosted HDD benchmark runner) is in
`references/scaling.md`.

## 7. Release checklist

1. Bump `version` in `Cargo.toml`; `cargo build` to refresh `Cargo.lock`.
2. `scripts/ci.sh` green locally.
3. Tag `vX.Y.Z`; the release job builds `target/<triple>/release/fastcp` per target.
4. Smoke test the artifact: `fastcp --version`, `fastcp probe / /tmp`,
   `fastcp -n <src> <dst>`.
