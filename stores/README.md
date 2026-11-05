# Store builds (parked)

Quota Deck ships from GitHub as a signed, notarized download. The store channels are kept here,
out of the way, in case they are ever wanted again. Nothing in this folder runs in CI.

| Folder | What | Notes |
|---|---|---|
| `apple/` | Mac App Store build: `appstore.sh`, sandbox entitlements, the store config override, the WidgetKit extension (`widget/`, `widget.sh`), the sandbox regression harness (`sandbox-check.sh`), the config gate (`check-appstore-config.mjs`) and `STORE.md` (listing, privacy answers, CLI exit codes). | Builds with `--no-default-features`, so no system-monitor code (private APIs) ships to the store. The store build is AI quota only. |
| `microsoft/` | Microsoft Store installers: `msstore.ps1` and its config override. | Windows only. |

## If you revive a store build

- The direct build now turns on `macOSPrivateApi` (real window vibrancy). The App Store override
  sets it back to `false`; `stores/apple/check-appstore-config.mjs` checks that override, not the
  base config.
- Tauri's `macos-private-api` Cargo feature now follows `system-monitor`, so a
  `--no-default-features` build must also get the store override config, or `tauri-build` stops
  on the mismatch:
  `TAURI_CONFIG="$(cat stores/apple/tauri.appstore.conf.json)" cargo clippy -p quotadeck-app --no-default-features`
  (`appstore.sh` passes the override through the Tauri CLI already).
- Run the gate and its tests from the repository root:
  `node stores/apple/check-appstore-config.mjs && node --test stores/apple/check-appstore-config.test.mjs`
- Then `stores/apple/appstore.sh` (needs the certificates listed in `docs/USER_ACTIONS.md` history).
