#!/usr/bin/env bash
# build.sh — check prerequisites, build fastcp, smoke-test the binary.
#
# Usage: scripts/build.sh [options]
#   --profile NAME     release (default) or debug
#   --target TRIPLE    cross-compile, e.g. x86_64-unknown-linux-musl
#   --check            run every quality gate (scripts/check.sh) before building
#   --clean            cargo clean first
#   --install [DIR]    install afterwards (scripts/install.sh; default ~/.local/bin)
#   --completions      with --install: also install shell completions
#   -h, --help         show this help
#
# Exit codes: 0 success, 1 failure, 2 bad usage.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"

profile=release target="" check=0 clean=0 install=0 install_dir="" completions=0
while (($#)); do
  case "$1" in
    --profile)     [[ $# -ge 2 ]] || usage_error "$1 needs a value"; profile="$2"; shift ;;
    --target)      [[ $# -ge 2 ]] || usage_error "$1 needs a value"; target="$2"; shift ;;
    --check)       check=1 ;;
    --clean)       clean=1 ;;
    --install)     install=1
                   if [[ $# -ge 2 && "$2" != -* ]]; then install_dir="$2"; shift; fi ;;
    --completions) completions=1 ;;
    -h|--help)     print_help; exit 0 ;;
    *)             usage_error "unknown option $1" ;;
  esac
  shift
done
[[ "$profile" == release || "$profile" == debug ]] || usage_error "--profile must be release or debug"
((completions == 0 || install == 1)) || usage_error "--completions needs --install"

# ── 1. prerequisites ──
[[ "$(uname -s)" == Linux ]] || warn "fastcp is developed on Linux; $(uname -s) builds without disk detection hints"
need cargo "install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
need rustc "install Rust: https://rustup.rs"
need cc "install a C toolchain (Debian/Ubuntu: sudo apt install build-essential; Fedora: sudo dnf install gcc; Arch: sudo pacman -S base-devel)"
rust_version="$(rustc --version 2>/dev/null | awk '{print $2}')" || true
[[ -n "$rust_version" ]] || die "rustc is installed but does not run — with rustup, run: rustup default stable"
if [[ -n "$MSRV" ]] && ! version_ge "$rust_version" "$MSRV"; then
  die "rustc $rust_version is older than the required $MSRV — run: rustup update stable"
fi
info "rustc $rust_version (needs ≥ ${MSRV:-?}), cargo $(cargo --version | awk '{print $2}')"
if [[ -n "$target" ]] && command -v rustup >/dev/null && ! rustup target list --installed | grep -qx "$target"; then
  run rustup target add "$target"
fi

# ── 2. optional clean + quality gates ──
((clean)) && run cargo clean
((check)) && run "$ROOT/scripts/check.sh"

# ── 3. build ──
cargo_args=(build --locked)
[[ "$profile" == release ]] && cargo_args+=(--release)
[[ -n "$target" ]] && cargo_args+=(--target "$target")
run cargo "${cargo_args[@]}"

out_dir="$ROOT/target${target:+/$target}/$profile"
bin="$out_dir/$BIN_NAME"
[[ -x "$bin" ]] || die "build finished but $bin is missing"

# ── 4. smoke test (only when the binary runs on this machine) ──
if [[ -z "$target" ]] || "$bin" --version >/dev/null 2>&1; then
  run "$bin" --version >&2
  run "$bin" probe --color never "$ROOT" "$ROOT" >/dev/null 2>&1 || die "smoke test failed: $bin probe"
  ok "smoke test passed"
else
  warn "skipping smoke test: $target binaries do not run here"
fi

# ── 5. optional install ──
if ((install)); then
  install_args=(--bin "$bin")
  [[ -n "$install_dir" ]] && install_args+=(--dir "$install_dir")
  ((completions)) && install_args+=(--completions)
  run "$ROOT/scripts/install.sh" "${install_args[@]}"
fi

# ── 6. summary ──
size="$(du -h "$bin" | cut -f1)"
ok "built $BIN_NAME ($profile${target:+, $target}, $size)"
printf '  binary: %s\n' "$bin" >&2
((install)) || printf '  next:   %s probe SRC DST   or   scripts/install.sh\n' "$bin" >&2
