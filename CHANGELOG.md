# Changelog

## 2026-07-25

- Phase 0: verified Codex `rate_limits` is non-null on real logs — Codex confirmed as an L1 MEASURED source.
- Phase 0: discovered and live-verified the Claude Code statusline `rate_limits` payload (`five_hour` + `seven_day`) — Claude Code upgraded to L1 with zero credentials.
- Phase 0: tested and rejected the OTLP telemetry path — no rate-limit metric, requires a listening socket, leaks PII.
- Phase 0: measured a 51% duplicate rate in Claude Code usage rows, confirming `(message.id, requestId)` dedup is mandatory.
- Phase 0: measured log volume (175 MB/day Codex, 0.49% quota-relevant) and restated the read budget as relative rather than a fixed MB/hour figure.
- Added `docs/DISCOVERY.md` with the full Phase 0 findings and `CLAUDE.md` with the project working rules.
