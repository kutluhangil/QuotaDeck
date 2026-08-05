# Codex fixtures

Files without a `synthetic_` prefix are verbatim lines from real `rollout-*.jsonl` logs on
the developer's machine. They contain no user text: a `token_count` record carries only
counters and limit readings.

| File | What it covers |
|---|---|
| `token_count_plus.jsonl` | Ordinary record: token totals plus a 7-day window on the `plus` plan |
| `token_count_high_usage.jsonl` | A 30-day window reported at 100% |
| `token_count_go_monthly.jsonl` | The same `primary` slot carrying a 30-day window |
| `token_count_zero_breakdown.jsonl` | `rate_limits: null` and an all-zero breakdown with a non-zero `total_tokens` (2.2% of records) |
| `token_count_null_info.jsonl` | `info: null` with limits present |
| `token_count_premium_no_window.jsonl` | `limit_id: "premium"`, both window slots null |
| `session_meta.jsonl` | The record opening a rollout file, carrying the session's `cwd` — the only place Codex names the directory the work was done in. The session uuid and the path are replaced; nothing else was changed |

`synthetic_` files cover shapes the schema permits but this machine never produced. They
exist so a CLI build that starts emitting them is handled rather than silently ignored.

| File | What it covers |
|---|---|
| `synthetic_both_windows.jsonl` | `primary` and `secondary` both populated |
| `synthetic_session_progression.jsonl` | Three cumulative totals from one session |
| `synthetic_noise.jsonl` | Record types that must be ignored |
