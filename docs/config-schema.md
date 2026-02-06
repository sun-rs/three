# Three config schema (draft)

This document captures the agreed configuration shape before code changes.

## Top-level keys

The config has exactly two top-level keys:

- `backend`
- `roles`

## Config file selection (client-aware)

Three supports **client-specific configs**. If a client hint is provided, the server prefers
`config-<client>.json` before falling back to `config.json`.

Client hint sources (first match wins):

- MCP parameter: `client` (e.g. `"claude"`, `"codex"`, `"opencode"`)
- Environment variable: `THREE_CLIENT` (used when calling the server without a plugin)

Search order per layer:

1. **User config**
   - `~/.config/three/config-<client>.json`
   - `~/.config/three/config.json`
2. **Project config** (overrides user)
   - `./.three/config-<client>.json`
   - `./.three/config.json`
   - `./.three.json` (legacy fallback; no client-specific variant)

If no client hint is provided, only `config.json` / `.three.json` are considered.

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
- `fallback` (optional): Fallback model + patterns for model-not-found errors.

Adapter definitions are embedded in the server (no `adapter.json` config file).
User `config.json` should not include `adapter` fields unless you need to override
advanced behavior (prompt transport or output parsing).

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
- `prompt_transport` (optional): How to send prompt text to the backend.
  - `arg` (default): pass prompt as a CLI argument (current behavior)
  - `stdin`: write prompt to stdin (no prompt argument)
  - `auto`: use `stdin` when prompt length exceeds `prompt_max_chars`
- `prompt_max_chars` (optional): Max prompt length before `auto` switches to `stdin`
  (default: `32768`).
  - Embedded adapters default to `auto`. When stdin is selected, prompt arguments are omitted
    (no mixed argv+stdin).

Template context variables (stable names):

- `prompt` (string)
- `model` (string; selected model id)
- `session_id` (string or empty)
- `workdir` (string)
- `options` (object; merged model options + variant overrides)
- `capabilities` (object; from the selected role)
- `include_directories` (string; comma-separated extra dirs inferred from prompt)
- `prompt_transport` (string; resolved transport: `arg` or `stdin`)

`output_parser` types:

- `json_stream`
  - `session_id_path` (string)
  - `message_path` (string)
  - `pick` (string: `first` or `last`)
  - `fallback` (string, optional): `codex` enables Codex JSONL fallback parsing
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

## backend.<name>.fallback

Fallback definition for model-not-found errors. Structure:

```json
{
  "model": "backend/model@variant",
  "patterns": ["model_not_found", "unknown model"]
}
```

- `model` uses the same reference rules as `roles.<id>.model`.
- `patterns` is a list of **case-insensitive substrings** used to detect
  model-not-found errors for that backend. There is **no default**.
- If `fallback` is set, `patterns` must include at least one non-empty string.

When the primary model fails with a matching error, the server attempts the fallback
model (can span backends). Fallbacks run with the same role capabilities; if the target
backend does not support the requested filesystem capability, the fallback is skipped.
When a fallback is used, the response `warnings` includes `model fallback used: ...`.

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
- `capabilities` (optional): Unified capability semantics (fields default to allow):
  - `filesystem` (optional, default `read-write`): `read-only` | `read-write`
  - `shell` (optional, default `allow`): `allow` | `deny`
  - `network` (optional, default `allow`): `allow` | `deny`
  - `tools` (optional, default `["*"]`): list of tool names or `*`
- `enabled` (optional, default `true`): disable a role without deleting it.
- `timeout_secs` (optional): Override timeout in seconds for this role.

Note: `roles.<id>.fallback_models` is **not supported** and will error on load.

Example persona override:

```json
{
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2@xhigh",
      "personas": {
        "description": "Architecture decisions with long-term tradeoffs.",
        "prompt": "You are Oracle. Focus on architecture, tradeoffs, and long-term risks."
      }
    }
  }
}
```

`capabilities` are semantic and are mapped to CLI flags in `adapter.args_template`.
Adapters may optionally declare `filesystem_capabilities` to enforce supported values.
If the list exists and a role requests a filesystem capability not in the list,
`resolve_profile` fails for that role (config load still succeeds).

## Parsing rules: unknown fields and defaults

### Top-level

- Only `backend` and `roles` are recognized.
- Other top-level keys cause a validation error.

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

## MCP tool parameter behavior (three / batch / roundtable)

This section documents how MCP tools interpret runtime parameters.

### Session key

- If `session_key` is provided, it is used verbatim for persistence/locking.
- Otherwise, the key is derived as `hash(repo_root + role + role_id + client + conversation_id)`.
  - `client` comes from MCP `client` param (or `THREE_CLIENT`).
  - `conversation_id` comes from MCP `conversation_id` param (or `THREE_CONVERSATION_ID`).
  - If `conversation_id` is missing, auto-resume may cross top-level chats that share repo+role (a warning is returned).

### Conversation scoping

- `three`, `batch`, and `roundtable` all accept `conversation_id` (optional).
- Use the same `conversation_id` across calls in one main CLI chat to keep child-session reuse isolated.
- `batch` and `roundtable` forward `conversation_id` to each fan-out task.

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

## Roundtable behavior

- `roundtable` fan-outs participant prompts and returns per-participant contributions only.
- There is no MCP-side `moderator` role parameter.
- Multi-round synthesis is the conductor/main-CLI responsibility (plugin or skill workflow).
- `batch` and `roundtable` emit MCP logging notifications during fan-out by default (`started` / `completed role`).
  Clients that render `notifications/message` can show real-time completion progress.

## Role → CLI mapping (summary)

The only per-role inputs that can reach a CLI are:

- `model` (backend/model@variant) → becomes the backend `model` string.
- `personas` → used to build the final prompt (system + persona + user task). If omitted, the built-in persona is used when available.
- `capabilities` → passed to adapter as `capabilities.*` for flag mapping.
- `options` / `variants` → merged into `options` and exposed to adapter.
- `timeout_secs` → used by the server to enforce backend timeout.
- `backend.<id>.fallback` → used by the server for model fallback on model-not-found errors.

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
