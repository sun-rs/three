# Roles Notes (examples)

This file explains role-related details for `examples/config.json`.

## Intentional validation examples

The example config includes two reader roles that are **intentionally labeled** as
validation examples:

- `kimi_reader`
- `opencode_reader`

These backends do **not** support `read-only` filesystem capability. In the example
config they are already set to `filesystem = "read-only"` so you can verify that MCP
capability-range validation behaves as expected. The server should reject those roles
during profile resolution.

To use the config without the validation error, either remove these roles or change:

```
capabilities.filesystem = "read-write"
```

## Persona override example

The `oracle` role includes a `personas` block to demonstrate how config can override
the built-in persona prompt. All other roles rely on the MCP server defaults.
