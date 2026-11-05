#!/usr/bin/env bash
# test.sh — run the test suites.
#
# Usage: scripts/test.sh [SUITE] [-- CARGO_TEST_ARGS...]
#   SUITE       all (default), unit, integration, cli, doc
#   -h, --help  show this help
#
# Examples:
#   scripts/test.sh                         everything
#   scripts/test.sh unit -- engine::pipeline   one module, unit tests only
#   scripts/test.sh cli -- --nocapture         binary tests with output
#
# Exit codes: 0 all passed, 1 a test failed, 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

suite=all
extra=()
while (($#)); do
  case "$1" in
    all|unit|integration|cli|doc) suite="$1" ;;
    --)        shift; extra=("$@"); break ;;
    -h|--help) print_help; exit 0 ;;
    *)         usage_error "unknown suite or option $1" ;;
  esac
  shift
done

need cargo "install Rust from https://rustup.rs"
# A developer's tuning must not change test behaviour.
unset FASTCP_CHUNK FASTCP_MEM

case "$suite" in
  all)         run cargo test --locked --all-targets -- "${extra[@]}"
               run cargo test --locked --doc -- "${extra[@]}" ;;
  unit)        run cargo test --locked --lib -- "${extra[@]}" ;;
  integration) run cargo test --locked --test copy_modes --test cli -- "${extra[@]}" ;;
  cli)         run cargo test --locked --test cli -- "${extra[@]}" ;;
  doc)         run cargo test --locked --doc -- "${extra[@]}" ;;
esac
ok "$suite tests passed"
