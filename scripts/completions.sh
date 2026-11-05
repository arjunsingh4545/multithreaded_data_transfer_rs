#!/usr/bin/env bash
# completions.sh — print or install shell completions for fastcp.
#
# Usage: scripts/completions.sh [options]
#   --shell SHELL   bash, zsh, fish, elvish or powershell (default: basename of $SHELL)
#   --install       write to the shell's per-user completion directory instead of stdout
#   --bin PATH      fastcp binary to ask (default: target/release/fastcp, built if missing)
#   -h, --help      show this help
#
# Install locations:
#   bash  ${XDG_DATA_HOME:-~/.local/share}/bash-completion/completions/fastcp
#   zsh   ~/.zfunc/_fastcp   (needs `fpath+=~/.zfunc; autoload -Uz compinit; compinit` in ~/.zshrc)
#   fish  ${XDG_CONFIG_HOME:-~/.config}/fish/completions/fastcp.fish
#
# Exit codes: 0 success, 1 failure, 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

shell="$(basename "${SHELL:-bash}")" install=0 bin=""
while (($#)); do
  case "$1" in
    --shell)   [[ $# -ge 2 ]] || usage_error "$1 needs a value"; shell="$2"; shift ;;
    --install) install=1 ;;
    --bin)     [[ $# -ge 2 ]] || usage_error "$1 needs a value"; bin="$2"; shift ;;
    -h|--help) print_help; exit 0 ;;
    *)         usage_error "unknown option $1" ;;
  esac
  shift
done

if [[ -z "$bin" ]]; then
  bin="$ROOT/target/release/$BIN_NAME"
  [[ -x "$bin" ]] || run "$ROOT/scripts/build.sh" >&2
fi
[[ -x "$bin" ]] || die "no executable at $bin"

if ((install == 0)); then
  exec "$bin" completions "$shell"
fi

data="${XDG_DATA_HOME:-$HOME/.local/share}"
config="${XDG_CONFIG_HOME:-$HOME/.config}"
case "$shell" in
  bash) dest="$data/bash-completion/completions/$BIN_NAME" ;;
  zsh)  dest="$HOME/.zfunc/_$BIN_NAME" ;;
  fish) dest="$config/fish/completions/$BIN_NAME.fish" ;;
  *)    die "no standard install location for '$shell'; print it with: scripts/completions.sh --shell $shell" 2 ;;
esac

run mkdir -p "$(dirname "$dest")"
"$bin" completions "$shell" >"$dest"
ok "installed $shell completions → $dest"
if [[ "$shell" == zsh ]]; then
  info "make sure ~/.zshrc has: fpath+=~/.zfunc; autoload -Uz compinit; compinit"
fi
info "open a new shell to use them"
