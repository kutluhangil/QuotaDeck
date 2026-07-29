#!/usr/bin/env bash
#
# Build, sign and upload the Mac App Store package.
#
# Everything this script needs is account data, so nothing here is checked in: certificates live
# in the login keychain and the rest comes from the environment. Run it from the repository root.
#
#   TEAM_ID=ABCDE12345 \
#   APPLE_API_KEY=... APPLE_API_ISSUER=... \
#   scripts/appstore.sh
#
# Prerequisites that have to exist before the first run, all created in the Apple Developer
# portal and downloaded once:
#
#   1. An "Apple Distribution" certificate in the login keychain.
#   2. A "3rd Party Mac Developer Installer" certificate in the login keychain.
#   3. A Mac App Store provisioning profile for com.kutluhangil.quotadeck, saved as
#      app/MacAppStore.provisionprofile.
#   4. An App Store Connect API key, as APPLE_API_KEY / APPLE_API_ISSUER.
#
# The `.app` is never uploaded directly; the store only accepts a `.pkg`.

set -euo pipefail

: "${TEAM_ID:?set TEAM_ID to your Apple Developer Team ID}"
: "${APPLE_API_KEY:?set APPLE_API_KEY to your App Store Connect key id}"
: "${APPLE_API_ISSUER:?set APPLE_API_ISSUER to your App Store Connect issuer id}"

APP_NAME="Quota Deck"
IDENTIFIER="com.kutluhangil.quotadeck"
PROFILE="app/MacAppStore.provisionprofile"
TARGET="universal-apple-darwin"
BUNDLE_DIR="target/${TARGET}/release/bundle/macos"
APP="${BUNDLE_DIR}/${APP_NAME}.app"
PKG="target/${APP_NAME// /}.pkg"

if [[ ! -f "${PROFILE}" ]]; then
  echo "error: ${PROFILE} is missing — download the Mac App Store profile for ${IDENTIFIER}" >&2
  exit 1
fi

# The Team ID is substituted into a copy rather than committed. Cleaned up on any exit so a
# failed run does not leave an account identifier in the working tree.
ENTITLEMENTS="$(mktemp -t quotadeck-entitlements).plist"
trap 'rm -f "${ENTITLEMENTS}"' EXIT
sed "s/\$TEAM_ID/${TEAM_ID}/g" app/Entitlements.appstore.plist > "${ENTITLEMENTS}"

echo "==> bundling"
npm --prefix ui ci
npm --prefix ui run build
cargo tauri build \
  --bundles app \
  --target "${TARGET}" \
  --config app/tauri.appstore.conf.json

echo "==> signing the app"
# --deep because the bundle carries its own frameworks; --force because the bundler already
# applied an ad-hoc signature that this has to replace.
codesign --sign "Apple Distribution" \
  --entitlements "${ENTITLEMENTS}" \
  --options runtime \
  --deep --force --timestamp \
  "${APP}"

echo "==> verifying the sandbox actually made it into the signature"
# The failure this catches is Asset Validation error 90296, "App sandbox not enabled", which is
# what a bundle whose entitlements were not embedded looks like from Apple's side. Cheaper to
# find here than after a twenty-minute upload.
if ! codesign -d --entitlements :- "${APP}" 2>/dev/null | grep -q "com.apple.security.app-sandbox"; then
  echo "error: the signed bundle carries no sandbox entitlement" >&2
  exit 1
fi
# The privacy claim, checked rather than trusted: no outbound-connection capability may appear.
if codesign -d --entitlements :- "${APP}" 2>/dev/null | grep -q "\.network\."; then
  echo "error: the signed bundle claims a network capability; the listing says it makes none" >&2
  exit 1
fi

echo "==> building the installer package"
xcrun productbuild \
  --sign "3rd Party Mac Developer Installer" \
  --component "${APP}" /Applications \
  "${PKG}"

echo "==> uploading"
xcrun altool --upload-app \
  --type macos \
  --file "${PKG}" \
  --apiKey "${APPLE_API_KEY}" \
  --apiIssuer "${APPLE_API_ISSUER}"

echo "done: ${PKG}"
