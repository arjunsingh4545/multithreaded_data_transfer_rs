#!/usr/bin/env bash
# Runs every fastcp quality gate in CI order; stops at the first failure.
#
# Usage: ci.sh [--quick] [--no-release]
#   --quick       fmt + layers + clippy + unit tests only (pre-commit speed)
#   --no-release  skip the release build
#
# Environment:
#   CARGO_FLAGS   extra flags for cargo (default: --locked)
#   RUSTDOCFLAGS  forwarded to `cargo doc` (default: -D warnings)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel 2>/dev/null || (cd "$here/../../../.." && pwd))"
cd "$root"

quick=0; release=1
for arg in "$@"; do
  case "$arg" in
    --quick) quick=1 ;;
    --no-release) release=0 ;;
    -h|--help) sed -n '2,11p' "$0"; exit 0 ;;
    *) echo "ci.sh: unknown flag $arg" >&2; exit 2 ;;
  esac
done

flags="${CARGO_FLAGS:---locked}"
export RUSTDOCFLAGS="${RUSTDOCFLAGS:--D warnings}"
# Tests must not inherit a developer's tuning.
unset FASTCP_CHUNK FASTCP_MEM

if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
  c_step=$'\033[1;36m' c_fail=$'\033[1;31m' c_ok=$'\033[1;32m' c_off=$'\033[0m'
else
  c_step='' c_fail='' c_ok='' c_off=''
fi

passed=()
step() { # name command...
  local name="$1"; shift
  printf '\n%s▸ %s%s\n  $ %s\n' "$c_step" "$name" "$c_off" "$*"
  local started=$SECONDS
  if "$@"; then
    passed+=("$name ($((SECONDS - started))s)")
  else
    printf '\n%s✖ %s failed%s\n' "$c_fail" "$name" "$c_off" >&2
    exit 1
  fi
}

step "format"        cargo fmt --all -- --check
step "layers"        bash "$here/check_layers.sh" "$root"
# shellcheck disable=SC2086
step "clippy"        cargo clippy $flags --all-targets --all-features -- -D warnings
if ((quick)); then
  # shellcheck disable=SC2086
  step "unit tests"  cargo test $flags --lib
else
  # shellcheck disable=SC2086
  step "tests"       cargo test $flags --all-targets
  # shellcheck disable=SC2086
  step "doc tests"   cargo test $flags --doc
  # shellcheck disable=SC2086
  step "rustdoc"     cargo doc $flags --no-deps
  if ((release)); then
    # shellcheck disable=SC2086
    step "release build" cargo build $flags --release
  fi
  # Optional supply-chain gates: run only when the tools are installed.
  if command -v cargo-deny >/dev/null; then step "cargo deny" cargo deny check; fi
  if command -v cargo-audit >/dev/null; then step "cargo audit" cargo audit; fi
fi

printf '\n%s✔ all gates passed%s\n' "$c_ok" "$c_off"
printf '  · %s\n' "${passed[@]}"
