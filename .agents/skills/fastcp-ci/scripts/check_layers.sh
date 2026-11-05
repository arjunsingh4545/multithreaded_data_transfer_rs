#!/usr/bin/env bash
# Enforces fastcp's layer boundaries on non-test code.
#
#   util ◄── domain ◄── platform ◄── engine ◄── cli
#
# Rules (see .agents/skills/fastcp-codebase/SKILL.md §2):
#   * a layer may only `use crate::<lower layer>` (engine may also use crate::error)
#   * clap / clap_complete / indicatif / console / tokio only in cli
#   * libc and `unsafe` only in platform
#   * rayon / crossbeam_channel / tokio_util not in util or domain
#
# Test code is ignored: everything from a `#[cfg(test)]` line that is directly
# followed by an inline `mod name {` to the end of the file. (Convention: test
# modules are the last item in each file.)
#
# Usage: check_layers.sh [crate-root]     (default: git toplevel or cwd)
# Exit:  0 = clean, 1 = violations found, 2 = bad invocation
set -euo pipefail

root="${1:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
src="$root/src"
[[ -d "$src" ]] || { echo "check_layers: no src/ under $root" >&2; exit 2; }

declare -A allowed=(
  [util]=""
  [domain]="util"
  [platform]="util domain"
  [engine]="util domain platform error"
  [cli]="util domain platform engine error cli"
)
layers=(util domain platform engine cli)
violations=0

report() { # file line message
  printf '  %s:%s: %s\n' "${1#"$root"/}" "$2" "$3"
  violations=$((violations + 1))
}

# Print a file without its trailing test module(s), preserving line numbers.
strip_tests() {
  awk '
    pending {
      if ($0 ~ /^[[:space:]]*(pub(\(crate\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*\{/) exit
      print pending_line; pending = 0
    }
    /^[[:space:]]*#\[cfg\(test\)\][[:space:]]*$/ { pending = 1; pending_line = $0; next }
    { print }
  ' "$1"
}

# Lines that are not pure comments (keeps doc text like "home of `unsafe`" out of the way).
code_lines() { grep -nv '^[[:space:]]*//' || true; }

for layer in "${layers[@]}"; do
  dir="$src/$layer"
  [[ -d "$dir" ]] || continue
  while IFS= read -r -d '' file; do
    body="$(strip_tests "$file" | code_lines)"
    [[ -n "$body" ]] || continue

    # 1. crate-internal imports
    while IFS=: read -r line target; do
      [[ -z "$target" ]] && continue
      [[ "$target" == "$layer" ]] && continue
      [[ " ${allowed[$layer]} " == *" $target "* ]] && continue
      report "$file" "$line" "$layer must not depend on crate::$target"
    done < <(printf '%s\n' "$body" | grep -oE '^[0-9]+:.*' |
             sed -nE 's/^([0-9]+):.*crate::(util|domain|platform|engine|cli|error)\b.*/\1:\2/p')

    # 2. external crates restricted to certain layers
    check_pattern() { # regex message
      while IFS=: read -r line _; do
        report "$file" "$line" "$2"
      done < <(printf '%s\n' "$body" | grep -E "^[0-9]+:.*$1" || true)
    }
    if [[ "$layer" != cli ]]; then
      check_pattern '\b(clap|clap_complete|indicatif|console)::' "terminal crates belong in cli"
      check_pattern '\btokio::' "the tokio runtime belongs in cli (tokio_util is fine in engine)"
    fi
    if [[ "$layer" != platform ]]; then
      check_pattern '\blibc::' "libc belongs in platform"
      check_pattern '\bunsafe\b' "unsafe belongs in platform"
    fi
    if [[ "$layer" == util || "$layer" == domain ]]; then
      check_pattern '\b(rayon|crossbeam_channel|tokio_util)::' "concurrency crates belong in engine or above"
    fi
  done < <(find "$dir" -name '*.rs' -print0 | sort -z)
done

# 3. every unsafe block in platform is documented (clippy also enforces this)
while IFS=: read -r file line _; do
  prev=$((line - 1)); start=$((line > 6 ? line - 6 : 1))
  if ! sed -n "${start},${prev}p;${line}p" "$file" | grep -q 'SAFETY:'; then
    report "$file" "$line" "unsafe block without a // SAFETY: comment"
  fi
done < <(grep -rnE '\bunsafe[[:space:]]*\{' "$src/platform" --include='*.rs' || true)

if ((violations > 0)); then
  echo "check_layers: $violations violation(s)" >&2
  exit 1
fi
echo "check_layers: ok"
