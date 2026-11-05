#!/usr/bin/env bash
# clean.sh — remove build output.
#
# Usage: scripts/clean.sh [--release-only]
#   --release-only  remove only target/release (keeps debug builds for fast tests)
#   -h, --help      show this help
#
# Exit codes: 0 success, 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

args=()
while (($#)); do
  case "$1" in
    --release-only) args=(--release) ;;
    -h|--help)      print_help; exit 0 ;;
    *)              usage_error "unknown option $1" ;;
  esac
  shift
done

need cargo "install Rust from https://rustup.rs"
run cargo clean "${args[@]}"
ok "cleaned"
