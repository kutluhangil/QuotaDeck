# CodexBar Gap Roadmap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the release, reliability, configurability, history, CLI, and localization gaps found in the CodexBar comparison without weakening Quota Deck's local-only privacy model.

**Architecture:** Quota Deck remains a zero-network Tauri application. Provider policy and operational health live in the app layer, parsing and historical accounting remain in Rust core/provider crates, and the frontend receives typed snapshots only. Provider instances, not provider brands, become the unit of checkpointing when multi-account support lands.

**Tech Stack:** Rust 1.85+, Tauri v2, React 19, TypeScript strict mode, Zustand, Vitest, Astro, redb, GitHub Actions.

**Spec:** `QUOTA_DECK_BLUEPRINT.md`, `CLAUDE.md`, and the accepted CodexBar comparison report from 2026-08-25.

## Global Constraints

- No network requests and no listening sockets.
- Never read OAuth tokens, cookies, Keychain, Credential Manager, or provider auth files.
- Only local session and telemetry logs may be read; provider data is read-only.
- The frontend never receives filesystem capability.
- An unavailable or unknown value is explicit; no silent fallback and no fabricated parser schema.
- JSONL ingestion stays incremental and byte-offset based.
- Settings writes and checkpoints stay atomic and backward compatible.
- Production Rust paths use no `unwrap()` or `expect()`.
- Every behavior change follows red-green-refactor and updates `CHANGELOG.md`.
- Every phase ends with fresh tests, a separate commit, and a separate push on `main`, as required by `CLAUDE.md`.
- New provider work requires an anonymized real fixture before parser code.

## Scope Rulings

1. CodexBar features that require cookies, OAuth, API keys, provider-internal endpoints, status polling, HTTP servers, or secret sync are intentionally excluded.
2. `additionalRoots` means several log roots folded into one quota identity. It must not be labelled multi-account.
3. True multi-account support uses a stable provider-instance identity with separate checkpoints, plans, alerts, health, and history.
4. The Mac App Store GUI and the CLI are separate distribution artifacts. The GUI must not promise that it installs a shell binary.
5. Retention growth triggers a controlled rebuild from logs; it never pretends pruned history is still available.
6. Provider breadth is evidence-gated. Unsupported frontend IDs are not presented as compiled providers.

## Phase 0 — Plan, ownership, and baseline

### Task 0.1: Persist the execution contract

**Files:**
- Create: `docs/superpowers/plans/2026-08-25-codexbar-gap-roadmap.md`
- Create: `docs/USER_ACTIONS.md`

- [x] Record implementation phases, privacy boundaries, verification commands, commit boundaries, and evidence gates in this plan.
- [x] Put only account-, hardware-, human-perception-, or real-fixture-dependent work in `docs/USER_ACTIONS.md`.
- [x] Confirm `git status --short --branch` is clean before implementation.
- [x] Commit with `docs: record the CodexBar gap roadmap` and push separately.

### Task 0.2: Record a reproducible baseline

- [x] Run `cargo test --workspace`; expected baseline: 367 passed, 0 failed, 4 perf tests and 6 real-log tests ignored.
- [x] Run `npm test --prefix ui -- --run`; expected baseline: 71 passed.
- [x] Run `npm run build --prefix ui` and `npm run check --prefix site`; both must exit 0.
- [x] Record Node/npm compatibility warnings as environment notes, not false test failures.

## Phase 1 — Release and native correctness first

### Task 1.1: Remove the duplicate tray definition

**Files:**
- Modify: `app/tauri.conf.json`
- Modify: `scripts/check-appstore-config.mjs`
- Modify: `CHANGELOG.md`

**Interfaces:**
- The only owner of tray creation is `app/src/tray.rs::install`.
- The configuration regression check rejects any future declarative `app.trayIcon` block.

- [x] RED: extend `scripts/check-appstore-config.mjs` to require `base.app?.trayIcon === undefined`; run it and verify it fails because the block exists.
- [x] GREEN: remove `app.trayIcon` from `app/tauri.conf.json` and rerun the check.
- [x] Run `cargo test -p quotadeck-app tray`, `node scripts/check-appstore-config.mjs`, and `npm run build --prefix ui`.

### Task 1.2: Add the missing Astro semantic gate

**Files:**
- Modify: `site/package.json`
- Modify: `site/package-lock.json`
- Modify: `CHANGELOG.md`

