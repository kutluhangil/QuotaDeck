# Quota Deck — working rules

## Git

Blueprint §10 forbade commits. Kutluhan overrode this on 2026-07-25: **commit and push at the end of each phase.**

- Commit and push as **separate commands**, never chained.
- Never add `Co-Authored-By: Claude` or any AI attribution trailer. GitHub Contributors must show only Kutluhan.
- Never change `user.name` / `user.email`.
- Stay on `main`. Do not create branches.
- Prefer non-interactive git (`git --no-pager diff`).

## Architecture red lines

- **No network requests.** `reqwest`, `hyper`, `ureq` or any other HTTP client must never be added as a dependency. This is required for ToS and App Store compliance, and `com.apple.security.network.client` is deliberately absent from the entitlements.
- **No listening sockets either.** Ruled out in Phase 0 (`docs/DISCOVERY.md` §4) — it would require `com.apple.security.network.server` and break the same claim.
- **Keychain / Credential Manager is never read.**
- Provider auth files (`auth.json`, `credentials.json`, `.credentials`) are never opened, listed, or even probed for existence.
- Only session and telemetry log files are read. Read-only. Never written to.
- `tauri-plugin-fs` is never exposed to the frontend. All file access lives in Rust behind a capability whitelist.
- The one exception to "never write outside our own data dir" is the opt-in Claude Code statusline shim, which edits `settings.json` only with explicit user consent, chains the user's previous command instead of replacing it, and is revertible in one click.

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
