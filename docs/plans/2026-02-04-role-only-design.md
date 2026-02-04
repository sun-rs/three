# Role-only config + embedded adapter catalog (design)

Date: 2026-02-04

## Summary

We will remove external adapter.json/server.json and embed the adapter catalog in the Rust MCP server. The public configuration will only expose roles (no brains). A role defines model, persona, capabilities, and options. Capabilities are validated against the embedded adapter capabilities; unsupported capabilities will fail the role before any tool execution. This makes the boundary between "server ability" and "user intent" explicit and testable.

## Goals

- Make adapter capabilities an internal, versioned server concern.
- Use roles as the single public entrypoint (remove brains terminology).
- Enforce capability compatibility per role before any execution.
- Preserve backend/model@variant model format with backend as first path segment.

## Non-goals

- Backward compatibility for brains or external adapter files.
- Auto-migration tooling.
- Expanding CLI feature coverage beyond current adapters.

## Architecture and boundaries

There are three layers:
1) Host plugin (e.g., Claude Code) invokes a role by name.
2) MCP server resolves role -> backend execution plan.
3) Backend CLI executes with adapter-rendered arguments.

The adapter catalog is embedded in Rust, not user-editable. The config.json exposes roles only. The server owns capability enforcement and command rendering. Plugins only pass the role name; they do not interpret config.json.

## Data flow and validation

1) Load config.json -> parse roles -> validate model format and role capabilities.
2) Merge with embedded adapter catalog -> validate role.capabilities subset of adapter capabilities.
3) On invocation, render adapter template and run backend CLI.

Validation is per role: if a role exceeds adapter capability (e.g., read-only on a read-write-only backend), the role fails and is unavailable, while other roles remain usable.

## Model identifiers

Model reference format is `backend/<model>@variant` where the first `/` segment is the backend id. Anything after the first `/` is considered part of the model name, so provider/model (opencode) remains valid. Variant is optional and defined by `@variant`. Whether a backend supports variants is enforced by its embedded adapter.

## Config schema changes

- Replace `brains` with `roles` at the top level.
- Keep role shape the same (model, persona, capabilities, options, etc.).
- Remove adapter path fields from config.

## Error handling

- Unknown role: error and no execution.
- Role capability exceeds adapter: role rejected on load; invocation returns a clear error.
- Backend errors: surfaced as runtime errors with adapter/role context.

## Testing plan

- Unit tests for role parsing and adapter capability validation.
- Update adapter render tests to use embedded catalog.
- E2E tests remain aligned to backend capabilities:
  - read-only + read-write for supporting backends
  - read-write only for non-supporting backends
  - add e2e for role capability rejection
  - keep include_directories multi-path tests

## Migration notes

Breaking change: users must rename `brains` to `roles` in config.json. External adapter files are removed. Documentation will include a short manual migration snippet.

## Risks

- Breaking change may surprise users; mitigate with clear docs and schema errors.
- Embedded adapter requires code updates for new CLI features; accepted trade-off for safety and clarity.
