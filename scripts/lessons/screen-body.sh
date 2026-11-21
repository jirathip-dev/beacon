#!/bin/sh
# Each ScrollView or .presentationDetents sheet in a file needs a .screenBody(); exits 1 with file:line otherwise.
cd "$(dirname "$0")/../.."
test "$#" -gt 0 || set -- app/Beacon/*.swift
fail=0
for f in "$@"; do
  awk -v f="$f" '
    /ScrollView *[({]|\.presentationDetents\(/ { n++; at[n] = FNR }
    { b += gsub(/\.screenBody\(/, "&") }
    END { for (i = b + 1; i <= n; i++) printf "%s:%d: screen body without .screenBody(): pad it with .screenBody() (the design .body inset), not literal .padding\n", f, at[i]; exit n > b }
  ' "$f" || fail=1
done
exit "$fail"
