# Codex CLI (three)

This document describes how three maps config to the Codex CLI, how sessions are resumed,
and Codex-specific notes. Output modes and parsing rules live in `docs/cli-output-modes.md`.

## Scope

- Backend: `codex`
- Non-interactive `codex exec` mode
- Output details: `docs/cli-output-modes.md` (authoritative)

## Command template (conceptual)

```
exec
{% if capabilities.filesystem == 'read-only' %}--sandbox read-only{% endif %}
{% if capabilities.filesystem == 'read-write' %}--sandbox workspace-write{% endif %}
{% if capabilities.filesystem == 'danger-full-access' %}--sandbox danger-full-access{% endif %}

{% if not session_id and model != 'default' %}--model {{ model }}{% endif %}
{% if session_id and model != 'default' %}-c model={{ model }}{% endif %}

{% if options.model_reasoning_effort %}-c model_reasoning_effort={{ options.model_reasoning_effort }}{% endif %}
{% if options.text_verbosity %}-c text_verbosity={{ options.text_verbosity }}{% endif %}

--skip-git-repo-check
{% if not session_id %}-C {{ workdir }}{% endif %}
--json

{% if session_id %}resume {{ session_id }}{% endif %}
{{ prompt }}
```

## Parameter mapping

### Model

- No session: `--model <model>`
- With session: `-c model=<model>` (avoid `--model`)
- If `model == "default"`, three omits model flags.

### Prompt

- Prompt is the last positional argument.
- No stdin usage and no implicit `--` separator.

### Session resume

- If `session_id` exists: `resume <session_id>`
- `--continue` (resume by cwd) is not used.

### Filesystem

- `filesystem` -> `--sandbox read-only|workspace-write|danger-full-access`

### options / variants

- Passed via `-c key=value` (e.g. `model_reasoning_effort`, `text_verbosity`).
- Variants map into `-c` the same way.

## Output modes

See `docs/cli-output-modes.md`.

## Notes and limitations

- `codex exec resume` supports fewer flags than `codex exec`.
- Do not rely on `--model` when resuming; use `-c model=...`.
- Default text output is streaming and mixes reasoning with output; three avoids it.

## Default model behavior

When `model == "default"`, three does not pass model flags; Codex uses its configured default.
