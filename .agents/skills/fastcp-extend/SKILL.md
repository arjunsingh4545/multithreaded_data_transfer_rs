---
name: fastcp-extend
description: Step-by-step recipes for changing the multithreaded_data_transfer_rs (fastcp) crate without breaking its layering — adding or removing a util helper, a platform adapter (DeviceProbe / IoHints / ExtentLocator), an ordering policy, a copy strategy, an error variant, a CLI flag or subcommand, or a progress event. Use whenever you add, move, rename or delete code in this repo. Read fastcp-codebase first for the map; finish with fastcp-ci to verify.
metadata:
  scope: implementation, refactoring
  crate: multithreaded_data_transfer_rs
---

# Extending fastcp

## 0. Before you touch anything

1. Decide the **layer** (fastcp-codebase §2). Ask, in order:
   - Knows nothing about copying/disks? → `util`
   - Pure data or a pure decision? → `domain`
   - Talks to the OS (syscalls, sysfs, ioctl)? → `platform`
   - Runs the copy (threads, files, ordering)? → `engine`
   - Parses args, prints, draws bars? → `cli`
2. Search before adding: `rg -n 'fn <name>|struct <Name>|trait <Name>' src`.
   If a helper already exists in another layer, **move it down** to the lowest
   layer that all callers can reach rather than duplicating it.
3. Plan the tests first (one behaviour per test, named after the behaviour).

## 1. Universal checklist (every change)

- [ ] Code lives in the right layer; no new upward `use crate::…`.
- [ ] Public items have `///` docs; key APIs get a doctest.
- [ ] Errors: typed `thiserror` variant in the layer's enum; no `unwrap()`
      outside tests; `expect` only with `#[expect(clippy::expect_used, reason)]`.
- [ ] Generics for hot paths, enums for runtime choices; borrow instead of clone.
- [ ] `#[cfg(test)] mod tests` at the **end** of the file.
- [ ] Re-export from `lib.rs` only if library users need it.
- [ ] `bash .agents/skills/fastcp-ci/scripts/ci.sh` passes.

## 2. Recipes

Each recipe lists files in the order to edit them. Code templates are in
`references/templates.md`.

### R1. Add a util helper
1. Pick the module: `util/fs.rs` (paths/files), `util/io.rs` (byte I/O),
   `util/sync.rs` (threads/locks/errors), `util/units.rs` (quantities).
   A genuinely new concern → `util/<name>.rs` + `pub mod <name>;` in `util/mod.rs`.
2. Signature: borrow inputs (`&Path`, `&[u8]`), return borrowed data when
   possible (`Option<&Path>`, `Cow<'_, str>`), `io::Result` for fallible I/O.
3. No `use crate::…` at all (util is the bottom layer). No `libc` — OS-specific
   helpers belong in `platform/ffi.rs`.
4. Unit tests in the same file using `tempfile`.
5. Replace any duplicated inline code at call sites with the helper.

### R2. Remove (or move) a util helper
1. `rg -n '\b<name>\b' src tests .agents` — list every caller, including docs and skills.
2. If callers remain in one layer only, move the helper into that layer's
   module instead of deleting it.
3. Delete the function **and** its tests; delete the module + `pub mod` line if empty.
4. If it was re-exported in `lib.rs` or mentioned in a doctest, update those.
5. Update `.agents/skills/fastcp-codebase/SKILL.md` §4 lookup table.
6. Run ci.sh (catches dead code via `-D warnings`).

### R3. Add a platform adapter (new implementation of an existing trait)
Traits: `DeviceProbe` (`platform/device/mod.rs`), `IoHints`
(`platform/hints/mod.rs`, all methods default to no-op), `ExtentLocator`
(`platform/extent/mod.rs`).
1. New file next to the trait, e.g. `platform/hints/macos.rs`, gated with
   `#[cfg(target_os = "…")]` in the parent `mod.rs`.
