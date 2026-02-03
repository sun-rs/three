---
description: Show effective three role->model mapping (no LLM calls)
---

# /three:info

Show which backend/model/effort/policy each `three` role uses.

This command calls `mcp__three__info` which only reads config (no codex/gemini).

## Steps

1. Call the MCP tool `mcp__three__info` with:
   - `cd`: `.`

2. Present a compact table with:
   - role
   - description
   - brain
   - backend
   - model
   - reasoning_effort
   - codex sandbox (if codex)
   - codex skip_git_repo_check
   - timeout_secs
   - prompt_present + prompt_preview
