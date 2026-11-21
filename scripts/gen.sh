#!/bin/sh
# Writes cli/src/lib.rs from the modules in cli/src; a file is rewritten only when its bytes change.
set -eu
cd "$(dirname "$0")/.."
export LC_ALL=C
write() {
  cmp -s "$2" "$1" || cat "$2" > "$1"
  rm -f "$2"
}
src=cli/src
tmp=$(mktemp)
for f in "$src"/*.rs "$src"/*/mod.rs; do
  [ -e "$f" ] || continue
  case "$f" in "$src"/lib.rs | "$src"/main.rs) continue ;; esac
  n=${f#"$src"/}
  n=${n%.rs}
  n=${n%/mod}
  echo "pub mod $n;"
done | sort -u > "$tmp"
write "$src/lib.rs" "$tmp"
