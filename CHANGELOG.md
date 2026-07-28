# Changelog

## 2026-07-28

- Phase 3: added the Tauri v2 tray application — accessory activation policy, a transparent borderless panel positioned under the menu bar item, and click-away dismissal.
- Phase 3: the panel window is granted no filesystem capability at all; it receives folded snapshots over an event and nothing else.
- Phase 3: added the menu bar glyph, drawn as raw RGBA from the live reading. It stays a monochrome template image until usage passes 85%, and only then takes the critical colour.
- Phase 3: added the design system as CSS custom properties — the blueprint's dark and light palettes, Martian Mono for every number and Inter for text, both bundled with their SIL OFL licences.
- Phase 3: added the provider card with confidence badges, a level ramp that also carries pattern and text so it survives colour blindness, and empty states that give direction instead of apologising.
- Phase 3: added `ProviderEngine`, which keeps cursors between ticks. A timed full re-scan would have read hundreds of megabytes a minute; a quiet tick now reads zero bytes.
- Phase 3: the panel window sizes itself to its content rather than leaving empty surface below the cards.
- Phase 3: fixed an unreadable log folder being reported as "not installed" — found by running the app under `sandbox-exec`, which is the state every user starts in under the macOS App Sandbox.
- Phase 3: CI now builds and typechecks the panel, and enforces that no network entitlement and no frontend filesystem capability are ever added.

## 2026-07-25

- Phase 2: added the Codex provider — reads `~/.codex/sessions/**/rollout-*.jsonl`, classifies windows by reported duration, and reports measured limits per `limit_id`.
- Phase 2: counted Codex tokens from `last_token_usage` instead of differencing the session running total, after measuring that differencing over-reports by 17% when a resumed session carries its predecessor's total into a new file.
- Phase 2: deduped Codex re-emitted records on `(file + running total, turn total)`, which removed all 37 duplicates found in real logs without dropping the 5 distinct turns that shared a running total.
- Phase 2: split `cached_input_tokens` out of `input_tokens` and `reasoning_output_tokens` out of `output_tokens` so neither is counted twice.
- Phase 2: added `discovery` (segment-wildcard file finding, newest first) and `scan` (discovery plus incremental reading plus parsing) to core.
- Phase 2: `quotadeck debug <provider>` now prints windows, confidence, reset times and token totals from real logs.
- Phase 2: added six real and six synthetic Codex fixtures with a README separating measured shapes from defensive ones.

- Phase 1: added the Cargo workspace (`core`, `providers`, `app`) with no HTTP client and no listening socket.
- Phase 1: added the core data model — quota windows classified by reported duration, several independent limits per provider, and a confidence level that decays to stale after 30 minutes.
- Phase 1: added `EventIndex` with `(message.id, requestId)` dedup, cumulative-to-delta conversion per session, and retention-bounded pruning.
- Phase 1: added byte-offset incremental reading with a partial-line buffer, chunk limiting, and rotation detection by inode or file index.
- Phase 1: added bounded reverse-tail reading so an L1 refresh costs a few KB regardless of log size.
- Phase 1: added the non-recursive `notify` watcher with 750 ms per-path coalescing.
- Phase 1: added the redb store with batched writes (500 records / 60 s / shutdown).
- Phase 1: added the `Provider` trait, the provider registry, and the `quotadeck` debug CLI.
- Phase 1: added criterion benchmarks and CI enforcing format, clippy, tests, and the no-HTTP / no-credentials constraints.

- Phase 0: verified Codex `rate_limits` is non-null on real logs — Codex confirmed as an L1 MEASURED source.
- Phase 0: discovered and live-verified the Claude Code statusline `rate_limits` payload (`five_hour` + `seven_day`) — Claude Code upgraded to L1 with zero credentials.
- Phase 0: tested and rejected the OTLP telemetry path — no rate-limit metric, requires a listening socket, leaks PII.
- Phase 0: measured a 51% duplicate rate in Claude Code usage rows, confirming `(message.id, requestId)` dedup is mandatory.
- Phase 0: measured log volume (175 MB/day Codex, 0.49% quota-relevant) and restated the read budget as relative rather than a fixed MB/hour figure.
- Added `docs/DISCOVERY.md` with the full Phase 0 findings and `CLAUDE.md` with the project working rules.
