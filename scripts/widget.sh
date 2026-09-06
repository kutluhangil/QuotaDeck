#!/usr/bin/env bash
#
# Build the WidgetKit extension and lay out the .appex bundle.
#
# Hand-assembled rather than driven by an .xcodeproj: an appex is a bundle with an Info.plist
# and an executable, and five hundred lines of generated pbxproj cannot be reviewed in a diff.
#
#   TEAM_ID=ABCDE12345 scripts/widget.sh
#
# Produces target/widget/QuotaDeckWidget.appex, unsigned. Signing belongs to scripts/appstore.sh,
# which needs an account; this script needs only a toolchain, so CI can run it to catch a Swift
# error without one.

set -euo pipefail

: "${TEAM_ID:?set TEAM_ID to your Apple Developer Team ID}"

NAME="QuotaDeckWidget"
OUT="target/widget/${NAME}.appex"
# Matches app/tauri.conf.json minimumSystemVersion. An appex must not claim a lower floor
# than the host that carries it.
DEPLOYMENT="13.0"

rm -rf "${OUT}"
mkdir -p "${OUT}/Contents/MacOS"

# Universal, because the host is: an appex whose architecture does not match its host is not
# loaded, and the failure is silent.
for arch in arm64 x86_64; do
  swiftc \
    -target "${arch}-apple-macos${DEPLOYMENT}" \
    -O \
    -parse-as-library \
    -o "target/widget/${NAME}-${arch}" \
    app/widget/QuotaDeckWidget.swift
done

lipo -create \
  "target/widget/${NAME}-arm64" \
  "target/widget/${NAME}-x86_64" \
  -output "${OUT}/Contents/MacOS/${NAME}"
rm -f "target/widget/${NAME}-arm64" "target/widget/${NAME}-x86_64"

sed "s/\$TEAM_ID/${TEAM_ID}/g" app/widget/Info.plist > "${OUT}/Contents/Info.plist"
plutil -lint "${OUT}/Contents/Info.plist" >/dev/null

echo "built ${OUT}"
