#!/bin/sh
# Installs the fan helper on this Mac for local testing, without a Developer ID build.
#
#   sudo scripts/fanhelper-dev.sh install     build (dev requirement), ad-hoc sign, start as root
#   scripts/fanhelper-dev.sh test             Cool for 8 s, then Auto; prints what the fan did
#   sudo scripts/fanhelper-dev.sh uninstall   fans to Auto, stop, remove every file it added
#
# The release build is registered by the app through SMAppService instead (docs/USER_ACTIONS.md).
# Run from the repository root.
set -eu

id="com.kutluhangil.quotadeck.fanhelper"
binary="/Library/PrivilegedHelperTools/${id}.dev"
plist="/Library/LaunchDaemons/${id}.plist"
client="target/fanhelper/quotadeck-fanhelper"

need_root() {
  if [ "$(id -u)" -ne 0 ]; then
    echo "run with sudo: sudo $0 $1" >&2
    exit 1
  fi
}

case "${1:-}" in
install)
  need_root install
  # Build as the invoking user so target/ stays theirs.
  sudo -u "${SUDO_USER:-$USER}" env FANHELPER_DEV=1 sh scripts/build-fanhelper.sh >/dev/null
  sudo -u "${SUDO_USER:-$USER}" codesign --force --sign - --identifier "$id" "$client"
  install -m 755 -o root -g wheel "$client" "$binary"
  codesign --force --sign - --identifier "$id" "$binary"
  cat > "$plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>${id}</string>
  <key>ProgramArguments</key><array><string>${binary}</string><string>daemon</string></array>
  <key>MachServices</key><dict><key>${id}</key><true/></dict>
</dict>
</plist>
PLIST
  chown root:wheel "$plist"
  chmod 644 "$plist"
  launchctl bootout system "$plist" 2>/dev/null || true
  launchctl bootstrap system "$plist"
  echo "installed: $binary (dev requirement: identifier ${id})"
  ;;
test)
  [ -x "$client" ] || { echo "build first: sudo $0 install" >&2; exit 1; }
  echo "before:  $("$client" client status)"
  "$client" client set 0 cool
  sleep 8
  echo "cool:    $("$client" client status)"
  "$client" client set 0 auto
  sleep 8
  echo "auto:    $("$client" client status)"
  ;;
uninstall)
  need_root uninstall
  if [ -x "$binary" ]; then "$binary" client restore >/dev/null 2>&1 || true; fi
  launchctl bootout system "$plist" 2>/dev/null || true
  rm -f "$plist" "$binary"
  echo "removed"
  ;;
*)
  echo "usage: sudo $0 install | $0 test | sudo $0 uninstall" >&2
  exit 64
  ;;
esac
