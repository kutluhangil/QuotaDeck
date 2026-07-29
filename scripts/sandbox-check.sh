#!/usr/bin/env bash
#
# Run the path resolution inside a real App Sandbox and assert what the sandbox changes.
#
# `sandbox-exec` is deprecated and its profile language is not the App Sandbox. An ad-hoc code
# signature carrying `app/Entitlements.plist` is — the kernel applies the same sandbox it applies
# to a store build, which makes this the only local test worth trusting.
#
# The entitlement has to be attached to a *bundle*. A bare Mach-O signed with
# `com.apple.security.app-sandbox` is killed with SIGTRAP at launch, because the container is
# named after `CFBundleIdentifier` and there is nowhere to put it. So this wraps the debug
# binary in the smallest bundle that satisfies that, using the shipping identifier so the
# container is the same one the real app gets.
#
# What is asserted, in order of how expensive each failure is to discover in App Review:
#
#   1. `$HOME` is rewritten to the container. If it is not, the process was not sandboxed and
#      every assertion below is vacuous — so this one aborts rather than failing.
#   2. `real_home` still reports the real home. That is the `getpwuid` lookup. Reading `$HOME`
#      here is the bug that makes every installed tool look absent inside the sandbox.
#   3. Our own data directory stays inside the container, the one place we can write with no
#      permission at all.
#   4. Every provider root reports `denied`, never `missing`. Reporting an installed tool as
#      absent sends the user to reinstall something they already have.
#
# Run from the repository root. No account, certificate or provisioning profile needed.

set -euo pipefail

IDENTIFIER="com.kutluhangil.quotadeck"
BIN="target/debug/quotadeck-debug"
STAGE="target/sandbox-check"
APP="${STAGE}/QuotaDeckCheck.app"
EXEC="${APP}/Contents/MacOS/QuotaDeckCheck"

echo "==> building"
cargo build -p quotadeck-app --bin quotadeck-debug

echo "==> unsandboxed baseline"
UNSANDBOXED="$("${BIN}" paths)"
echo "${UNSANDBOXED}"

REAL_HOME="$(printf '%s\n' "${UNSANDBOXED}" | awk '$1=="home" {print $2}')"
if [[ -z "${REAL_HOME}" ]]; then
  echo "error: the baseline run reported no home directory" >&2
  exit 1
fi

echo "==> wrapping it in a bundle and signing with the sandbox entitlement"
rm -rf "${STAGE}"
mkdir -p "${APP}/Contents/MacOS"
cp "${BIN}" "${EXEC}"
cat > "${APP}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>QuotaDeckCheck</string>
<key>CFBundleIdentifier</key><string>${IDENTIFIER}</string>
<key>CFBundleName</key><string>QuotaDeckCheck</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
</dict></plist>
PLIST
codesign --sign - --entitlements app/Entitlements.plist --force "${APP}"

echo "==> sandboxed"
SANDBOXED="$("${EXEC}" paths)"
echo "${SANDBOXED}"

field() { printf '%s\n' "${SANDBOXED}" | awk -v k="$1" '$1==k {print $2}'; }

fail=0
check() {
  if [[ "$2" == "$3" ]]; then
    echo "OK   $1"
  else
    echo "FAIL $1: expected $3, got $2" >&2
    fail=1
  fi
}

# 1. Proof the sandbox is on. Without it the rest proves nothing.
env_home="$(field env-home)"
if [[ "${env_home}" == *"/Library/Containers/${IDENTIFIER}/"* ]]; then
  echo "OK   the process is sandboxed (\$HOME is the container)"
else
  echo "FAIL the process is not sandboxed: \$HOME is ${env_home}" >&2
  echo "     nothing below would be meaningful; check that codesign embedded the entitlement" >&2
  exit 1
fi

# 2. The whole reason `paths::real_home` does not read `$HOME`.
check "the real home survives the sandbox" "$(field home)" "${REAL_HOME}"

# 3. Our writes belong in the container.
data="$(field data)"
if [[ "${data}" == "${env_home}"* ]]; then
  echo "OK   the data directory stays inside the container"
else
  echo "FAIL the data directory left the container: ${data}" >&2
  fail=1
fi

# 4. An installed tool must read as unreachable, never as absent. Skipped with a note rather
# than passed silently when this machine has no supported tool installed — a green run that
# checked nothing is worse than a stated gap.
roots="$(printf '%s\n' "${SANDBOXED}" | grep '^root ' || true)"
if [[ -z "${roots}" ]]; then
  echo "SKIP no provider root on this machine, so the denied-vs-missing case was not exercised"
else
  while read -r _ key state _; do
    check "${key} reports a permission problem, not a missing tool" "${state}" "denied"
  done <<< "${roots}"
fi

rm -rf "${STAGE}"
exit "${fail}"