- [x] Install `@astrojs/check` as a local dev dependency; do not install globally.
- [x] Change `site`'s `check` script to `ASTRO_TELEMETRY_DISABLED=1 astro check && npm run build`.
- [x] Run `npm run check --prefix site` and confirm the Astro diagnostic summary reports 0 errors.

### Task 1.3: Update release truth and automated guards

**Files:**
- Modify: `docs/RELEASE_CHECKLIST.md`
- Modify: `docs/STORE.md`
- Modify: `CHANGELOG.md`

- [x] Replace stale 285/47 counts with fresh verified counts.
- [x] Remove the stale claim that runaway-agent detection is missing.
- [x] Remove or qualify any statement implying the App Store bundle ships the CLI.
- [x] Keep signing, TestFlight, VoiceOver, 72-hour soak, Windows runtime, Linux runtime, price, and domain unresolved until evidence exists.

### Task 1.4: Exercise the real macOS application

**Files:**
- Modify only if runtime evidence reveals a reproducible defect.

- [x] Build the release app with `npm --prefix ui exec tauri -- build --bundles app`.
- [ ] Launch the generated `.app`; verify exactly one tray item appears.
- [ ] Exercise tray toggle, panel focus/dismissal, settings, dashboard, manual refresh when available, and statusline preview/manual sandbox flow.
- [x] Inspect process output for actionable errors and add a failing regression test before any runtime fix.
- [x] Run `codesign --verify --deep --strict --verbose=2` and entitlement/private-framework checks; report signing-material blockers honestly.

### Task 1.5: Verify, commit, and push

- [x] Run format, clippy, workspace tests, UI tests/build, site check, App Store config check, icon check, sandbox check, and release perf budgets.
- [x] Commit with `fix(release): verify the native tray and site gates`.
- [x] Push in a separate command.

## Phase 2 — Provider controls

### Task 2.1: Add provider policy to persisted settings

**Files:**
- Modify: `app/src/deck.rs`
- Modify: `ui/src/types.ts`
- Modify: `CHANGELOG.md`

**Interfaces:**
- `Settings.disabled_providers: BTreeSet<String>` serializes as `disabledProviders`.
- `Settings.provider_order: Vec<String>` serializes as `providerOrder`.
- `Settings::is_provider_enabled(ProviderId) -> bool`.
- `Settings::ordered_provider_ids(&[ProviderId]) -> Result<Vec<ProviderId>>` rejects duplicates and unknown keys with actionable paths/keys.

- [x] RED: add tests for legacy settings migration, default all-enabled registry order, partial order appending new compiled providers, duplicate keys, unknown keys, and failed-save rollback.
- [x] GREEN: implement the settings fields, validation helpers, and atomic setters.
- [x] Mirror the exact serialized shape in TypeScript.

### Task 2.2: Expose only the compiled provider catalogue

**Files:**
- Modify: `app/src/lib.rs`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/store.ts`

**Interfaces:**
- `ProviderDescriptor { id, displayName, supportsMeasured, enabled }`.
- Tauri command `provider_catalogue() -> Result<Vec<ProviderDescriptor>, String>` returns compiled providers in configured order.

- [x] RED: test that the catalogue contains exactly the registered providers and never the frontend-only planned IDs.
- [x] GREEN: add the command and typed store state.

### Task 2.3: Stop disabled providers at the backend boundary

**Files:**
- Modify: `app/src/lib.rs`
- Modify: `app/src/bin/debug.rs` or its Phase 6 successor

- [x] RED: add deterministic tests showing disabled providers are not refreshed, watched, emitted, alerted, exported, or selected for tray headline.
- [x] GREEN: filter `publish` and `sync_watches` using the current settings; order state/history by `providerOrder` on every pass.
- [x] Explicit `--provider` against a disabled provider returns an actionable error unless `--include-disabled` is supplied.
- [x] Re-enabling wakes the read loop and resumes from the existing checkpoint without double counting.

### Task 2.4: Build keyboard-accessible provider settings

**Files:**
- Modify: `ui/src/components/SettingsView.tsx`
- Modify: `ui/src/store.ts`
- Modify: `ui/src/i18n/en.ts`
- Modify: `ui/src/i18n/tr.ts`
- Modify: `ui/src/styles/panel.css`

- [x] Add enable switches and Up/Down buttons; do not make drag-and-drop the only ordering mechanism.
- [x] Disable movement at list boundaries and preserve visible focus.
- [x] Surface backend validation/save failures through the existing settings error area.
- [x] Run catalogue parity tests, TypeScript, Vitest, and production build.

### Task 2.5: Verify, commit, and push

- [x] Run full Rust/UI/site/perf verification.
- [ ] Commit with `feat(settings): control provider visibility and order`.
- [ ] Push separately.

## Phase 3 — Manual refresh, provider health, and tray overview

### Task 3.1: Add operational health without contaminating core snapshots

**Files:**
- Modify: `app/src/deck.rs`
- Modify: `app/src/lib.rs`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/store.ts`

