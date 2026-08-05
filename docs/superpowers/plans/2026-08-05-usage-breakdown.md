# Usage Breakdown Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Executed inline, phase by phase, with a commit and push between phases.

**Goal:** Answer "what spent my quota" — by model, by project, and by autonomous agent — and make the answer scriptable.

**Architecture:** One reusable hourly, label-keyed rollup (`core/src/breakdown.rs`) sits beside the existing `BucketSeries` inside `EventIndex`. Three instances of it carry the three dimensions. Every label is derived from data the parser already holds — `UsageEvent.model` for models, `LineSource::path` for projects and agent origin — so no new file is opened and no extra byte is read. The dashboard pulls the breakdown on the same on-demand channel that already carries `hours`.

**Tech Stack:** Rust (workspace crates `core`, `providers`, `app`), Tauri v2, React + TypeScript (`ui`), Vitest, `cargo test`.

## Global Constraints

Copied verbatim from `CLAUDE.md`; every task below inherits them.

- **No network requests.** `reqwest`, `hyper`, `ureq` or any other HTTP client must never be added as a dependency.
- **No listening sockets.**
- **Keychain / Credential Manager is never read.** Provider auth files are never opened, listed, or probed.
- Only session and telemetry log files are read. Read-only. Never written to.
- `tauri-plugin-fs` is never exposed to the frontend.
- JSONL files are never read from the beginning — always a byte-offset cursor. No `read_to_string` on a log file.
- No disk write per event. Batched: 500 records or 60 seconds.
- **State the memory and binary-size impact before adding any dependency.** This plan adds no dependency.
- `cargo clippy --workspace --all-targets -- -D warnings` must be clean.
- No `unwrap()` / `expect()` on production paths.
- An unparseable line returns `Ok(None)`, never `Err`.
- TypeScript: `strict: true`, no `any`.
- Quota windows are classified by `window_minutes`, never by key name.
- A tool that is not installed reports `Unavailable`. Never synthesize or estimate data for it.
- Estimated values always carry the "estimated" confidence badge.
- Comments and identifiers in English.
- **If a perf benchmark is red, stop and fix it before continuing.**
- Commit and push at the end of each phase, as **separate commands**. No `Co-Authored-By` trailer. Stay on `main`.

## Design Rules Inherited From the Product

These are not style preferences; each was argued for in `CHANGELOG.md` and breaking one is a regression.

1. **Volume is not fullness.** A breakdown bar shows share of spend, not quota level. It uses the neutral ink ramp, never the level ramp — the same argument that put the heatmap on a neutral ramp in Phase 7.
2. **Never fabricate a label.** Codex names no model in any record. Those records get `None`, rendered as an explicit "not reported" row. An empty breakdown is never shown as if the data were absent.
3. **Bounded memory.** Distinct labels per dimension are capped. Overflow is counted and surfaced, never silently merged into an "other" bucket — that would under-report without saying so, which is what `CostRange::unpriced_tokens` already exists to prevent.
4. **Backward-compatible checkpoints.** New checkpoint fields carry `#[serde(default)]`, the pattern `read_errors` already established in `core/src/engine.rs:85`.

---

## File Structure

