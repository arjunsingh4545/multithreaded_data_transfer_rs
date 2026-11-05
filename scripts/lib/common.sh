# Shared helpers for scripts/*.sh. Source it, don't run it:
#
#   source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"
#
# Provides:
#   ROOT                crate root (absolute), and the shell is cd'ed into it
#   BIN_NAME            binary name from Cargo.toml's [[bin]] (fastcp)
#   MSRV                rust-version from Cargo.toml
#   info/ok/warn MSG    status lines on stderr
#   die MSG [CODE]      error on stderr, exit CODE (default 1)
#   usage_error MSG     error + hint, exit 2
#   need CMD [HINT]     die unless CMD is on PATH
#   run CMD...          print the command, then run it
#   print_help          print the script's leading comment block
#   version_ge A B      true if version A >= B (dotted numbers)

# shellcheck shell=bash
[[ -n "${_FASTCP_COMMON:-}" ]] && return 0
_FASTCP_COMMON=1

# Colour only for terminals, and never when NO_COLOR is set (https://no-color.org).
if [[ -t 2 && -z "${NO_COLOR:-}" ]]; then
  _c_info=$'\033[1;36m' _c_ok=$'\033[1;32m' _c_warn=$'\033[1;33m' _c_err=$'\033[1;31m' _c_dim=$'\033[2m' _c_off=$'\033[0m'
else
  _c_info='' _c_ok='' _c_warn='' _c_err='' _c_dim='' _c_off=''
fi

info()  { printf '%s▸%s %s\n' "$_c_info" "$_c_off" "$*" >&2; }
ok()    { printf '%s✔%s %s\n' "$_c_ok" "$_c_off" "$*" >&2; }
warn()  { printf '%swarning:%s %s\n' "$_c_warn" "$_c_off" "$*" >&2; }
die()   { printf '%serror:%s %s\n' "$_c_err" "$_c_off" "$1" >&2; exit "${2:-1}"; }
usage_error() { printf '%serror:%s %s\n  try: %s --help\n' "$_c_err" "$_c_off" "$1" "${0##*/}" >&2; exit 2; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "'$1' not found${2:+ — $2}"
}

run() {
  printf '%s  $ %s%s\n' "$_c_dim" "$*" "$_c_off" >&2
  "$@"
}

# Prints the comment block after the shebang (the script's usage text).
print_help() {
  awk 'NR == 1 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "$0"
}

version_ge() {
  [[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" == "$2" ]]
}

# Reads a top-level `key = "value"` from Cargo.toml (first match).
_toml_value() {
  sed -nE "s/^$1[[:space:]]*=[[:space:]]*\"([^\"]+)\".*/\1/p" "$ROOT/Cargo.toml" | head -n1
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
[[ -f "$ROOT/Cargo.toml" ]] || die "Cargo.toml not found in $ROOT"
cd "$ROOT"

# [[bin]] name if declared, otherwise the package name.
BIN_NAME="$(awk '/^\[\[bin\]\]/ { inbin = 1; next } /^\[/ { inbin = 0 }
  inbin && /^name[[:space:]]*=/ { gsub(/.*=[[:space:]]*"|".*/, ""); print; exit }' Cargo.toml)"
[[ -n "$BIN_NAME" ]] || BIN_NAME="$(_toml_value name)"
MSRV="$(_toml_value rust-version)"
export ROOT BIN_NAME MSRV