**Interfaces:**
- `ProviderHealth { provider, state, lastAttemptAt, lastSuccessAt, consecutiveFailures, lastError, nextRetryAt }`.
- `HealthState` is `healthy | stale | error | disabled | unavailable`.
- `DeckState` carries `health` and `refreshing`; old successful snapshots remain visible on a later read failure and are explicitly marked stale/error.

- [x] RED: test success→failure preservation, failure→success reset, disabled/unavailable separation, and global `updatedAt` versus per-provider success.
- [x] GREEN: maintain health in the read-loop owner and serialize it with the deck.

### Task 3.2: Add a single-flight refresh command

**Files:**
- Modify: `app/src/lib.rs`
- Modify: `app/src/deck.rs`
- Modify: `ui/src/store.ts`
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/Dashboard.tsx`

**Interfaces:**
- Read-loop message `RefreshNow { request_id }`.
- Tauri command `refresh_now() -> Result<RefreshReceipt, String>` acknowledges queueing.
- Concurrent requests coalesce into one pass; the next state event completes the UI action.

- [x] RED: test queue wake-up, coalescing, disabled providers, error propagation, and no concurrent engine access.
- [x] GREEN: add Refresh controls to panel and dashboard with `aria-busy` and visible failure text.

### Task 3.3: Add a local-only tray overview

**Files:**
- Modify: `app/src/tray.rs`
- Modify: `app/src/i18n.rs`

- [x] RED: test a pure menu model for provider order, measured percentage, stale/error wording, disabled omission, and locale.
- [x] GREEN: keep Open first for Linux, then disabled provider summary rows, Dashboard, Refresh, separator, and Quit.
- [x] Rebuild/replace the menu only when its model changes; surface replacement/rollback errors.

### Task 3.4: Add pace-exhaustion notifications

**Files:**
- Modify: `app/src/alerts.rs`
- Modify: `app/src/i18n.rs`

- [x] RED: test first-pass silence, healthy→at-risk transition, jitter suppression, re-arm after healthy, mute behavior, reset behavior, and threshold/pace same-tick coalescing.
- [x] GREEN: announce projections as projections with local exhaustion time; never represent them as measured limits.

### Task 3.5: Verify, commit, and push

- [ ] Run full verification and native tray smoke.
- [ ] Commit with `feat(refresh): expose provider health and tray actions`.
- [ ] Push separately.

## Phase 4 — Time-correct pricing and controlled rebuilds

### Task 4.1: Version the embedded price schema

**Files:**
- Modify: `core/prices/anthropic.json`
- Modify: `core/src/pricing.rs`
- Modify: `providers/src/claude_code.rs`

**Interfaces:**
- `PricePeriod { effectiveFrom, effectiveUntil, source, sourceCheckedAt, rates }`.
- `price_for_at(model, at)` uses inclusive start and exclusive end.
- `cost_of_at(model, at, usage)` returns `None` for unknown models or uncovered dates.

- [ ] RED: test interval boundaries, overlap rejection, invalid rates/dates, unknown dates, model normalization, and fixture timestamps on both sides of a price change.
- [ ] GREEN: pass each Claude event timestamp to pricing and retain unpriced accounting.

### Task 4.2: Add pricing revision migration

**Files:**
- Modify: `core/src/engine.rs`
- Modify: `core/src/store.rs`
- Modify: `app/src/lib.rs`

**Interfaces:**
- Provider checkpoints carry `pricing_revision`.
- A mismatch deletes only the affected provider-instance checkpoint and rebuilds from readable logs.

- [ ] RED: test old revision rebuild, matching revision resume, no double count, and failure leaving other providers intact.
- [ ] GREEN: implement scoped checkpoint deletion and explicit rebuild health state.

### Task 4.3: Verify, commit, and push

- [ ] Run real Claude fixtures and release perf budgets; record table memory/binary-size delta.
- [ ] Commit with `fix(pricing): apply rates by event date`.
- [ ] Push separately.

## Phase 5 — Retention, date range, and safe export

### Task 5.1: Make retention an explicit policy

**Files:**
- Modify: `app/src/deck.rs`
- Modify: `core/src/engine.rs`
- Modify: `core/src/events.rs`
- Modify: `app/src/lib.rs`
- Modify: `app/src/bin/debug.rs` or its Phase 6 successor

**Interfaces:**
- Allowed `retentionDays`: 32, 90, 365; default 32 for legacy files.
- Lowering prunes immediately and marks the checkpoint dirty.
- Raising schedules a scoped rebuild from available logs and reports progress/limitations.

- [ ] RED: test migration, invalid values, prune, rebuild, restore compatibility, and unavailable old logs.
- [ ] GREEN: use the configured horizon consistently in app history and CLI export.
- [ ] Benchmark 365-day checkpoint size, peak RSS, cold rebuild, and hourly read/write budgets before accepting it.

### Task 5.2: Add retained-range filtering and copy export

**Files:**
- Modify: `ui/src/history.ts`
- Modify: `ui/src/history.test.ts`
- Modify: `ui/src/Dashboard.tsx`
- Modify: `app/src/lib.rs`
- Modify: `app/src/export.rs`

- [ ] RED: test 90/365 presets, custom half-open date range, UTC/local DST edges, and empty ranges.
- [ ] GREEN: add range controls and backend-generated JSON/CSV copied to clipboard; do not grant filesystem capability.
- [ ] Keep unpriced tokens, dropped labels, provider health, and schema version in exports.

### Task 5.3: Verify, commit, and push

- [ ] Run full verification plus 365-day perf and persistence checks.
- [ ] Commit with `feat(history): add retention controls and safe exports`.
- [ ] Push separately.

## Phase 6 — Productize the CLI

### Task 6.1: Rename and test the command surface

**Files:**
- Move: `app/src/bin/debug.rs` → `app/src/bin/quotadeckctl.rs`
- Modify: `app/Cargo.toml`
- Create: `app/src/cli.rs`
- Modify: `app/src/lib.rs`

**Interfaces:**
- Binary name: `quotadeckctl`.
- Commands: `--help`, `--version`, `providers`, `status`, `export`, `config show`, `config validate`, `guard`, `tray`, and `statusline preview`.
- Mutating statusline commands remain explicit and unavailable in the sandboxed store artifact.

- [ ] RED: integration tests cover help/version, unknown flags, conflicting formats, unknown/disabled providers, malformed config, stable JSON schema, and documented exit codes.
- [ ] GREEN: move argument parsing into pure/testable code and keep stdout data separate from stderr diagnostics.

### Task 6.2: Make distribution claims true

**Files:**
- Modify: `docs/STORE.md`
- Modify: `docs/RELEASE_CHECKLIST.md`
- Modify: release workflow files if a standalone CLI artifact is shipped.

- [ ] Build a standalone signed/notarized CLI artifact or remove every claim that the App Store GUI installs it.
- [ ] Document a stable JSON `schemaVersion` and keep the CSV header regression test.

### Task 6.3: Verify, commit, and push

- [ ] Run CLI integration tests from the built release binary and pipe tests including broken pipe behavior.
- [ ] Commit with `feat(cli): ship a stable quotadeckctl interface`.
- [ ] Push separately.

## Phase 7 — Multi-root and true provider instances

### Task 7.1: Add deduplicated additional roots

**Files:**
- Modify: `core/src/engine.rs`
- Modify: `app/src/deck.rs`
- Modify: `app/src/lib.rs`
- Modify: `ui/src/components/SettingsView.tsx`

- [ ] RED: test disjoint roots, parent/child overlap, symlink/canonical duplicate, root removal, watch cleanup, and unreadable path errors.
- [ ] GREEN: resolve configured roots in the backend, deduplicate files by canonical path/file identity, and label this feature “additional log folders.”

### Task 7.2: Introduce provider-instance identity

**Files:**
- Modify: `core/src/types.rs`
- Modify: `core/src/engine.rs`
- Modify: `core/src/store.rs`
- Modify: `app/src/deck.rs`
- Modify: `app/src/lib.rs`
- Modify: `app/src/alerts.rs`
- Modify: UI types, settings, cards, history, and exports.

**Interfaces:**
- `ProviderInstanceId { provider: ProviderId, key: String }`.
- Every instance owns roots, checkpoint, plan, alert thresholds, health, snapshot label, and history.

- [ ] RED: test two instances with colliding session/request IDs, independent checkpoints, plans, alerts, health, order, export rows, and delete/disable behavior.
- [ ] GREEN: migrate the existing provider-only settings/checkpoints to a `default` instance without data loss.
- [ ] Keep extra-home/disc roots unavailable in App Store builds until multiple security-scoped bookmarks are implemented and tested.

### Task 7.3: Verify, commit, and push

- [ ] Run sandbox bookmark, checkpoint migration, real-log, and perf suites.
- [ ] Commit with `feat(providers): support isolated local account instances`.
- [ ] Push separately.

## Phase 8 — German and Spanish localization

### Task 8.1: Generalize locale registries

**Files:**
- Modify: `ui/src/i18n/index.ts`
- Modify: `ui/src/i18n/i18n.test.ts`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/components/SettingsView.tsx`
- Modify: `app/src/i18n.rs`

