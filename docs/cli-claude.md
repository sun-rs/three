# Claude CLI (three)

This document describes how three maps config to the Claude Code CLI, how sessions are resumed,
and Claude-specific notes. Output modes and parsing rules live in `docs/cli-output-modes.md`.

## Scope

- Backend: `claude`
- Non-interactive print mode
- Output details: `docs/cli-output-modes.md` (authoritative)

## Command template (conceptual)

```
--print
{{ prompt }}
--output-format json
{% if model != 'default' %}--model {{ model }}{% endif %}

{% if capabilities.filesystem == 'read-write' %}--dangerously-skip-permissions{% endif %}
{% if capabilities.filesystem == 'read-only' %}--permission-mode plan{% endif %}

{% if session_id %}--resume {{ session_id }}{% endif %}
```

## Parameter mapping

### Model

- `roles.<id>.model` -> `--model <model-id>`
- If `model == "default"`, three omits `--model`.

### Prompt

- Passed as a single argument to `--print`.
- No stdin usage and no implicit `--` separator.

### Session resume

- If `session_id` exists: `--resume <session_id>`
- `--continue` (resume by cwd) is not used.

### Filesystem / approval

- `filesystem: read-only` -> `--permission-mode plan`
- `filesystem: read-write` -> `--dangerously-skip-permissions`

### options / variants

- Not mapped by default; extend the adapter template if needed.

## Output modes

See `docs/cli-output-modes.md`.

## Notes and limitations

- `--print` is required to avoid interactive mode.
- `--dangerously-skip-permissions` bypasses approvals; use only in trusted environments.

## Default model behavior

When `model == "default"`, three does not pass `--model`; Claude CLI uses its configured default.