| File | Responsibility | Phase |
|---|---|---|
| `core/src/breakdown.rs` | **New.** Hourly label-keyed rollup: `BreakdownSeries`, `BreakdownPoint`, label cap and overflow counter. | 1 |
| `core/src/lib.rs` | Register the `breakdown` module. | 1 |
| `core/src/events.rs` | Hold the three series in `EventIndex`; fold into them in `ingest_usage`; trim in `prune`; carry them through the checkpoint. | 1, 2, 3 |
| `core/src/provider.rs` | `LineSource` gains the accessors that derive a project label and an agent origin from its own path. | 2, 3 |
| `providers/src/claude_code.rs` | Set project and origin on every `UsageEvent`. | 2, 3 |
| `providers/src/codex.rs` | Set project from the session's `cwd` where the record carries one. | 2 |
| `providers/src/copilot_cli.rs` | Set project where the session records one. | 2 |
| `app/src/deck.rs` | `ProviderHistory` gains the breakdown vectors. | 1, 2, 3 |
| `app/src/lib.rs` | Fill them at the same call site that already fills `hours`. | 1, 2, 3 |
| `app/src/bin/debug.rs` | `export` subcommand: JSON/CSV to stdout, exit codes. | 4 |
| `ui/src/types.ts` | Mirror the new payload types. | 1, 2, 3 |
| `ui/src/breakdown.ts` | **New.** Fold hourly breakdown points into a range total, sorted by spend. | 1 |
| `ui/src/breakdown.test.ts` | **New.** Vitest for that fold. | 1 |
| `ui/src/components/BreakdownList.tsx` | **New.** The neutral-ramp share list. | 1 |
| `ui/src/Dashboard.tsx` | Mount the breakdown lists. | 1, 2, 3 |
| `ui/src/i18n/en.ts`, `ui/src/i18n/tr.ts` | Strings for every new label. | 1, 2, 3 |
| `ui/src/styles/panel.css` | Neutral-ramp share bar. | 1 |
| `CHANGELOG.md` | One entry per phase. | all |

---

## Phase 1 — Model breakdown

**Why first:** `UsageEvent.model` is already populated by two of three providers and the price table is already embedded, yet `HistoryPoint` drops the model on the floor. The user cannot see that an Opus output token costs 50× a Haiku cache read even though the app already knows it. Cheapest real gain in the plan.

### Task 1.1: `BreakdownSeries` in core

