# Changelog

## 2026-07-25

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
