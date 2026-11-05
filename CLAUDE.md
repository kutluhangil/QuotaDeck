# Quota Deck — working rules

## Git

Blueprint §10 forbade commits. On 2026-07-25 Kutluhan allowed commit and push at the end of each phase; on 2026-10-22 he revoked that. **Never commit or push on your own, not even at the end of a phase.** Commit or push only when Kutluhan explicitly asks for it.

- When asked, commit and push as **separate commands**, never chained.
- Never add `Co-Authored-By: Claude` or any AI attribution trailer. GitHub Contributors must show only Kutluhan.
- Never change `user.name` / `user.email`.
- Stay on `main`. Do not create branches.
- Prefer non-interactive git (`git --no-pager diff`).

## Architecture red lines

- **No network requests.** `reqwest`, `hyper`, `ureq` or any other HTTP client must never be added as a dependency. This keeps the privacy claim true and the tools' ToS respected; no entitlement for outbound connections exists.
- **No listening sockets either.** Ruled out in Phase 0 (`docs/DISCOVERY.md` §4) — it would require `com.apple.security.network.server` and break the same claim.
- **Keychain / Credential Manager is never read.**
- Provider auth files (`auth.json`, `credentials.json`, `.credentials`) are never opened, listed, or even probed for existence.
- Only session and telemetry log files are read. Read-only. Never written to.
- `tauri-plugin-fs` is never exposed to the frontend. All file access lives in Rust behind a capability whitelist.
- `settings.json` (status line) is edited only with explicit consent, chains the previous command, and offers one-click revert.

### Distribution and v2 exceptions (Kutluhan, 2026-11-04 / 2026-11-05, `docs/V2_PLAN.md`)

Quota Deck ships only from GitHub, open source (GPL-3.0-only), as a Developer ID-signed, notarized DMG (`scripts/release-macos.sh`). The Mac App Store and Microsoft Store builds are parked in `stores/` and are not built or checked in CI.

These live behind the `system-monitor` Cargo feature (on by default). The parked App Store build compiled without it. The direct build also enables `macOSPrivateApi` for the transparent Liquid Glass popover.

- **Hardware sensors** are read through IOKit/AppleSMC, IOHIDEventSystemClient and IOReport (private APIs). Read-only.
- **Docker** is read by running the `docker` CLI with read-only subcommands only (`info`, `ps`, `stats --no-stream`, `system df`). Never `run`, `start`, `stop`, `rm` or anything that changes state.
- **Antigravity live quota** may run `agy -p /usage --output-format json` only when the user turns that setting on. The network request is made by `agy`, not by Quota Deck, and the setting says so. Antigravity account files (`google_accounts.json`, OAuth/token files) are never opened, listed or probed.
- **Fan control** goes through a privileged helper that the user installs on first use (macOS admin prompt). It writes only fan SMC keys (`F*md`, `F*Tg`, `Ftst`), always returns fans to Auto when the app quits, sleeps or disconnects, and can be removed from Settings. Nothing else is ever written to the SMC.

## Performance rules

- JSONL files are never read from the beginning. Always a byte-offset cursor.
- No `read_to_string` on a log file.
- No disk write per event. Batched: 500 records or 60 seconds.
- The watcher is not recursive. Only known subdirectories.
- L1 (measured limits) is obtained by a bounded reverse-tail read, not by full ingest — quota records are 0.49% of log volume (`docs/DISCOVERY.md` §6).
- State the memory and binary-size impact before adding any dependency.

## Code

- Rust: `cargo clippy -- -D warnings` must be clean.
- No `unwrap()` / `expect()` on production paths. A panic in the parser kills the app.
- An unparseable line returns `Ok(None)`, never `Err`. One corrupt line must not drop the whole file.
- TypeScript: `strict: true`, no `any`.
- A new provider is one file + a fixture test + a registration in `providers/mod.rs`.
- Comments and identifiers in English.

## Provider data rules

- Quota windows are classified by `window_minutes`, **never** by key name (`primary` / `secondary` are not stable across CLI versions — see `docs/DISCOVERY.md` §2.2).
- Claude Code usage rows are deduped on `(message.id, requestId)`. The observed duplicate rate is 51%; skipping this roughly doubles reported cost.
- A tool that is not installed reports `Unavailable`. Never synthesize or estimate data for it.
- Estimated values always carry the "estimated" confidence badge in the UI. Presenting a guess as measured is the top trust-killer in this category.

## Test

- Every provider gets real sample JSONL under `tests/fixtures/<provider>/`.
- All personal data in fixtures must be anonymized.
- If a perf benchmark is red, stop and fix it before continuing.
