#!/usr/bin/env bash
# lint.sh — static checks: layer boundaries, clippy (pedantic, warnings as errors), shellcheck.
#
# Usage: scripts/lint.sh [--fix]
#   --fix       let clippy apply machine-applicable fixes first (needs a clean or staged tree)
#   -h, --help  show this help
#
# Exit codes: 0 clean, 1 findings, 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

fix=0
while (($#)); do
  case "$1" in
    --fix)     fix=1 ;;
    -h|--help) print_help; exit 0 ;;
    *)         usage_error "unknown option $1" ;;
  esac
  shift
done

need cargo "install Rust from https://rustup.rs"
run bash "$ROOT/.agents/skills/fastcp-ci/scripts/check_layers.sh" "$ROOT"
((fix)) && run cargo clippy --locked --all-targets --all-features --fix --allow-staged
run cargo clippy --locked --all-targets --all-features -- -D warnings

if command -v shellcheck >/dev/null; then
  mapfile -t scripts < <(find "$ROOT/scripts" "$ROOT/.agents/skills" -name '*.sh' | sort)
  run shellcheck -x "${scripts[@]}"
else
  warn "shellcheck not installed — skipping shell script lint"
fi
ok "lint clean"
