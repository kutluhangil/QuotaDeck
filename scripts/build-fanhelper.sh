#!/bin/sh
# Builds the fan helper. Run from the repository root.
#
#   TEAM_ID=ABCDE12345 scripts/build-fanhelper.sh   release: only our Developer ID team may connect
#   FANHELPER_DEV=1   scripts/build-fanhelper.sh    local test: any binary signed with the helper's
#                                                   identifier may connect (scripts/fanhelper-dev.sh)
#   scripts/build-fanhelper.sh                      compile check and `selftest` only; the daemon
#                                                   refuses to start
set -eu
out="${1:-target/fanhelper}"
id="com.kutluhangil.quotadeck.fanhelper"
if [ -n "${TEAM_ID:-}" ]; then
  requirement="identifier \"${id}\" and anchor apple generic and certificate leaf[subject.OU] = \"${TEAM_ID}\""
elif [ "${FANHELPER_DEV:-}" = "1" ]; then
  requirement="identifier \"${id}\""
else
  requirement="unconfigured"
fi
mkdir -p "$out"
# `|` as the sed delimiter: the requirement contains slashes in no case, quotes in all.
sed "s|@PEER_REQUIREMENT@|${requirement}|" app/fanhelper/main.swift > "$out/main.swift"
swiftc -O -target arm64-apple-macos13 -o "$out/quotadeck-fanhelper" "$out/main.swift"
rm "$out/main.swift"
echo "$out/quotadeck-fanhelper"
