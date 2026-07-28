# Changelog

## 2026-07-28

- Phase 5: added the Claude Code provider — reads `~/.claude/projects/**`, including subagent and workflow-agent transcripts, whose calls bill to the same subscription.
- Phase 5: deduped usage on `(message.id, requestId)`. One API response is written as several rows, one per streamed content block, each repeating the whole `usage` object; 45.8% of 3412 real rows were repeats. Deduping on `uuid` instead would have caught none of them — it is regenerated per row.
- Phase 5: quota estimates are denominated in equivalent API cost rather than tokens. At published rates an Opus output token and a Haiku cache read differ by 50x, so a rolling token count cannot be compared against one ceiling.
- Phase 5: embedded a LiteLLM-derived Anthropic price table at build time. Lookup takes the longest matching model prefix, never the family — `claude-opus-4-1` bills at three times `claude-opus-4-5` and the shorter id prefixes the longer one.
- Phase 5: a model with no known price is counted as unpriced, never as free. The remainder is carried to the UI and blocks calibration, because a numerator short by an unknown amount must not anchor a ceiling.
- Phase 5: added the plan picker (Pro / Max 5x / Max 20x). "Not set" is the default and produces no estimate at all; a tier nobody picked would put an unrequested percentage under an estimated badge.
- Phase 5: one measured window now calibrates the tiers the tool did not report. The seeded ceilings are assumptions — only the published 1 : 5 : 20 tier scaling is taken as given — and a real reading replaces them.
- Phase 5: added the opt-in statusline shim, which reads Claude Code's real `five_hour` and `seven_day` percentages. It chains the user's existing command instead of replacing it, records `rate_limits` and the version only, preserves every other key in `settings.json`, and reverts in one click.
- Phase 5: settings are now persisted to the app data directory, so a chosen plan survives a restart.
- Phase 5: fixed the Horizon strip drawing a seven-day axis over one day of buckets. The series is trimmed to the longest window, and an estimated window added after the snapshot was built never reached that calculation — found on real logs, where a 91% weekly reading sat beside a near-empty strip.
- Phase 5: `quotadeck-debug` gained `statusline` (the exact before and after of the settings change) and an optional plan argument on `debug`, so the estimate can be checked against real logs from a terminal.
- Phase 5: CI now enforces that only the shim module touches `settings.json`, and that the shim records no payload field beyond the rate limits.

- Phase 4: added the Horizon strip — the timeline behind each provider's headline window, drawn as SVG from the five-minute bucket series.
- Phase 4: the strip's time axis comes from the reported `window_minutes`, so it is a week for a Codex weekly limit and five hours for a session one. No window length is assumed anywhere.
- Phase 4: dropped the blueprint's "returning capacity" ghost layer. It only holds for a sliding window, and the two providers measured in Phase 0 disagree — Claude Code resets on clean half-hour boundaries while Codex resets at an arbitrary instant, and `resets_at - window_minutes` lands 3.6 hours before a reading that already showed 68% of a week. The strip now states only the span it can prove.
- Phase 4: column height is linear against the busiest column with a visible floor, so a burst is never understated and a quiet column never reads as a gap.
- Phase 4: recency is carried by a gradient mask over the whole strip rather than per-column opacity — one declaration, a smooth ramp, and a floor each theme sets for itself.
- Phase 4: hovering the strip replaces the axis labels in place with that slice's clock time, its width and its token total. The readout is fixed-height so the card does not move under the cursor.
- Phase 4: added the menu bar strip mode, a 44×16 miniature of the same fold. It stays a template image below 85% like every other tray mode.
- Phase 4: `quotadeck debug <provider>` now prints the strip as text, so the fold can be checked against real logs rather than fixtures alone.
- Phase 4: snapshots now carry only the history the strip can draw instead of the full retention window — the panel was being sent up to 32 days of buckets every five seconds.
- Phase 4: added Vitest and wired it into CI. The panel's fold is a mirror of `core/src/horizon.rs` and is held to the same cases.

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
