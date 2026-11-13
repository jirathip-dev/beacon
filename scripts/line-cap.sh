#!/bin/sh
# Non-test lines must stay at most line_cap in ci/cap.toml. Counted: non-blank lines of cli/src/**/*.rs
# before the file's first `#[cfg(test)]`, plus non-blank lines of app/**/*.swift outside test targets (*Tests/).
set -eu
cd "$(dirname "$0")/.."
cap=$(sed -n 's/^line_cap *= *\([0-9]*\).*/\1/p' ci/cap.toml)
test -n "$cap" || { echo "line_cap missing from ci/cap.toml" >&2; exit 2; }
n=$({
  find cli/src -name '*.rs' -exec awk '/^[[:space:]]*#\[cfg\(test\)\]/ { exit } NF { n++ } END { print n + 0 }' {} \;
  find app -name '*.swift' ! -path '*Tests/*' -exec awk 'NF { n++ } END { print n + 0 }' {} \;
} | awk '{ s += $1 } END { print s + 0 }')
echo "non-test lines: $n / $cap"
test "$n" -le "$cap"
