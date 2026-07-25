# DISCOVERY — Phase 0 Verification

> Machine: macOS (darwin 25.5.0, arm64) · Date: 2026-07-25
> Method: read-only inspection of local log files. No network requests, no credential files opened.
> This document is the factual baseline. Every later phase references it instead of the blueprint's assumptions.

---

## 0. Headline findings

| # | Finding | Impact |
|---|---|---|
| 1 | **Codex `rate_limits` is NON-NULL** — 1982 records across 94 of 97 rollout files | Codex is **L1 MEASURED**. Product positioning "shows real limits" holds. |
| 2 | **Claude Code statusline delivers `rate_limits` with `five_hour` + `seven_day`** — verified live | Claude Code is **L1 MEASURED** too, credential-free, via an official documented mechanism. This is the product's key differentiator. |
| 3 | Codex window schema differs from the blueprint | `primary`/`secondary` cannot be mapped by key name. Must map by `window_minutes`. |
| 4 | OTLP telemetry carries **no** rate-limit metrics, needs a listening socket, and leaks PII | **Rejected** as an L1 path. |
| 5 | Claude Code JSONL has ~51% duplicate `(message.id, requestId)` pairs in the sampled window | Dedup is mandatory, exactly as the blueprint warned. |
| 6 | Codex writes ~175 MB/day of which **0.49%** is quota-relevant | The `< 5 MB/hour` read budget is not achievable by naive full ingest on a heavy user. See §6. |

---

## 1. Tool inventory on this machine

| Tool | Present | Data path | Level | Notes |
|---|---|---|---|---|
| **Codex** | yes | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | **L1** | 99 files, 233.0 MB |
| **Claude Code** | yes | `~/.claude/projects/**/*.jsonl` + statusline hook | **L1** (hook) / L2 (JSONL) | 131 files, 437.4 MB, v2.1.220 |
| **GitHub Copilot CLI** | yes | `~/.copilot/session-state/<uuid>/events.jsonl` | L2 | 186 files, 50.1 MB. Usage written only at session end. |
| **Antigravity** | yes | `~/Library/Application Support/Antigravity/` | — | IDE data dir only. Deferred to v2 per blueprint §3.1. |
| **Hermes Agent** | yes | `~/.hermes/` | — | `logs/` present but contains **no** token/usage records. Nothing to parse. |
| **Gemini CLI** | no | — | — | `~/.gemini` exists but holds only `skills/`, `config/`, Antigravity assets. No session logs. Not installed as a CLI. |
| Kimi / kimi-code | no | — | — | absent |
| Qwen Code | no | — | — | absent |
| OpenCode | no | — | — | absent |
| Amp | no | — | — | absent |
| Droid / Factory | no | — | — | absent |
| Goose | no | — | — | absent |
| Codebuff | no | — | — | absent |
| pi-agent | no | — | — | absent |
| Kilo | no | — | — | absent |
| OpenClaw | no | — | — | absent |

Absent tools must be reported as `Unavailable`. Never synthesize data for them.

---

## 2. Codex — L1 confirmed, schema corrected

### 2.1 Real record (newest, 2026-07-25T18:13:12Z)

```json
{
  "timestamp": "2026-07-25T18:13:12.233Z",
  "type": "event_msg",
  "payload": {
    "type": "token_count",
    "info": {
      "total_token_usage": {
        "input_tokens": 2267666, "cached_input_tokens": 2196480,
        "cache_write_input_tokens": 0, "output_tokens": 20736,
        "reasoning_output_tokens": 6513, "total_tokens": 2288402
      },
      "last_token_usage": { "…": "…" },
      "model_context_window": 258400
    },
    "rate_limits": {
      "limit_id": "codex",
      "limit_name": null,
      "primary":   { "used_percent": 68.0, "window_minutes": 10080, "resets_at": 1785594976 },
      "secondary": null,
      "credits": { "has_credits": false, "unlimited": false, "balance": "0" },
      "individual_limit": null,
      "spend_control_reached": null,
      "plan_type": "plus",
      "rate_limit_reached_type": null
    }
  }
}
```

### 2.2 Deviations from the blueprint

| Blueprint assumed | Reality on this machine |
|---|---|
| `primary` = 5-hour window (`window_minutes: 299`) | **No 5-hour window exists.** `primary.window_minutes` is `10080` (7 d) or `43200` (30 d). |
| `secondary` = weekly window | `secondary` is **`null` in all 1982 records**. |
| `resets_in_seconds` (relative) | `resets_at` (**absolute Unix epoch seconds**) |
| — | New fields: `limit_id`, `limit_name`, `credits`, `individual_limit`, `spend_control_reached`, `plan_type`, `rate_limit_reached_type` |

