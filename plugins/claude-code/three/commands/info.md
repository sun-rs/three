---
description: Troubleshooting view of role->model mapping (no LLM calls)
---

# /three:info

Troubleshooting command. Most role commands call `mcp__three__info` internally.

Shows which backend/model/effort/policy each `three` role uses.

This command calls `mcp__three__info` which only reads config (no codex/gemini).
Persona previews come from built-in defaults unless overridden in config.

## Steps

1. Call the MCP tool `mcp__three__info` with:
   - `cd`: `.`

2. Present a compact table with:
   - role
   - description
   - backend
   - model
   - reasoning_effort
   - codex sandbox (if codex)
   - codex skip_git_repo_check
   - timeout_secs
   - prompt_present + prompt_preview
   - warnings (if any)
