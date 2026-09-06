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
WIDGET_NAME="QuotaDeckWidget"
WIDGET_PROFILE="app/widget/${WIDGET_NAME}.provisionprofile"
TARGET="universal-apple-darwin"
BUNDLE_DIR="target/${TARGET}/release/bundle/macos"
APP="${BUNDLE_DIR}/${APP_NAME}.app"
PKG="target/${APP_NAME// /}.pkg"

if [[ ! -f "${PROFILE}" ]]; then
  echo "error: ${PROFILE} is missing — download the Mac App Store profile for ${IDENTIFIER}" >&2
  exit 1
fi

if [[ ! -f "${WIDGET_PROFILE}" ]]; then
  echo "error: ${WIDGET_PROFILE} is missing — download the profile for ${IDENTIFIER}.widget" >&2
  echo "       an appex embeds its own profile; the host's does not cover it" >&2
  exit 1
fi

# The Team ID is substituted into a copy rather than committed. Cleaned up on any exit so a
# failed run does not leave an account identifier in the working tree.
ENTITLEMENTS="$(mktemp -t quotadeck-entitlements).plist"
WIDGET_ENTITLEMENTS="$(mktemp -t quotadeck-widget-entitlements).plist"
trap 'rm -f "${ENTITLEMENTS}" "${WIDGET_ENTITLEMENTS}"' EXIT
sed "s/\$TEAM_ID/${TEAM_ID}/g" app/Entitlements.appstore.plist > "${ENTITLEMENTS}"
sed "s/\$TEAM_ID/${TEAM_ID}/g" app/widget/Entitlements.widget.plist > "${WIDGET_ENTITLEMENTS}"

# The host reads its App Group identifier from this at compile time. Absent in every other
# build, which is why `widget::publish` is a no-op there rather than a failure.
export QUOTADECK_APP_GROUP="${TEAM_ID}.com.kutluhangil.quotadeck.shared"

echo "==> bundling"
npm --prefix ui ci
npm --prefix ui run build
node scripts/check-appstore-config.mjs
npm --prefix ui exec tauri -- build \
  --bundles app \
  --target "${TARGET}" \
  --config app/tauri.appstore.conf.json

echo "==> building and embedding the widget extension"
bash scripts/widget.sh
PLUGINS="${APP}/Contents/PlugIns"
mkdir -p "${PLUGINS}"
rm -rf "${PLUGINS}/${WIDGET_NAME}.appex"
cp -R "target/widget/${WIDGET_NAME}.appex" "${PLUGINS}/"
cp "${WIDGET_PROFILE}" "${PLUGINS}/${WIDGET_NAME}.appex/Contents/embedded.provisionprofile"

# Inside out, deliberately not --deep.
#
# --deep re-signs nested code with the *host's* entitlements. That would hand the appex
# `files.user-selected.read-only`, a capability it has no claim to and which Asset Validation
# rejects for disagreeing with the widget's own provisioning profile. Each nested item is
# therefore signed with what belongs to it, and the host last.
echo "==> signing the widget"
codesign --sign "Apple Distribution" \
  --entitlements "${WIDGET_ENTITLEMENTS}" \
  --options runtime --timestamp --force \
  "${PLUGINS}/${WIDGET_NAME}.appex"

if [[ -d "${APP}/Contents/Frameworks" ]]; then
  echo "==> signing bundled frameworks"
  find "${APP}/Contents/Frameworks" -mindepth 1 -maxdepth 1 -print0 |
    while IFS= read -r -d '' item; do
      codesign --sign "Apple Distribution" --options runtime --timestamp --force "${item}"
    done
fi

echo "==> signing the app"
# --force because the bundler already applied an ad-hoc signature that this has to replace.
codesign --sign "Apple Distribution" \
  --entitlements "${ENTITLEMENTS}" \
  --options runtime --timestamp --force \
  "${APP}"

codesign --verify --deep --strict --verbose=2 "${APP}"

echo "==> verifying the widget kept its own entitlements"
# The failure this catches is a --deep signature creeping back in: the appex would then carry
# the host's file-access entitlement, and Asset Validation would reject the upload.
WIDGET_SIGNED="$(codesign -d --entitlements :- "${PLUGINS}/${WIDGET_NAME}.appex" 2>/dev/null)"
if grep -q "\.network\." <<<"${WIDGET_SIGNED}"; then
  echo "error: the widget was signed with a network capability" >&2
  exit 1
fi
if grep -q "files.user-selected" <<<"${WIDGET_SIGNED}"; then
  echo "error: the widget was signed with the host's file access; --deep has crept back in" >&2
  exit 1
fi
if ! grep -q "application-groups" <<<"${WIDGET_SIGNED}"; then
  echo "error: the widget was signed without its app group; it can reach no data" >&2
  exit 1
fi

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

if otool -L "${APP}/Contents/MacOS/quotadeck" | grep -qi 'WebKit.*Private\|PrivateFrameworks'; then
  echo "error: the executable links a private framework" >&2
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
