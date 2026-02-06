# Three config schema (draft)

This document captures the agreed configuration shape before code changes.

## Top-level keys

The config has exactly two top-level keys:

- `backend`
- `roles`

## backend

`backend` is a map keyed by backend name. The key **is the command** and must be one of:

- `claude`
- `codex`
- `opencode`
- `kimi`
- `gemini`

Each backend entry contains:

- `models`: Model definitions for that backend.
- `timeout_secs` (optional): Default timeout in seconds for this backend.

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

`roles` configures technical settings for each role. Personas are built into the MCP server and can be overridden per role if needed. Each role entry contains:

- `model`: A string in the form `backend/model@variant` (variant optional).
  - Example: `codex/gpt-5.2-codex@xhigh`
  - The `model` segment may include `/` (e.g. `opencode/cchGemini/gemini-3-flash-preview`).
    Only the first `/` splits `backend` from `model`.
  - Special case: `backend/default` uses the CLI's default model and may omit a backend.models entry.
    Variants are not allowed for `default`.
- `personas` (optional): Override the built-in persona for a role.
  - If omitted, the MCP server uses its default persona (when available).
  - Unknown roles without `personas` have no injected persona.
  - Fields:
    - `description` (string)
    - `prompt` (string)
- `capabilities`: Required object with unified capability semantics (fields default to allow):
  - `filesystem` (optional, default `read-write`): `read-only` | `read-write`
  - `shell` (optional, default `allow`): `allow` | `deny`
  - `network` (optional, default `allow`): `allow` | `deny`
  - `tools` (optional, default `["*"]`): list of tool names or `*`
- `enabled` (optional, default `true`): disable a role without deleting it.
- `timeout_secs` (optional): Override timeout in seconds for this role.

`capabilities` are semantic and are mapped to CLI flags in `adapter.args_template`.
Adapters may optionally declare `filesystem_capabilities` to enforce supported values.
If the list exists and a role requests a filesystem capability not in the list,
`resolve_profile` fails for that role (config load still succeeds).

## Parsing rules: unknown fields and defaults

### Top-level

- Only `backend` and `roles` are recognized.
- Other top-level keys are ignored unless they cause a type mismatch.

### backend / models

- Unknown fields inside `backend` entries or `models` are ignored unless they cause a type error.
- Required fields described above must exist and be the correct type.

### roles

- `enabled` defaults to `true` when omitted.
- If `enabled=false`, `resolve_profile` fails for that role.
- Capability fields default to allow (`filesystem=read-write`, `shell=allow`, `network=allow`, `tools=["*"]`).
- Unknown fields inside `roles` entries are ignored unless they cause a type error.

## Timeout precedence

Timeouts resolve in this order (highest to lowest):

1) MCP tool call `timeout_secs`
2) `roles.<id>.timeout_secs`
3) `backend.<id>.timeout_secs`
4) Default `600`

## MCP tool parameter behavior (three)

This section documents how the `three` MCP tool interprets runtime parameters.

### Session key

- If `session_key` is provided, it is used verbatim for persistence/locking.
- Otherwise, the key is derived as `hash(repo_root + role + role_id)`.

### Session resume

`force_new_session=true` has the highest priority.

- If `force_new_session=true`:
  - Any provided `session_id` is ignored (a warning is returned).
  - The request is treated as a new session.
- Otherwise:
  - If `session_id` is provided, it is treated as an explicit resume.
  - Else if a session store record exists, it is reused (if the backend supports sessions).
  - Kimi uses `--continue` when the store has history (no session id available).

### Persona injection

- Persona is injected **only** for new sessions.
- If the request is considered a resume (explicit `session_id`, store hit, or Kimi `--continue`),
  persona is not re-injected.

### Contract and patch validation

- `contract=patch_with_citations` enforces a patch + citations in the model output.
- `validate_patch=true` runs `git apply --check` and fails the request if the patch is invalid.

## Role → CLI mapping (summary)

The only per-role inputs that can reach a CLI are:

- `model` (backend/model@variant) → becomes the backend `model` string.
- `personas` → used to build the final prompt (system + persona + user task). If omitted, the built-in persona is used when available.
- `capabilities` → passed to adapter as `capabilities.*` for flag mapping.
- `options` / `variants` → merged into `options` and exposed to adapter.
- `timeout_secs` → used by the server to enforce backend timeout.

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
