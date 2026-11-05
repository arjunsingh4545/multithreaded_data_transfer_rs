#!/usr/bin/env bash
# doc.sh — build the API documentation (warnings are errors), optionally open it.
#
# Usage: scripts/doc.sh [--open]
#   --open      open the docs in a browser afterwards
#   -h, --help  show this help
#
# Exit codes: 0 success, 1 doc errors, 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

open=()
while (($#)); do
  case "$1" in
    --open)    open=(--open) ;;
    -h|--help) print_help; exit 0 ;;
    *)         usage_error "unknown option $1" ;;
  esac
  shift
done

need cargo "install Rust from https://rustup.rs"
export RUSTDOCFLAGS="${RUSTDOCFLAGS:--D warnings}"
run cargo doc --locked --no-deps "${open[@]}"
ok "docs: target/doc/multithreaded_data_transfer_rs/index.html"
