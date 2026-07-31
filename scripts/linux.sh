#!/usr/bin/env bash
#
# Build the Linux packages.
#
# Run from the repository root on a Linux machine:
#
#   scripts/linux.sh
#
# Three formats, because Linux has no single one. `.deb` and `.rpm` are what the two package
# manager families install and update through; the AppImage is the one file that runs on a
# distribution neither of them covers, without root and without a repository.
#
# There is no store submission here. Flathub and Snap both want an account and a review queue
# that nobody is waiting on, and the app asks for nothing at install time that would need one —
# no sandbox grant, no network capability to declare.
#
# Build dependencies on Debian/Ubuntu:
#
#   sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
#     libayatana-appindicator3-dev librsvg2-dev patchelf build-essential curl file
#
# On Fedora:
#
#   sudo dnf install -y webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
#     librsvg2-devel patchelf rpm-build

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "error: this builds Linux packages and has to run on Linux" >&2
  exit 1
fi

echo "==> building the panel"
npm --prefix ui ci
npm --prefix ui run build

echo "==> bundling"
# The base config targets macOS; this overrides only the bundle section.
cargo tauri build --config app/tauri.linux.conf.json

BUNDLE_DIR="target/release/bundle"

echo "==> checking the packages carry no network capability"
# The claim the listing makes is enforced by the dependency tree rather than by a manifest on
# Linux, so this is what stands in for the entitlement check the macOS script runs.
if cargo tree --workspace | grep -iE 'reqwest|hyper|ureq|isahc|surf|curl'; then
  echo "error: an HTTP client is in the dependency tree; see CLAUDE.md" >&2
  exit 1
fi

echo "==> produced"
find "${BUNDLE_DIR}" -maxdepth 2 -type f \
  \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' \) -print
