#!/usr/bin/env bash
# <name>.sh — one line saying what this script does.
#
# Usage: scripts/<name>.sh [options] [ARGS]
#   -x, --example VALUE   what the option does (default: …)
#   -h, --help            show this help
#
# Exit codes: 0 success, 1 failure, 2 bad usage.
#
# Copy this file to scripts/<name>.sh, fill in the blanks, `chmod +x` it, and
# add a Makefile target that calls it. Conventions: scripts/README.md.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

example="default"
args=()
while (($#)); do
  case "$1" in
    -x|--example) [[ $# -ge 2 ]] || usage_error "$1 needs a value"; example="$2"; shift ;;
    -h|--help)    print_help; exit 0 ;;
    --)           shift; args+=("$@"); break ;;
    -*)           usage_error "unknown option $1" ;;
    *)            args+=("$1") ;;
  esac
  shift
done

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

need cargo "install Rust from https://rustup.rs"
info "doing the thing with $example (${#args[@]} argument(s))"
run true
ok "done"