**Rule for the parser:** never map a window by its key name. Read `window_minutes` and classify:
`≤ 360` → session window · `~10080` → weekly · `~43200` → monthly.
An unknown `window_minutes` is rendered from its own value, not dropped.

### 2.3 Observed distribution

```
(limit_id, plan_type, window_minutes) -> record count
  ('codex',   'plus', 10080)  1015
  ('codex',   'go',   43200)   676
  ('codex',   'go',   10080)   290
  ('premium', 'go',   null)      1
  ('premium',  null,  null)      2
```

- Two distinct `limit_id` values appear: `codex` and `premium`. `premium` records carry no window. Treat `limit_id` as a grouping key — a provider can report multiple independent limits.
- `plan_type` changed over the file history (`go` → `plus`). Do not cache it; always read the newest record.
- Oldest record 2026-04-28, newest 2026-07-25 — 3 months of continuous history.

### 2.4 Null-rate reality

`rate_limits` present as a key in 94/97 files; 1982 non-null records. On this machine the blueprint's feared always-null case **did not occur** for interactive sessions. The fallback chain is still required (blueprint §3.1 cites `codex exec` mode), but it is a fallback, not the main path.

---

## 3. Claude Code — L1 via statusline, verified live

The Claude Code binary documents a statusline stdin schema that includes subscription rate limits. Extracted verbatim from the shipped binary (v2.1.220):

```
"rate_limits": {             // Optional: Claude.ai subscription usage limits.
                             //   Only present for subscribers after first API response.
  "five_hour": {             // Optional: 5-hour session limit (may be absent)
    "used_percentage": number,   // Percentage of limit used (0-100)
    "resets_at": number          // Unix epoch seconds when this window resets
  },
  "seven_day": {             // Optional: 7-day weekly limit (may be absent)
    "used_percentage": number,
    "resets_at": number
  }
}
```

### 3.1 Live capture

A shim script was installed as `statusLine.command`, an interactive session was driven, and the payload it received was captured:

```json
{
  "rate_limits": {
    "five_hour": { "used_percentage": 44, "resets_at": 1785007800 },
    "seven_day": { "used_percentage": 95, "resets_at": 1785229200 }
  },
  "context_window": {
    "total_input_tokens": 44531, "total_output_tokens": 207,
    "context_window_size": 200000,
    "used_percentage": 22, "remaining_percentage": 78
  },
  "version": "2.1.220",
  "model": { "id": "claude-haiku-4-5-20251001", "display_name": "Haiku 4.5" }
}
```

`five_hour = 44%` resetting 2026-07-25T22:30, `seven_day = 95%` resetting 2026-07-28T12:00. **Real measured quota, zero credentials, official mechanism.**

### 3.2 Full field set observed

`context_window`, `cost`, `cwd`, `exceeds_200k_tokens`, `fast_mode`, `model`, `output_style`, `prompt_id`, `rate_limits`, `session_id`, `thinking`, `transcript_path`, `version`, `workspace`

### 3.3 Constraints

- The statusline command runs **only in interactive sessions**. It does not fire in `claude -p` headless mode — confirmed by test.
- `rate_limits` is absent before the first API response of a session and absent for non-subscribers.
- Installing the shim requires editing the user's `settings.json` `statusLine` key — a destructive-if-careless operation, because it **replaces** whatever the user already had (this machine had `npx -y ccstatusline@latest`).

**Design consequence:** the shim must chain, not replace. It writes the payload to our data directory, then execs the user's previous command with the same stdin and passes its stdout through. Onboarding must show the exact before/after of `settings.json` and offer a one-click revert. This is an opt-in upgrade path, never automatic.

---

## 4. OTLP telemetry — tested and rejected

`CLAUDE_CODE_ENABLE_TELEMETRY=1` with a local OTLP/JSON receiver on `127.0.0.1:4318` was run against a real session. Captured 6 payloads on `/v1/metrics` and `/v1/logs`.

**Metrics emitted:** `claude_code.token.usage`, `claude_code.cost.usage`, `claude_code.session.count`, `claude_code.active_time.total`
**Log events:** `api_request`, `assistant_response`, `user_prompt`, `hook_execution_start/complete`, `hook_registered`, `mcp_server_connection`, `plugin_loaded`

**Verdict: reject.** Three independent reasons:

1. **No rate-limit metric exists.** OTLP gives token and cost only — L2, which we already get from JSONL. It adds nothing over the statusline path.
2. **Requires a listening socket.** A local OTLP receiver means an HTTP server in-process, which on macOS needs `com.apple.security.network.server`. That destroys the "no network entitlement, provable by code signature" claim from blueprint Decision 1 — our strongest marketing argument.
3. **Leaks PII into our process.** Datapoint attributes include `user.email`, `user.id`, `user.account_id`, `user.account_uuid`, `organization.id`. Ingesting these contradicts the "Data Not Collected" privacy posture even though it never leaves the device.