- [ ] RED: make catalogue parity/placeholder tests iterate every registered language.
- [ ] GREEN: add `de` and `es` to frontend and native locale resolution with self-named language labels.

### Task 8.2: Add complete app catalogues

**Files:**
- Create: `ui/src/i18n/de.ts`
- Create: `ui/src/i18n/es.ts`
- Modify: `app/src/i18n.rs`

- [ ] Translate every existing string, notification, tray action, folder grant, health state, and new feature string.
- [ ] Verify placeholder arity, percent-sign placement, number grouping, reset clocks, and screen-reader language tags.

### Task 8.3: Decide site localization separately

- [ ] Do not add partial `/de` or `/es` routes. If site localization is approved, generalize route/hreflang/sitemap registries first, then add all pages and copy.

### Task 8.4: Verify, commit, and push

- [ ] Run Rust i18n tests, all catalogue tests, UI build, Astro semantic check, and native locale smoke.
- [ ] Commit with `feat(i18n): add German and Spanish app catalogues`.
- [ ] Push separately.

## Phase 9 — Evidence-gated provider expansion and final release gate

### Task 9.1: Acquire real local fixtures

- [ ] Follow `docs/USER_ACTIONS.md` for OpenCode, Windsurf, JetBrains, Cursor, Kimi, Gemini CLI, Qwen Code, or another requested tool.
- [ ] Anonymize the smallest complete local record set before committing it.
- [ ] Record root, glob/database schema, event semantics, cumulative/delta behavior, and measured/derived confidence in the fixture README.

