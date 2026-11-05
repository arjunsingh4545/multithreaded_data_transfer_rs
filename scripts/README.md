# scripts/

Every developer task is a bash script here; the `Makefile` only aliases them,
so `make build` and `scripts/build.sh` do exactly the same thing.

| script | make target | does |
|---|---|---|
| `build.sh` | `build`, `debug` | prerequisite check (rustc ≥ MSRV, C linker) → `cargo build --locked` → smoke test; `--check`, `--clean`, `--target`, `--install [DIR]`, `--completions` |
| `check.sh` | `check`, `quick` | all quality gates via `.agents/skills/fastcp-ci/scripts/ci.sh` |
| `test.sh` | `test` | `all` / `unit` / `integration` / `cli` / `doc`, extra args after `--` |
| `fmt.sh` | `fmt`, `fmt-check` | `cargo fmt`, or `--check` |
| `lint.sh` | `lint` | layer rules + clippy `-D warnings` + shellcheck (if installed) |
| `doc.sh` | `doc` | rustdoc with warnings as errors, `--open` |
| `install.sh` | `install` | copy the binary to `~/.local/bin` (or `--dir`, or `$PREFIX/bin`) |
| `uninstall.sh` | `uninstall` | remove the binary and completion files |
| `completions.sh` | `completions` | print, or `--install`, bash/zsh/fish completions |
| `clean.sh` | `clean` | `cargo clean` (`--release-only`) |
| `_template.sh` | — | starting point for a new script |
| `lib/common.sh` | — | shared helpers (sourced, not executed) |

Every script supports `-h/--help`. Pass options through make with `ARGS="…"`.

## Conventions for new scripts

1. Start from `_template.sh`: `cp scripts/_template.sh scripts/<name>.sh && chmod +x scripts/<name>.sh`.
2. Header: shebang `#!/usr/bin/env bash`, then a comment block with a one-line
   summary, `Usage:`, every option, and exit codes. `--help` prints this block
   (`print_help`), so the comment *is* the documentation.
3. `set -euo pipefail`, then `source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"`.
   This `cd`s to the crate root, so scripts work from any directory, and sets
   `ROOT`, `BIN_NAME` (from `Cargo.toml`) and `MSRV`.
4. Output goes to **stderr** through `info` / `ok` / `warn` / `die`; stdout is
   for data a caller may pipe (e.g. `completions.sh` without `--install`).
   Colour only on a terminal; `NO_COLOR=1` disables it.
5. Run commands through `run cmd args…` so the user sees what is executed.
6. Exit codes: `0` success, `1` failure, `2` bad usage (`usage_error`).
   Unknown options are errors, never ignored.
7. Check tools with `need <cmd> "<how to install>"` before using them.
8. Never call `sudo`; let the user run the script with sudo when a system
   path is needed. Never write outside the repo except into the user's
   chosen install directory.
9. Temporary files: `mktemp -d` + `trap 'rm -rf "$tmp"' EXIT`.
10. Reuse other scripts by calling them (`run "$ROOT/scripts/x.sh"`) instead
    of copying their logic.
11. Add a `Makefile` target with a `## description` comment (it shows up in
    `make help`) and a row in the table above.
12. `make lint` runs `shellcheck -x` on every script when shellcheck is installed.
