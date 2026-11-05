#!/usr/bin/env bash
# fmt.sh — format the code, or only check formatting.
#
# Usage: scripts/fmt.sh [--check]
#   --check     fail instead of rewriting files (what CI runs)
#   -h, --help  show this help
#
# Exit codes: 0 formatted / already clean, 1 needs formatting (--check), 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

check=0
while (($#)); do
  case "$1" in
    --check)   check=1 ;;
    -h|--help) print_help; exit 0 ;;
    *)         usage_error "unknown option $1" ;;
  esac
  shift
done

need cargo "install Rust from https://rustup.rs"
if ((check)); then
  run cargo fmt --all -- --check || die "run scripts/fmt.sh (or make fmt) to fix formatting"
  ok "formatting is clean"
else
  run cargo fmt --all
  ok "formatted"
fi