**Files:**
- Create: `core/src/breakdown.rs`
- Modify: `core/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `core/src/breakdown.rs`

**Interfaces:**
- Consumes: `crate::types::{Cost, CostRange, TokenRollup}`, `crate::history::SECONDS_PER_HOUR`.
- Produces:
  - `pub const MAX_BREAKDOWN_LABELS: usize = 64;`
  - `pub struct BreakdownPoint { pub start: i64, pub label: Option<String>, pub tokens: TokenRollup, pub cost: CostRange }`
  - `pub struct BreakdownSeries` with:
    - `pub fn new() -> Self`
    - `pub fn add(&mut self, at: DateTime<Utc>, label: Option<&str>, tokens: &TokenRollup, cost: Cost)`
    - `pub fn trim_before(&mut self, cutoff: DateTime<Utc>)`
    - `pub fn points(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<BreakdownPoint>`
    - `pub fn labels_dropped(&self) -> u64`
    - `pub fn len(&self) -> usize`, `pub fn is_empty(&self) -> bool`
  - `Serialize + Deserialize + Clone + Debug + Default` on both.

- [ ] **Step 1: Write the failing tests** in `core/src/breakdown.rs`, covering:
  `records_in_one_hour_fold_into_one_point`, `a_missing_label_is_kept_as_none_rather_than_invented`,
  `two_labels_in_the_same_hour_stay_apart`, `unpriced_tokens_stay_apart_from_the_dollar_total`,
  `the_range_is_half_open_so_two_calls_never_double_count`, `trimming_drops_hours_before_the_cutoff`,
  `a_new_label_past_the_cap_is_counted_as_dropped_rather_than_merged`,
  `an_already_known_label_still_counts_after_the_cap_is_reached`.
- [ ] **Step 2:** `cargo test -p quotadeck-core breakdown` — expect FAIL (module not found).
- [ ] **Step 3:** Implement `BreakdownSeries` over `BTreeMap<(i64, Option<String>), Entry>`. Hour key is `at.timestamp().div_euclid(SECONDS_PER_HOUR) * SECONDS_PER_HOUR`. Tuple ordering puts the hour first, so `trim_before` is a `split_off` on `(cutoff_hour, None)`. Track distinct labels in a `BTreeSet<Option<String>>`; past `MAX_BREAKDOWN_LABELS` a *new* label increments `labels_dropped` and the record is not folded, while a known label is always folded.
- [ ] **Step 4:** `cargo test -p quotadeck-core breakdown` — expect PASS.
- [ ] **Step 5:** `cargo clippy --workspace --all-targets -- -D warnings` — expect clean.

### Task 1.2: Wire the model dimension into `EventIndex`

**Files:**
- Modify: `core/src/events.rs` — `EventIndex`, `ingest_usage`, `prune`, `EventIndexCheckpoint`, `checkpoint`, `restore`.

**Interfaces:**
- Consumes: `BreakdownSeries` from Task 1.1.
- Produces: `pub fn models(&self) -> &BreakdownSeries` on `EventIndex`.

- [ ] **Step 1: Write the failing tests** in `core/src/events.rs`:
  `usage_is_folded_into_the_model_breakdown`,
  `a_record_with_no_model_lands_under_the_unreported_label`,
  `a_duplicate_record_does_not_reach_the_model_breakdown`,
  `pruning_trims_the_model_breakdown_with_the_series`,
  `the_model_breakdown_survives_a_checkpoint_round_trip`,
  `a_checkpoint_written_before_the_model_breakdown_existed_still_restores`.
- [ ] **Step 2:** `cargo test -p quotadeck-core events` — expect FAIL.
- [ ] **Step 3:** Add `models: BreakdownSeries` to `EventIndex`. Fold in `ingest_usage` at the same guard the bucket series uses (`if !delta.is_zero() || usage.requests != 0.0`), so a deduped or zero-delta record never reaches it. Trim in `prune`. Add `#[serde(default)] models: BreakdownSeries` to `EventIndexCheckpoint`.
- [ ] **Step 4:** `cargo test -p quotadeck-core` — expect PASS.
- [ ] **Step 5:** Perf gate: `cargo test -p quotadeck-core --release --test perf -- --ignored` — all four budgets must stay green. **If red, stop and fix.**

### Task 1.3: Carry it to the dashboard payload

**Files:**
- Modify: `app/src/deck.rs:76-81` (`ProviderHistory`), `app/src/lib.rs:1002-1009` (the fill site).

**Interfaces:**
- Produces: `ProviderHistory { id, hours, models: Vec<BreakdownPoint>, modelsDropped: u64 }` over IPC (camelCase).

- [ ] **Step 1:** Add `models: Vec<BreakdownPoint>` and `models_dropped: u64` to `ProviderHistory`.
- [ ] **Step 2:** Fill both from `engine.index().models()` at the existing call site, over the same `history_from..now` range `hours` uses.
- [ ] **Step 3:** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` — expect clean.

### Task 1.4: The panel-side fold and its test

**Files:**
- Create: `ui/src/breakdown.ts`, `ui/src/breakdown.test.ts`
- Modify: `ui/src/types.ts`

**Interfaces:**
- Produces:
  - `export interface BreakdownPoint { start: number; label: string | null; tokens: TokenRollup; cost: CostRange }`
  - `export interface BreakdownRow { label: string | null; tokens: number; costUsd: number; unpricedTokens: number; share: number }`
  - `export function foldBreakdown(points: BreakdownPoint[], fromSeconds: number, toSeconds: number): BreakdownRow[]`

- [ ] **Step 1: Write the failing tests** in `ui/src/breakdown.test.ts`:
  points outside the range are excluded; rows are sorted by cost descending with tokens as tiebreak; `share` sums to 1 across rows when anything was priced; a `null` label survives the fold; unpriced tokens are carried, not folded into cost; an empty input returns an empty array (never a fabricated row).
- [ ] **Step 2:** `cd ui && npx vitest run breakdown` — expect FAIL.
- [ ] **Step 3:** Implement `foldBreakdown`. Share is computed over cost where every row is priced, and over tokens otherwise — mixing the two would rank an unpriced model at zero.
- [ ] **Step 4:** `cd ui && npx vitest run` — expect PASS. `npx tsc --noEmit` clean.

### Task 1.5: `BreakdownList` and the dashboard

**Files:**
- Create: `ui/src/components/BreakdownList.tsx`
- Modify: `ui/src/Dashboard.tsx`, `ui/src/i18n/en.ts`, `ui/src/i18n/tr.ts`, `ui/src/styles/panel.css`, `ui/src/demo.ts`

- [ ] **Step 1:** `BreakdownList` renders label, share bar (neutral ink ramp — **never** the level ramp), token count and cost, each through `Intl` via the existing `ui/src/format.ts`. A `null` label renders the localized "model not reported" string. `modelsDropped > 0` renders an explicit "N labels not shown" line.
- [ ] **Step 2:** Mount it on the dashboard under the existing range picker, folding with `foldBreakdown` over the selected range.
- [ ] **Step 3:** Add every string to both catalogues. The i18n test compares key paths and arity, so a missing Turkish key fails the build.
- [ ] **Step 4:** Extend `ui/src/demo.ts` with model points so the sample deck shows the list.
- [ ] **Step 5:** `cd ui && npx tsc --noEmit && npx vitest run` — expect clean.

### Task 1.6: Verify, changelog, commit, push

- [ ] **Step 1:** `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] **Step 2:** `cargo test --workspace`
- [ ] **Step 3:** `cargo test -p quotadeck-core --release --test perf -- --ignored`
- [ ] **Step 4:** `cd ui && npx tsc --noEmit && npx vitest run`
- [ ] **Step 5:** `cd site && npm run build`
- [ ] **Step 6:** Add the `CHANGELOG.md` entry under a `## 2026-08-05` heading.
- [ ] **Step 7:** `git add -A` then `git commit -m "feat(dashboard): what the quota was spent on, by model"` then `git push` — three separate commands.

