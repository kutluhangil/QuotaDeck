# Changelog

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
