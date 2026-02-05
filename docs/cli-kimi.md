# Kimi CLI (three)

This document describes how three maps config to the Kimi CLI, how sessions are resumed,
and Kimi-specific notes. Output modes and parsing rules live in `docs/cli-output-modes.md`.

## Scope

- Backend: `kimi`
- Non-interactive print mode
- Output details: `docs/cli-output-modes.md` (authoritative)

## Command template (conceptual)

```
--print
--thinking
--output-format text
--final-message-only
--work-dir {{ workdir }}
{% if model != 'default' %}--model {{ model }}{% endif %}
{% if resume and not session_id %}--continue{% endif %}
{% if session_id %}--session {{ session_id }}{% endif %}
--prompt {{ prompt_or_guardrail }}
```

`prompt_or_guardrail`:

```
{% if capabilities.filesystem == 'read-only' %}
{{ prompt }}
[guardrail: do not write files]
{% else %}
{{ prompt }}
{% endif %}
```

## Parameter mapping

### Model

- `roles.<id>.model` -> `--model <model>`
- If `model == "default"`, three omits `--model`.

### Prompt

- Passed as a single argument to `--prompt`.

### Session resume

- If `session_id` exists: `--session <id>`
- If no session id but history exists for the same repo+role: `--continue`

### Filesystem / approval

- Kimi CLI does not expose a read-only flag.
- three uses a prompt guardrail for read-only roles (best effort). The exact guardrail
  string is defined in the adapter template.

### options / variants

- Not mapped by default; extend the adapter template if needed.

## Output modes

See `docs/cli-output-modes.md`.

## Notes and limitations

- `--print` is required to avoid interactive mode.
- Text output does not include a session id; resume relies on `--continue`.

## Default model behavior

When `model == "default"`, three does not pass `--model`; Kimi uses its configured default.