---

## Phase 2 — Project breakdown

**Why:** `anthropics/claude-code#33978` consolidates 10+ issues; per-project allocation is the named use case. Claude Code's path is `projects/<encoded-cwd>/<session>.jsonl`, so the project is already in `LineSource::path` — attribution costs no extra I/O.

### Task 2.1: Derive the project label from the source path

**Files:** Modify `core/src/provider.rs` (`LineSource`).

**Interfaces:**
- Produces: `pub fn project_label(&self, root: &Path) -> Option<String>` on `LineSource` — the first path segment below `root`, returned verbatim. Decoding Claude Code's encoded cwd is **not** attempted here; the provider owns that.

- [ ] Write the failing test, run it, implement, run it, clippy.

### Task 2.2: Set `project` on every `UsageEvent`

**Files:** Modify `core/src/events.rs` (`UsageEvent.project: Option<String>`), `providers/src/claude_code.rs`, `providers/src/codex.rs`, `providers/src/copilot_cli.rs`.

- [ ] Claude Code: decode the encoded-cwd segment into a readable project name; a segment that does not decode is passed through verbatim rather than dropped.
- [ ] Codex: read the session's `cwd` where the rollout records one; `None` otherwise.
- [ ] Copilot: `None` unless a real fixture proves a project field exists. **Do not guess a schema.**
- [ ] Fixture tests per provider under `tests/fixtures/<provider>/`, anonymized.

### Task 2.3: Second `BreakdownSeries`, payload, UI

- [ ] `projects: BreakdownSeries` in `EventIndex`, same fold/trim/checkpoint treatment as Task 1.2.
- [ ] `ProviderHistory.projects` + `projectsDropped`.
- [ ] Reuse `BreakdownList` — no second component.
- [ ] Both catalogues, demo data.

### Task 2.4: Verify, changelog, commit, push

- [ ] Same six checks as Task 1.6, then `git commit -m "feat(dashboard): which project the quota went to"` and a separate `git push`.

---

## Phase 3 — Runaway agent detection

**Why:** the differentiator. `providers/src/claude_code.rs:274-277` already globs subagent and workflow transcripts; no scanned competitor reads them. What is missing is that `UsageEvent` does not record which glob a record came from, so subagent spend cannot be separated from the main thread.

### Task 3.1: Origin on the event

**Files:** Modify `core/src/provider.rs`, `core/src/events.rs`, `providers/src/claude_code.rs`.

**Interfaces:**
- Produces: `pub enum AgentOrigin { Main, Subagent, Workflow }` in `core/src/events.rs`, and `UsageEvent.origin: AgentOrigin`.
- `AgentOrigin` is derived from the path shape, matching the three globs already declared.

