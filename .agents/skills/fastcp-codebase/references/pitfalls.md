# Pitfalls and non-obvious behaviour

Each entry: symptom → cause → what to do.

## clap: `Option<FlattenedArgs>` is always `None` if the struct has nested `flatten`
- Symptom: bare `SRC DST` form exited 2 silently; `into_command()` returned `None`.
- Cause: clap_derive 4.6 leaves the auto `ArgGroup` empty for any `Args`
  struct containing `#[command(flatten)]` fields ("HACK: Validation isn't
  ready yet for nested arg groups"), and `Option<T>` detection checks that group.
- Fix in place: `Cli` flattens `Option<CopyPaths>` (positionals only, no nested
  flatten) plus non-optional `SafetyArgs`/`PerfArgs`/`DetectArgs`, and
  `Cli::into_command` reassembles `CopyArgs`. Keep `CopyPaths` flat. Test:
  `cli::args::tests::bare_paths_parse_as_copy`.

## Global flags before a subcommand (`fastcp -q probe /`)
clap's `args_conflicts_with_subcommands` also counts *global* flags, so with it
`-q probe /` parsed `probe` as SRC. The crate instead uses
`subcommand_negates_reqs` and `Cli::try_parse_args` (`cli/args/mod.rs`), which
rejects only top-level **non-global** args given on the command line
(`copy_only_arg`) when a subcommand is present. Always parse through
`Cli::parse_args`/`try_parse_args`, never `Cli::parse`, or the conflict check
is skipped. New global flags need `global = true`; new copy flags are covered
automatically. Tests: `output_flags_may_precede_a_subcommand`,
`copy_flags_conflict_with_other_subcommands`.

## Directory metadata is not preserved
`ScanManifest::directories` is `Vec<PathBuf>`; `skeleton::create_dirs` only
creates them. Adding it means storing `FileMeta` per directory and applying it
**after** all files are written (writing into a dir updates its mtime).

## Scoped threads: join everything before looking at results
Returning early from `thread::scope` while a spawned thread panicked and was
not joined makes the scope itself panic. `run_parallel` / `run_pipeline`
collect *all* `join_scoped` results into a `Vec` first.

## Writers end only when *all* `Router` clones are dropped
`run_pipeline` must `drop(router)` after spawning readers, or writers block
forever on `recv`.

## `BufferPool::acquire` must stay cancellation-aware
If a writer dies, queued messages (holding buffers) are not freed until the
channel is destroyed; a reader blocked in a plain `recv` would deadlock. The
50 ms `recv_timeout` poll is load-bearing.

## `read_full_at` short read means EOF
The reader stops when `n < chunk`. If you replace `read_full_at` with plain
`read_at`, short reads mid-file would truncate copies.

## `PendingFile::finish` trims to the bytes actually read
`fallocate` extends the file to the scanned size; if the source shrank, the
`End{len}` value (not `FileJob::size`) is authoritative.

## FIEMAP returns `None` on many filesystems
tmpfs, exFAT (likely), FUSE — `Fiemap` maps EOPNOTSUPP/ENOTTY to `Ok(None)`,
and `ByPhysical` falls back to path order. Unit tests accept `None`.

## dm/LVM/md devices lie about `rotational`
They often report `0` over spinning disks; `sysfs.rs` resolves `slaves/`.
USB enclosures may also misreport — users override with `--src-disk/--dst-disk`.

## Metadata on NTFS/exFAT
chmod may be refused or ignored; this is counted, not fatal. Tests that check
modes run on the temp filesystem (usually ext4/tmpfs).

## mtime tolerance for `--if-exists update`
2 s (FAT/exFAT granularity). Size must match exactly.

## `Output` is `Copy`
Pass it by value to free functions (`clippy::trivially_copy_pass_by_ref`).

## Integration-test helpers and `unwrap_used`
`clippy.toml`'s `allow-unwrap-in-tests` covers `#[test]` fns and `#[cfg(test)]`
modules, **not** plain helper fns in `tests/*.rs` or `tests/common/`. Those
crates need a crate-level `#![expect(clippy::unwrap_used, reason = "…")]`.

## The Ctrl-C integration test is timing-based
`tests/cli.rs::ctrl_c_exit_code_is_130_and_partial_files_are_removed` waits for
the `plan` line on stderr (printed after the signal handler is installed),
then sends `kill -INT`. It copies a 2 GiB sparse file in 4 KiB chunks to stay
busy. It needs the `kill` binary and enough free space for a partial file.

## Two names: library crate vs binary
- Library crate / package: `multithreaded_data_transfer_rs` (all `use` paths,
  doctests, `tests/*.rs` imports).
- Binary: `fastcp` (`[[bin]]` in `Cargo.toml`), so `target/release/fastcp` and
  `env!("CARGO_BIN_EXE_fastcp")` in `tests/cli.rs`.
- clap: `#[command(name = "fastcp")]` in `cli/args/mod.rs` — keep it equal to the
  `[[bin]]` name, otherwise `--help` and generated completions use the wrong name.
  `EXAMPLES` in the same file uses `fastcp` too.

## `clap_complete::generate` panics on write errors
`fastcp completions bash | head` closed stdout early and panicked. The command
renders into a `Vec<u8>` first and treats `BrokenPipe` as success.
