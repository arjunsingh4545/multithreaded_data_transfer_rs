#!/usr/bin/env bash
# uninstall.sh — remove an installed fastcp binary and its completion files.
#
# Usage: scripts/uninstall.sh [options]
#   --dir DIR    directory the binary was installed to (default: ~/.local/bin; env PREFIX → $PREFIX/bin)
#   -h, --help   show this help
#
# Exit codes: 0 success (also when nothing was installed), 1 failure, 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

dir="${PREFIX:+$PREFIX/bin}"; dir="${dir:-$HOME/.local/bin}"
while (($#)); do
  case "$1" in
    --dir)     [[ $# -ge 2 ]] || usage_error "$1 needs a value"; dir="$2"; shift ;;
    -h|--help) print_help; exit 0 ;;
    *)         usage_error "unknown option $1" ;;
  esac
  shift
done

data="${XDG_DATA_HOME:-$HOME/.local/share}"
config="${XDG_CONFIG_HOME:-$HOME/.config}"
removed=0
for file in "$dir/$BIN_NAME" \
            "$data/bash-completion/completions/$BIN_NAME" \
            "$HOME/.zfunc/_$BIN_NAME" \
            "$config/fish/completions/$BIN_NAME.fish"; do
  if [[ -e "$file" ]]; then
    run rm -f "$file"
    removed=$((removed + 1))
  fi
done

if ((removed)); then ok "removed $removed file(s)"; else info "nothing to remove"; fi
