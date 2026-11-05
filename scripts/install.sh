#!/usr/bin/env bash
# install.sh — install the fastcp binary (and optionally shell completions).
#
# Usage: scripts/install.sh [options]
#   --dir DIR        install directory (default: ~/.local/bin; env PREFIX → $PREFIX/bin)
#   --bin PATH       binary to install (default: target/release/fastcp, built if missing)
#   --completions    also install completions for $SHELL (bash, zsh or fish)
#   -h, --help       show this help
#
# Never uses sudo. For a system-wide install run e.g.:
#   sudo scripts/install.sh --dir /usr/local/bin
#
# Exit codes: 0 success, 1 failure, 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

dir="${PREFIX:+$PREFIX/bin}"; dir="${dir:-$HOME/.local/bin}"
bin="" completions=0
while (($#)); do
  case "$1" in
    --dir)         [[ $# -ge 2 ]] || usage_error "$1 needs a value"; dir="$2"; shift ;;
    --bin)         [[ $# -ge 2 ]] || usage_error "$1 needs a value"; bin="$2"; shift ;;
    --completions) completions=1 ;;
    -h|--help)     print_help; exit 0 ;;
    *)             usage_error "unknown option $1" ;;
  esac
  shift
done

if [[ -z "$bin" ]]; then
  bin="$ROOT/target/release/$BIN_NAME"
  [[ -x "$bin" ]] || run "$ROOT/scripts/build.sh"
fi
[[ -x "$bin" ]] || die "no executable at $bin"

run mkdir -p "$dir"
run install -m 0755 "$bin" "$dir/$BIN_NAME"
ok "installed $dir/$BIN_NAME"

case ":$PATH:" in
  *":$dir:"*) ;;
  *) warn "$dir is not on PATH — add to your shell rc:  export PATH=\"$dir:\$PATH\"" ;;
esac

((completions)) && run "$ROOT/scripts/completions.sh" --install --bin "$dir/$BIN_NAME"
exit 0