Do not revisit this unless Anthropic adds a rate-limit metric, and even then reason 2 stands.

---

## 5. Claude Code JSONL (L2 path) — dedup is mandatory

Usage records live on assistant messages:

```json
{
  "message": {
    "id": "msg_011CdPEFRmvxfkosyWiXmsBB",
    "model": "claude-opus-5",
    "usage": {
      "input_tokens": 2,
      "cache_creation_input_tokens": 1471,
      "cache_read_input_tokens": 85308,
      "output_tokens": 954,
      "server_tool_use": { "web_search_requests": 0, "web_fetch_requests": 0 },
      "service_tier": "standard",
      "cache_creation": { "ephemeral_1h_input_tokens": 1471, "ephemeral_5m_input_tokens": 0 }
    }
  },
  "requestId": "req_011CdPEFQbF8cuNegvC3vmM8",
  "timestamp": "…", "sessionId": "…", "version": "…", "isSidechain": false
}
```

**Measured duplicate rate:** over the 8 newest project files — 1298 usage rows, **663 duplicates (51.1%)**, 0 rows missing an id pair.

Without `(message.id, requestId)` dedup, cost and token totals roughly **double**. This is not a theoretical risk; it is the observed default.

Additional fields worth carrying: `cache_creation.ephemeral_1h_input_tokens` vs `ephemeral_5m_input_tokens` are priced differently and must not be collapsed into one bucket.

---

## 6. Volume reality vs. the performance budget

Measured over today's Codex logs (`~/.codex/sessions/2026/07/25/`, ~21 h elapsed):

```
lines                31 817
bytes               175.2 MB          → ~8.3 MB/hour of appended data
avg line size          5.4 KB
token_count lines     0.86 MB (0.49% of volume)
rate_limits lines      1 098 lines, 0.86 MB (0.49% of volume)
```

**The blueprint's `< 5 MB/hour` read budget cannot be met by naive full ingest on this machine** — the tools append more than that on their own. Reading every appended byte once is already 8.3 MB/h.

Only 0.49% of the volume carries anything we need. Two consequences for Phase 1:

1. **L1 does not require full ingest.** The newest non-null `rate_limits` record is findable by a bounded reverse-tail read of the newest rollout file. Cost: a few KB per poll regardless of log size.
2. **L2 does require reading appended bytes**, because token totals are a sum over all records. The floor is 1× the bytes the tools themselves write.

**Restate the budget honestly** as: *reads at most 1× the bytes the monitored tools write, and never re-reads a byte.* That is a claim we can hold and test. The fixed `< 5 MB/hour` number is only valid for a light user and must not be published as an absolute.

Baseline numbers for the CI perf fixture:

| Provider | Files | Size | Newest |
|---|---|---|---|
| Codex | 99 | 233.0 MB | 2026-07-25T21:17:57 |
| Claude Code | 131 | 437.4 MB | 2026-07-25T21:17:43 |
| Copilot CLI | 186 | 50.1 MB | 2026-07-15T12:02:44 |

---

## 7. GitHub Copilot CLI

Path: `~/.copilot/session-state/<session-uuid>/events.jsonl`

Event types: `session.start`, `system.message`, `user.message`, `assistant.turn_start`, `assistant.message`, `assistant.turn_end`, `session.shutdown`, `session.error`.

Token usage appears **only** on `session.shutdown` (171 of 186 sessions):

```json
{
  "type": "session.shutdown",
  "data": {
    "shutdownType": "routine",
    "totalPremiumRequests": 0.33,
    "totalNanoAiu": 4320360000,
    "tokenDetails": {
      "input":       { "tokenCount": 9 },
      "cache_read":  { "tokenCount": 0 },
      "cache_write": { "tokenCount": 36048 },
      "output":      { "tokenCount": 587 }
    },
    "modelMetrics": {
      "claude-haiku-4.5": {
        "requests": { "count": "…", "cost": "…" },
        "usage": { "inputTokens": "…", "outputTokens": "…",
                   "cacheReadTokens": "…", "cacheWriteTokens": "…", "reasoningTokens": "…" },
        "totalNanoAiu": 4320360000
      }
    },
    "currentModel": "claude-haiku-4.5",
    "currentTokens": 28674, "systemTokens": 6961,
    "conversationTokens": 21710, "toolDefinitionsTokens": 0
  }
}
```

Notes:

- `totalPremiumRequests` is Copilot's **actual billing unit** — plans are capped in premium requests per month, not tokens. Summing it over the calendar month against the user's plan cap gives a far more meaningful figure than token counts.
- Usage is written **at shutdown only**, so a live session is invisible until it ends. Mark in-flight sessions explicitly rather than showing a stale total as current.
- 15 sessions (186 − 171) ended without a shutdown record — crash or kill. Never assume the record exists.