### Task 9.2: Implement one provider per evidence package

**Files per provider:**
- Create: `providers/src/<provider>.rs`
- Create: `providers/tests/fixtures/<provider>/*`
- Modify: `providers/src/lib.rs`
- Modify: `core/src/types.rs` only if a new stable ID is required.

- [ ] RED: write parser, dedup, accounting, malformed-record, discovery, and double-scan fixture tests.
- [ ] GREEN: implement only fields proven by the fixture; unknown values remain unavailable/unpriced.
- [ ] Run real fixture tests and performance budgets before registration.
- [ ] Commit and push each provider separately.

### Task 9.3: Final cross-platform release gate

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo test --workspace` and all real-log tests available on this machine.
- [ ] Run release perf tests single-threaded with `--nocapture`.
- [ ] Run UI tests/build, Astro semantic check/build, App Store config, icon, sandbox, and no-network/no-credential guards.
- [ ] Re-run real macOS tray/panel/settings/dashboard/statusline/notification flow.
- [ ] Review the complete diff and unresolved `docs/USER_ACTIONS.md` items.
- [ ] Commit final documentation truth and push separately.

## Completion Definition

The implementation is complete only when every code-capable task above is checked, every phase has fresh evidence and a pushed commit, and remaining unchecked items require an external account, a different operating system, human-perception QA, a 72-hour duration, or a real provider fixture. Builds alone do not close native runtime or store-distribution tasks.
