# Claude Code fixtures

Files without a `synthetic_` prefix are real rows from `~/.claude/projects/**/*.jsonl` on the
developer's machine, anonymised: `message.content` emptied, `toolUseResult` dropped, `cwd` and
`gitBranch` replaced. Every counter, id, model name and timestamp is verbatim — those are what
the parser reads.

| File | What it covers |
|---|---|
| `assistant_usage.jsonl` | Ordinary assistant row: `claude-opus-5`, a 38 696-token **one-hour** cache write |
| `subagent_usage.jsonl` | A subagent transcript (`isSidechain: true`) whose calls bill to the same subscription, with a **five-minute** cache write |
| `duplicate_pair.jsonl` | Two rows of one streamed response: chained by `parentUuid`, different `uuid` and `timestamp`, identical `message.id`, `requestId` and `usage` |
| `synthetic_error_row.jsonl` | An API error row — model `<synthetic>`, every counter zero |
| `synthetic_noise.jsonl` | `user`, `attachment`, `system`, `file-history-snapshot` and `queue-operation` rows, all of which must be ignored |

`duplicate_pair.jsonl` is the fixture that matters most. Measured over 24 real files: 3412
usage rows, **1561 repeats (45.8%)**. Deduping on `uuid` catches **zero** of them, because it
is regenerated per row; `(message.id, requestId)` catches all of them.

`statusline_reading.jsonl` is not hand-written — it is the literal output of
`quotadeck-statusline`, fed the payload captured live in Phase 0 (`docs/DISCOVERY.md` §3.1),
with only the `at` timestamp frozen so the fixture is deterministic. It is therefore also the
proof of what the shim does **not** write: the source payload carried `cwd`, `session_id`,
`transcript_path`, `model` and `context_window`, and none of them appear here.

`synthetic_` files cover shapes the schema permits but this machine has not produced.

| File | What it covers |
|---|---|
| `synthetic_statusline_partial.jsonl` | `five_hour` absent, then `rate_limits` empty — both documented as possible before a session's first API response |
| `synthetic_unpriced_model.jsonl` | A model id the embedded price table does not know. Its tokens count; its cost does not, and the estimate says so |
| `synthetic_usage_without_cwd.jsonl` | A usage row carrying no `cwd`. All 9655 rows measured here carried one; a row that does not is left unattributed rather than labelled from the encoded directory name |
