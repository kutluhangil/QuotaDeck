# pi fixtures

No pi installation was available when this provider was written, so every file here is
`synthetic_`: built from the session format pi documents and ccusage's reader parses
(`type: "session"` header with `cwd`, `type: "message"` records with `message.usage`).
User text is replaced with `redacted`. Replace with verbatim lines from a real session
before release (`docs/USER_ACTIONS.md`).

| File | What it covers |
|---|---|
| `synthetic_session.jsonl` | Header with `cwd`; a user turn; a priced Claude reply; an unknown model with pi's own cost; an unknown model that recorded `0`; a zero-usage reply |