- [ ] Failing test per glob shape, implement, pass, clippy.

### Task 3.2: Third `BreakdownSeries` + the burst rule

**Files:** Modify `core/src/events.rs`, create `core/src/burst.rs`.

**Interfaces:**
- Produces: `pub struct Burst { pub since: DateTime<Utc>, pub tokens: u64, pub cost: CostRange, pub agents: usize }` and
  `pub fn detect(series: &BreakdownSeries, now: DateTime<Utc>) -> Option<Burst>`.
- A burst is reported only from **counted, non-main-thread** usage. Thresholds are derived from the user's own retained history, not from a hardcoded token figure — a fixed number would fire constantly for a heavy user and never for a light one.

- [ ] Failing tests: no burst from an empty history; no burst from main-thread-only usage; a burst when agent spend in the last hour exceeds the user's own profile by the configured factor; no burst reported twice for the same crossing (the hysteresis rule `app/src/alerts.rs` already established).

### Task 3.3: Notification and panel surface

**Files:** Modify `app/src/alerts.rs`, `app/src/i18n.rs`, `ui/src/components/ProviderCard.tsx`, both catalogues.

- [ ] Notification copy lives in Rust — the panel is usually closed and its webview may be suspended. Same argument as Phase 7.
- [ ] The panel shows the burst as a distinct row, not on the level ramp.

### Task 3.4: Verify, changelog, commit, push

- [ ] Same six checks, then `git commit -m "feat(alerts): the agents nobody was watching"` and a separate `git push`.

---

## Phase 4 — Export and exit codes

**Why:** makes the app scriptable without touching the network. `quotadeck-debug` has three subcommands and no export.

### Task 4.1: `export` subcommand

**Files:** Modify `app/src/bin/debug.rs`; create `app/src/export.rs`.

**Interfaces:**
- Produces: `pub fn to_json(state: &DeckState, history: &[ProviderHistory]) -> Result<String>` and
  `pub fn to_csv(history: &[ProviderHistory]) -> Result<String>`.
- CLI: `quotadeck-debug export [--json|--csv] [--provider <key>]`, writing to stdout only. No file is written outside the app's own data directory.

- [ ] Failing tests on both serializers, including a row whose cost is `Unpriced` — it must appear as an explicit empty cost with its token count, never as `0`.

### Task 4.2: Exit codes

- [ ] `0` ok, `10` near limit, `11` limit hit, `20` indeterminate — the vocabulary the ecosystem already uses. Documented in `docs/STORE.md` and asserted in a test.

### Task 4.3: Verify, changelog, commit, push

- [ ] Same six checks, then `git commit -m "feat(cli): a reading a script can read"` and a separate `git push`.

---

## Self-Review

**Spec coverage.** Four phases, in the order the user approved (model → project → agent → export). Each ends with a commit and a push as separate commands. Each phase's verification block runs the same six checks that `docs/RELEASE_CHECKLIST.md` records as the code-side gate.

**Placeholder scan.** Phase 1 carries full interfaces and named tests because it is executed first. Phases 2–4 carry exact files, exact type signatures, and named test obligations; their step-level code is written when the phase starts, against the code as it actually exists after the preceding phase lands. This is deliberate — writing literal code now for `core/src/burst.rs`, three phases ahead of the types it consumes, would be a guess presented as a plan.

**Type consistency.** `BreakdownSeries`, `BreakdownPoint`, `MAX_BREAKDOWN_LABELS` and `labels_dropped()` are named once in Task 1.1 and used under those names in Tasks 1.2, 1.3, 2.3 and 3.2. The TypeScript mirror is `BreakdownPoint` / `BreakdownRow` / `foldBreakdown`. `ProviderHistory` gains `models`, `projects` and `agents`, each with a matching `*Dropped` counter.

**Known risk.** Three hourly label-keyed series triple the checkpoint payload and add to resident memory. The 60 MB budget currently sits at 6.9 MB, and the perf gate is asserted after every phase. If it goes red, the project rule applies: stop and fix before continuing.
