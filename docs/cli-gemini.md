# Gemini CLI (three)

This document describes how three maps config to the Gemini CLI, how sessions are resumed,
and Gemini-specific notes. Output modes and parsing rules live in `docs/cli-output-modes.md`.

## Scope

- Backend: `gemini`
- Headless mode
- Output details: `docs/cli-output-modes.md` (authoritative)

## Command template (conceptual)

```
--output-format json
{% if capabilities.filesystem == 'read-only' %}--approval-mode plan{% endif %}
{% if capabilities.filesystem != 'read-only' %}-y{% endif %}
{% if model != 'default' %}-m {{ model }}{% endif %}
{% if capabilities.filesystem == 'read-only' %}--sandbox{% endif %}
{% if include_directories %}--include-directories {{ include_directories }}{% endif %}
{% if session_id %}--resume {{ session_id }}{% endif %}
--prompt {{ prompt }}
```

## Parameter mapping

### Model

- `roles.<id>.model` -> `-m <model>`
- If `model == "default"`, three omits `-m`.

### Prompt

- Passed as a single argument to `--prompt`.

### Session resume

- If `session_id` exists: `--resume <session_id>`

### Filesystem / approval

- `filesystem: read-only` -> `--approval-mode plan` and `--sandbox`
- `filesystem: read-write` -> `-y` (auto-approve)

### include-directories

- Derived from absolute paths in the prompt that point outside the workspace.
- Non-existent directories are filtered out.

### options / variants

- Not mapped by default; extend the adapter template if needed.

## Output modes

See `docs/cli-output-modes.md`.

## Notes and limitations

- `-y` auto-approves all operations; use only in trusted environments.
- `--sandbox` is a boolean flag; it does not encode read-only vs read-write on its own.

## Default model behavior

When `model == "default"`, three does not pass `-m`; Gemini uses its configured default.
