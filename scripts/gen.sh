#!/bin/sh
# Writes cli/src/lib.rs from the modules in cli/src and cli/src/cmd/mod.rs from the commands in cli/src/cmd;
# a file is rewritten only when its bytes change.
set -eu
cd "$(dirname "$0")/.."
export LC_ALL=C
write() {
  cmp -s "$2" "$1" || cat "$2" > "$1"
  rm -f "$2"
}
src=cli/src
cmds=$(for f in "$src"/cmd/*.rs; do
  n=${f##*/}
  n=${n%.rs}
  if [ -e "$f" ] && [ "$n" != mod ]; then echo "$n"; fi
done | sort)
mkdir -p "$src/cmd"
tmp=$(mktemp)
{
  for n in $cmds; do echo "pub mod $n;"; done
  cat <<'RS'

use std::process::ExitCode;

/// One `beacon <name>` command: its usage line and its entry point.
pub struct Command {
    pub name: &'static str,
    pub usage: &'static str,
    pub run: fn(&[&str]) -> Result<ExitCode, String>,
}

#[rustfmt::skip]
pub const ALL: &[Command] = &[
RS
  for n in $cmds; do echo "    Command { name: \"$n\", usage: $n::USAGE, run: $n::run },"; done
  cat <<'RS'
];

/// Every command's usage line, for an unknown or missing command.
pub fn usage() -> String {
    let lines: Vec<_> = ALL.iter().map(|c| c.usage).collect();
    format!("usage: {}", lines.join("\n       "))
}
RS
} > "$tmp"
write "$src/cmd/mod.rs" "$tmp"
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
