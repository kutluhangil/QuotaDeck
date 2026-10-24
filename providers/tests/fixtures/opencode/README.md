# OpenCode fixtures

No OpenCode installation was available when this provider was written, so every file is
`synthetic_`, built from the schemas ccusage's reader handles. The database fixture is SQL
text, loaded into a temporary database by the tests, so every row is reviewable. Replace
with rows from a real `opencode.db` before release (`docs/USER_ACTIONS.md`).

| File | What it covers |
|---|---|
| `synthetic_schema.sql` | Legacy `message` rows (user, complete reply, streaming reply); `session_message` rows for a parent and a fork whose first reply is a copy; `session_v2` fork boundary |
| `synthetic_message.json` | A pre-database `storage/message/<session>/<message>.json` file |
