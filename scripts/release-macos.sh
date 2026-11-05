#!/bin/sh
# Builds the GitHub release for macOS: a Developer ID-signed, notarized and stapled DMG that
# carries the fan helper. Run from the repository root. Publishes nothing; it prints the
# `gh release` command at the end.
#
# One-time setup (docs/USER_ACTIONS.md §7):
#   - a "Developer ID Application" certificate in the login keychain
#   - notary credentials: xcrun notarytool store-credentials quotadeck-notary \
#       --apple-id <apple id> --team-id <TEAM_ID>        (asks for an app-specific password)
#
#   TEAM_ID=ABCDE12345 scripts/release-macos.sh
#   SKIP_SIGN=1 scripts/release-macos.sh     unsigned build, to check the bundle layout only
set -eu

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
app="target/release/bundle/macos/Quota Deck.app"
helper_id="com.kutluhangil.quotadeck.fanhelper"
dmg="target/release/QuotaDeck-${version}-macos-arm64.dmg"
profile="${NOTARY_PROFILE:-quotadeck-notary}"

if [ "${SKIP_SIGN:-}" = "1" ]; then
  FANHELPER_DEV=1 sh scripts/build-fanhelper.sh >/dev/null
else
  : "${TEAM_ID:?set TEAM_ID to your Apple Developer Team ID}"
  identity=$(security find-identity -v -p codesigning | sed -n "s/.*\"\(Developer ID Application: .*(${TEAM_ID})\)\"/\1/p" | head -1)
  [ -n "$identity" ] || { echo "no 'Developer ID Application' certificate for team ${TEAM_ID} in the keychain" >&2; exit 1; }
  TEAM_ID="$TEAM_ID" sh scripts/build-fanhelper.sh >/dev/null
fi

# The app bundle only; the DMG is assembled below, after every piece is signed.
npm --prefix ui exec tauri -- build --bundles app --config app/tauri.fan.conf.json

test -x "$app/Contents/MacOS/quotadeck-fanhelper" || { echo "fan helper missing from the bundle" >&2; exit 1; }
test -f "$app/Contents/Library/LaunchDaemons/${helper_id}.plist" || { echo "launchd plist missing from the bundle" >&2; exit 1; }

if [ "${SKIP_SIGN:-}" = "1" ]; then
  echo "unsigned bundle ok: $app"
  exit 0
fi

# Inside out: nested code first, with the identifier the helper's peer check expects, then the
# app. Hardened runtime and a secure timestamp are what notarization requires.
codesign --force --options runtime --timestamp --identifier "$helper_id" --sign "$identity" \
  "$app/Contents/MacOS/quotadeck-fanhelper"
codesign --force --options runtime --timestamp --sign "$identity" "$app"
codesign --verify --strict --verbose=2 "$app"

stage=$(mktemp -d)
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
rm -f "$dmg"
hdiutil create -volname "Quota Deck" -srcfolder "$stage" -ov -format UDZO "$dmg" >/dev/null
rm -rf "$stage"
codesign --force --timestamp --sign "$identity" "$dmg"

xcrun notarytool submit "$dmg" --keychain-profile "$profile" --wait
xcrun stapler staple "$dmg"
spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"

# Release notes: this version's section of the changelog only.
notes="target/release/notes-${version}.md"
awk '/^## /{n++} n==1' CHANGELOG.md | sed '1d' > "$notes"

shasum -a 256 "$dmg"
echo
echo "ready: $dmg"
echo "publish with:"
echo "  gh release create v${version} \"$dmg\" --title \"Quota Deck ${version}\" --notes-file $notes"
