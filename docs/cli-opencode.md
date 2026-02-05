# OpenCode CLI (three)

This document describes how three maps config to the OpenCode CLI, how sessions are resumed,
and OpenCode-specific notes. Output modes and parsing rules live in `docs/cli-output-modes.md`.

## Scope

- Backend: `opencode`
- Non-interactive `opencode run` mode
- Output details: `docs/cli-output-modes.md` (authoritative)

## Command template (conceptual)

```
run
{% if model != 'default' %}-m {{ model }}{% endif %}
{% if session_id %}-s {{ session_id }}{% endif %}
--format json
{{ prompt }}
```

## Parameter mapping

### Model

- `roles.<id>.model` -> `-m <provider/model>`
- If `model == "default"`, three omits `-m`.

### Prompt

- Prompt is the last positional argument.

### Session resume

- If `session_id` exists: `-s <id>`
- `--continue` is not used.

### Filesystem

- OpenCode CLI has no read-only flag.
- three rejects read-only roles for this backend.

### options / variants

- Not mapped by default; extend the adapter template if needed.

## Output modes

See `docs/cli-output-modes.md`.

## Notes and limitations

- `--format default` does not include a session id; three always uses `--format json`.
- Only `type: text` events carry `part.text`; other events are ignored.

## Default model behavior

When `model == "default"`, three does not pass `-m`; OpenCode uses its configured default.