2. Implement only what the platform supports (inherit no-op defaults).
3. Every `unsafe` block: `// SAFETY:` comment naming the invariants (fd
   validity, buffer size/alignment). Convert return codes with
   `platform::ffi::{cvt, cvt_errno, to_off}`; map "not supported" errors with
   `ffi::is_unsupported` so callers fail soft.
4. Update the `Platform*` type alias (`PlatformProbe`/`PlatformHints`/
   `PlatformLocator`) for that target.
5. Tests: run against `tempfile` files and accept `Ok(None)`/unsupported where
   the CI filesystem (tmpfs/overlayfs) lacks the feature.

### R4. Add a new OS seam (a new trait)
1. Define the trait in `platform/<seam>/mod.rs` with a no-op/neutral impl
   (`NoX`) and a blanket `impl<T: Trait + ?Sized> Trait for &T`.
2. Add a type parameter with a default to `Engine`
   (`Engine<D = PlatformProbe, H = PlatformHints, L = PlatformLocator, X = PlatformX>`),
   extend `with_parts`, and thread `&self.x` into the engine function that needs it.
3. Update tests that call `Engine::with_parts` (unit tests in `engine/prepare.rs`,
   `tests/copy_modes.rs`).

### R5. Add an ordering policy
1. `domain/plan/tuning.rs`: add a variant to `OrderKind` + `Display` text.
2. `engine/order.rs`: add a unit struct implementing `OrderingPolicy`; add the
   match arm in `sort_jobs`. Prefer stable sorts (deterministic output).
3. `cli/args/value.rs`: add the `OrderArg` variant (doc comment = help text) and
   the `From<OrderArg> for OrderKind` arm.
4. Optionally make it a default in `domain/plan/pipeline.rs::profile()`.
5. Tests: policy unit test in `order.rs`; planner test if it became a default.

### R6. Add a copy strategy
1. `domain/plan/<name>.rs`: `<Name>Plan` struct + pure `<name>_plan(...)`
   function with unit tests; `pub mod` + `pub use` in `domain/plan/mod.rs`.
2. `domain/plan/mod.rs`: add `Strategy::<Name>(<Name>Plan)`; extend
   `Strategy::order`, `Display`, `plan()` selection and `auto_reason` if auto
   mode can pick it. Add a `Mode::<Name>` if it can be forced.
3. `engine/<name>/mod.rs`: `run_<name>(jobs, plan, …, progress, cancel) ->
   Result<CopyStats, EngineError>`. Reuse `engine/queue.rs` (`JobQueue`),
   `util::fs::RemoveOnDrop`, `engine/finalize.rs`, `engine/stats::combine`,
   `util::sync::join_scoped`. Scoped threads; join all before inspecting.
4. `engine/mod.rs`: `pub mod <name>;` and the match arm in `Engine::execute`.
5. `cli/ui/layout.rs`: `UiLayout` variant + `From<&Strategy>` arm;
   `cli/ui/mod.rs`: bars for it. `cli/args/value.rs`: `ModeArg` variant.
   `cli/args/convert.rs`: `FlagsUsed::hint_for` arm if it ignores some flags.
6. Tests: engine unit tests with `engine::testing::Fixture` + `assert_copied`
   (chunk boundaries, cancellation leaves no partial files, error propagation),
   plus one `tests/copy_modes.rs` case asserting `assert_trees_equal`.

### R7. Add an error variant
1. Pick the layer enum: `EngineError` (`engine/error.rs`), `ProbeError`
   (`platform/device/mod.rs`), `ParseByteSizeError` (`util/units.rs`).
2. Message: lowercase, quote paths `'{path}'`, include `#[source]` for causes.
3. Cancellation-like variants must return `true` from `Cancellation::is_cancellation`.
4. A new layer enum → add `#[error(transparent)] X(#[from] XError)` to
   `crate::Error` in `src/error.rs` and decide its `exit_code`.
5. Test the message or the mapping (see `error.rs` tests).

