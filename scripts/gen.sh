#!/bin/sh
# Writes cli/src/lib.rs from the modules in cli/src, cli/src/cmd/mod.rs from the commands in cli/src/cmd and
# cli/src/serve/once/mod.rs, cli/src/serve/each/mod.rs from the parts there;
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
part() {
  names=$(for f in "$src/serve/$1"/*.rs; do
    n=${f##*/}
    n=${n%.rs}
    if [ -e "$f" ] && [ "$n" != mod ]; then echo "$n"; fi
  done | sort)
  mkdir -p "$src/serve/$1"
  tmp=$(mktemp)
  {
    for n in $names; do echo "mod $n;"; done
    echo
    echo "use super::hub::$2;"
    echo "use std::thread::Scope;"
    echo
    echo "/// $3"
    echo "pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub$4) {"
    for n in $names; do echo "    $n::start(s, hub$5);"; done
    echo "}"
  } > "$tmp"
  write "$src/serve/$1/mod.rs" "$tmp"
}
part once Hub "Starts the parts that live as long as serve." "" ""
part each "{Hub, Loaded}" "Starts the parts that run again for each valid config." ", config: &Loaded" ", config"
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
