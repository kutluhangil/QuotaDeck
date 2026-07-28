# Copilot CLI fixtures

Files without a `synthetic_` prefix are verbatim records from real `events.jsonl` logs on the
developer's machine, with the event `id` and `parentId` uuids replaced and the GitHub request
identifiers in the error message masked. Nothing else was changed. A `session.shutdown`
record carries only counters, and `codeChanges.filesModified` was empty in every session here;
the records that do hold prompts and replies (`user.message`, `assistant.message`) are never
read by this parser and none are checked in.

| File | What it covers |
|---|---|
| `shutdown_gpt_mini.jsonl` | Ordinary session: one model, cache reads inside `inputTokens`, 0.33 premium requests |
| `shutdown_haiku_cache_write.jsonl` | Cache-write heavy session where `tokenDetails.input` reports 9 against a real 6128 — the record that rules `tokenDetails` out |
| `shutdown_premium_whole_request.jsonl` | A whole premium request, no cache activity |
| `shutdown_no_usage.jsonl` | Empty `modelMetrics` with zero credits (15 of 186 sessions here) |
| `session_error_quota_exceeded.jsonl` | `errorCode: "quota_exceeded"`, status 402 — the monthly allowance was gone |
| `session_error_context_limit.jsonl` | A `context_limit` error, which says nothing about quota and must report no limit |

`synthetic_` files cover shapes the schema permits but this machine never produced.

| File | What it covers |
|---|---|
| `synthetic_multi_model.jsonl` | Two models inside one `modelMetrics` map |
| `synthetic_noise.jsonl` | Record types that must be ignored, plus a shutdown with `data: null` and one with no timestamp |