### R8. Add a CLI flag
1. Pick the group struct in `cli/args/copy.rs`: `SafetyArgs` ("Safety"),
   `PerfArgs` ("Performance"), `DetectArgs` ("Disk detection"), or
   `OutputArgs` in `cli/args/mod.rs` (global, `global = true`).
   **Never** add fields to `CopyPaths` other than positionals (clap
   `Option<flatten>` pitfall — see fastcp-codebase/references/pitfalls.md).
2. Doc comment = help text (imperative, no trailing period); `value_name`
   in CAPS; sizes are `Option<ByteSize>`; counts `Option<NonZeroUsize>`;
   enums get a `ValueEnum` in `cli/args/value.rs` mirrored by a domain enum.
   Consider `env = "FASTCP_<NAME>"` for tuning knobs.
3. Plumb it: `cli/args/convert.rs` (`From<CopyArgs> for CopyRequest`) →
   `domain` field (`Tuning`, `CopyRequest`, …) → consumer.
4. If the flag only applies to some strategies, extend `FlagsUsed` and `hint_for`.
   Copy flags are rejected alongside `probe`/`completions` automatically by
   `Cli::try_parse_args`; a flag meant for every subcommand needs `global = true`
   and belongs in `OutputArgs`.
5. Tests in `cli/args/mod.rs` (use the `parse` helper, which goes through
   `Cli::try_parse_args`): parses, maps to the domain value, rejects bad
   input (`ErrorKind::ValueValidation`); keep `clap_definition_is_valid` green.
6. Update `EXAMPLES` if it is a headline feature.

### R9. Add a subcommand
1. `cli/args/mod.rs`: `XArgs` struct (`#[derive(Debug, Clone, Args)]`) and a
   `Command::X(XArgs)` variant with a doc comment (= help line).
2. `cli/commands/x.rs`: `impl Run for XArgs { fn run(self, ctx: &Context) -> Result<ExitCode> }`;
   `mod x;` + match arm in `cli/commands/mod.rs`.
3. Print via `ctx.out` (stderr) — stdout is reserved for machine output
   (like `completions`). Handle `BrokenPipe` when writing to stdout.
4. Tests: parse test in `cli/args/mod.rs`; black-box test in `tests/cli.rs`.

### R10. Add a progress event
1. `engine/progress.rs`: new method on `ProgressSink` **with a default no-op
   body**, and forward it in `impl ProgressSink for &P`.
2. Emit it from the engine; implement it in `cli/ui/mod.rs` (`CopyUi` or
   `ScanSpinner`). `NoProgress` needs no change.

### R11. Remove a feature end-to-end
Reverse the matching recipe, top layer first (cli → engine → platform →
domain → util), so each step compiles: remove the flag/variant from clap,
then the strategy/adapter, then the domain type, then orphaned helpers.
After each layer: `cargo check --all-targets`. Finally grep `.agents/` and
`README`/docs for stale references.

## 3. Change-impact table

| you changed | also update |
|---|---|
| `[[bin]] name` | `#[command(name)]` in `cli/args/mod.rs`, `EXAMPLES`, `env!("CARGO_BIN_EXE_<name>")` in `tests/cli.rs`, scripts |
| a domain enum | its CLI mirror in `cli/args/value.rs` + `From` impl |
| `Strategy` | `Strategy::order`, `Display`, `UiLayout`, `Engine::execute`, `FlagsUsed::hint_for` |
| `PipelinePlan` fields | `pipeline_plan`, `Display` for `Strategy`, pipeline tests' `plan()` helpers |
| `FileJob` fields | `engine/scan.rs`, `domain/job.rs::fixtures`, `engine/testing.rs`, `engine/filter.rs` tests |
| `Engine` type params | `Engine::new`, `with_parts`, `tests/copy_modes.rs`, `engine/prepare.rs` tests |
| public API | `lib.rs` re-exports + crate doc example |
| a developer task | new `scripts/<name>.sh` from `scripts/_template.sh` + `Makefile` target with `## help` + row in `scripts/README.md` |
| layer of a module | `fastcp-ci/scripts/check_layers.sh` rules if they change, fastcp-codebase tables |
