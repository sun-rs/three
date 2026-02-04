# Three config schema (draft)

This document captures the agreed configuration shape before code changes.

## Top-level keys

The config has exactly two top-level keys:

- `backend`
- `roles`

No other top-level keys are allowed.

## backend

`backend` is a map keyed by backend name. The key **is the command** and must be one of:

- `claude`
- `codex`
- `opencode`
- `kimi`
- `gemini`

Each backend entry contains:

- `models`: Model definitions for that backend.

Adapter definitions are embedded in the server (no `adapter.json` config file).
User `config.json` should not include `adapter` fields.

The embedded adapter catalog defines the **allowed backend set**. `config.json`
may use a subset of those backends. If a backend is referenced in `config.json`
but is not supported by the catalog, validation fails.

## Embedded adapter catalog

The embedded adapter catalog defines how to call each CLI. It contains:

```json
{
  "adapters": {
    "gemini": { "args_template": [...], "output_parser": {...} },
    "codex": { "args_template": [...], "output_parser": {...} }
  }
}
```

Each adapter entry contains:

- `args_template`: Array of template tokens (MiniJinja). Each array entry is rendered
  independently; empty results are dropped. Do **not** put multiple CLI tokens into
  a single entry.
- `output_parser`: How to extract session id and agent message from stdout.
- `filesystem_capabilities` (optional): List of supported filesystem values
  (`read-only`, `read-write`). If provided, roles requesting a value
  outside this list will fail during `resolve_profile`.

Template context variables (stable names):

- `prompt` (string)
- `model` (string; selected model id)
- `session_id` (string or empty)
- `workdir` (string)
- `options` (object; merged model options + variant overrides)
- `capabilities` (object; from the selected role)
- `include_directories` (string; comma-separated extra dirs inferred from prompt)

`output_parser` types:

- `json_stream`
  - `session_id_path` (string)
  - `message_path` (string)
  - `pick` (string: `first` or `last`)
- `json_object`
  - `message_path` (string)
  - `session_id_path` (string, optional; omit to treat as stateless)
- `regex`
  - `session_id_pattern` (string; regex)
  - `message_capture_group` (number)
- `text`
  - Treats stdout as plain text
  - `session_id` is always `stateless`

## backend.<name>.models

`models` is a map keyed by **model id** (the key is the id). There is no `id` field.

Each model entry can include:

- `options` (object, optional)
- `variants` (object, optional)

Rules:

- `options` / `variants` values are **basic types only**: string, number, bool.
- `variants` is an object map. Each variant overrides/extends `options` by upsert.
- Final options are resolved as: base `options` + variant overrides.

## roles

`roles` integrates the old roles/personas. Each role entry contains:

- `model`: A string in the form `backend/model@variant` (variant optional).
  - Example: `codex/gpt-5.2-codex@xhigh`
  - The `model` segment may include `/` (e.g. `opencode/cchGemini/gemini-3-flash-preview`).
    Only the first `/` splits `backend` from `model`.
  - Special case: `backend/default` uses the CLI's default model and may omit a backend.models entry.
    Variants are not allowed for `default`.
- `personas`: Required object with:
  - `description` (string)
  - `prompt` (string)
- `capabilities`: Required object with unified capability semantics:
  - `filesystem`: `read-only` | `read-write`
  - `shell` (optional, default `deny`): `allow` | `deny`
  - `network` (optional, default `deny`): `allow` | `deny`
  - `tools` (optional, default `[]`): list of tool names or `*`

`capabilities` are semantic and are mapped to CLI flags in `adapter.args_template`.
Adapters may optionally declare `filesystem_capabilities` to enforce supported values.
If the list exists and a role requests a filesystem capability not in the list,
`resolve_profile` fails for that role (config load still succeeds).

## Role → CLI mapping (summary)

The only per-role inputs that can reach a CLI are:

- `model` (backend/model@variant) → becomes the backend `model` string.
- `personas` → used to build the final prompt (system + persona + user task).
- `capabilities` → passed to adapter as `capabilities.*` for flag mapping.
- `options` / `variants` → merged into `options` and exposed to adapter.

Anything else must be added explicitly in the adapter template; there is no
implicit per-role flag injection.

## Example (minimal)

See `examples/config.json` for a full sample.

## Migration note

If you have an existing config that uses `brains`, rename the top-level key to `roles`:

```json
{
  "backend": { "...": {} },
  "roles": { "...": {} }
}
```