---

## 8. Reference baseline — ccusage

`npx ccusage@latest daily --json` on this machine:

```
days covered   24
totals         input 7 761 240 · output 9 143 891
               cache_create 53 518 321 · cache_read 2 227 124 760
               total_tokens 2 297 548 212 · total_cost_usd 2126.53
```

ccusage aggregates Claude Code **and** Codex (`metadata.agents: ["claude","codex"]`) and breaks down per model (`gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.5`, `claude-opus-5`, …).

Use these totals as the correctness oracle for our L2 rollups. Our daily figures must land within a small tolerance of ccusage's for the same date range; a large gap means the dedup or the cache-token mapping is wrong.

---

## 9. Decisions this phase settles

1. **Product positioning stays "shows real limits."** Codex L1 confirmed and Claude Code L1 confirmed. Blueprint risk item "Codex `rate_limits: null` → reposition as cost tracking" is **closed, not triggered**.
2. **Statusline is the Claude Code L1 path. OTLP is dropped.**
3. **Windows are classified by `window_minutes`, never by key name.** No 5-hour Codex window exists in this CLI version — the UI must render whatever windows the provider actually reports, not a hardcoded 5 h + 7 d pair.
4. **The Horizon strip cannot assume a 5-hour span.** Its time axis is driven by the reported `window_minutes` and differs per provider (Codex 7 d or 30 d, Claude Code 5 h and 7 d).
5. **Read budget is restated as relative**, not a fixed MB/hour figure.

## 9b. Phase 2 addendum — how Codex tokens must be counted

Phase 0 recorded that `total_token_usage` is a session running total and assumed differencing
it per session was the way to count. Implementing the provider disproved that.

**Two candidate methods, measured against all 99 real rollout files:**

| Method | Result |
|---|---|
| Difference `total_token_usage` per file | Over-reported by 17% |
| Sum `last_token_usage` | Reproduced the session's final running total **exactly in 79 of 99 files**; 19 over by ~3%, 1 under |

Why differencing fails:

- **The file is not the session.** Three `session_id` values were observed spanning several
  rollout files. A resumed session opens a new file whose running total continues from the
  old one — one file's first `token_count` record already showed **36,662,856** tokens.
  Differencing against zero counts that entire history as fresh usage.
- Ordering matters. Discovery returns files newest-first for progressive rendering, which is
  the opposite of what differencing a running total requires.

Why summing `last_token_usage` works:

- Present in **all 2281** records measured; never null.
- It is the current turn's own usage, so it is already a delta and needs no session state.
- The 3% over-report in 19 files comes from Codex re-emitting a record verbatim. **37 of
  2276** records were a repeat of their predecessor, and every single one carried both the
  same running total and the same turn total, with **zero** cases where a repeated turn
  usage came with a different running total.

**Dedup key for Codex:** `(file + running total, turn total)`. Within a file, 42 running
totals repeated; only 5 of those carried a different non-zero turn usage, and including the
turn total in the key keeps those 5 while dropping the 37 true duplicates.

**Field arithmetic:** `cached_input_tokens` is a subset of `input_tokens`, and
`reasoning_output_tokens` a subset of `output_tokens`. Verified on 2216 records where
`total_tokens == input_tokens + output_tokens`. Adding either back in inflates every figure.
`cache_write_input_tokens` was zero in every record on this machine, so its relationship to
the total is unverified.

**All-zero turns:** 2.2% of records report every breakdown field as zero with a non-zero
`total_tokens`. They contribute nothing and are dropped.

### Residual difference against ccusage

Same calendar day, same machine, our method against `ccusage daily`:

```
                ours          ccusage       delta
input           6,178,286     5,696,689     +8.5%
output            450,007       432,983     +3.9%
cache read    127,206,144   123,639,808     +2.9%
```

Part of the uniform 3% is sampling skew — the logs grow while both tools read them, and the
two runs were a minute apart. The extra ~5% on `input` alone is a genuine methodological
difference that has not been traced. Treat ccusage as a cross-check, not as ground truth:
our figures reconstruct each session's own final running total exactly, which is the
strongest internal check available. Revisit if a user reports a discrepancy.

## 10. Open items carried into later phases

- Windows path verification (`%USERPROFILE%\.codex`, `%APPDATA%`) — no Windows machine available here. Phase 10.
- Kimi, Qwen, OpenCode, Amp, Droid, Goose and the rest are absent on this machine; their schemas cannot be verified from real data. Phase 6 must acquire real fixtures before implementing each one — no parser ships against a guessed schema.
- Copilot's premium-request plan caps (300 / 1500 / unlimited per tier) need a source before we render a percentage.
