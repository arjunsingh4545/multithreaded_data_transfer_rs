#!/usr/bin/env bash
# check.sh — run the quality gates: format, layer rules, clippy, tests, docs.
#
# Usage: scripts/check.sh [--quick] [--release]
#   --quick     format, layers, clippy and unit tests only (pre-commit speed)
#   --release   also build the release binary (CI does this)
#   -h, --help  show this help
#
# Thin wrapper over the canonical gate list in
# .agents/skills/fastcp-ci/scripts/ci.sh, so humans and agents run the same checks.
#
# Exit codes: 0 all gates passed, 1 a gate failed, 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

quick=0 release=0
while (($#)); do
  case "$1" in
    --quick)   quick=1 ;;
    --release) release=1 ;;
    -h|--help) print_help; exit 0 ;;
    *)         usage_error "unknown option $1" ;;
  esac
  shift
done

need cargo "install Rust from https://rustup.rs"
gates="$ROOT/.agents/skills/fastcp-ci/scripts/ci.sh"
[[ -f "$gates" ]] || die "missing $gates"
ci_args=()
((quick)) && ci_args+=(--quick)
((release)) || ci_args+=(--no-release)
exec bash "$gates" "${ci_args[@]}"
