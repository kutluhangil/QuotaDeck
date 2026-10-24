# Gemini CLI fixtures

No Gemini CLI installation was available when this provider was written, so every file is
`synthetic_`, built from the chat-recording shapes ccusage's reader documents. Message text
is `redacted`. Replace with real recordings before release (`docs/USER_ACTIONS.md`).

| File | What it covers |
|---|---|
| `synthetic_session.json` | Whole-session document: user turns, a reply whose `input` includes the cache (total matches the sum without it), a reply whose total already excludes it, a zero reply |
| `synthetic_session.jsonl` | Line-per-message session where one reply is written twice with growing counters |
