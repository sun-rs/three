---
description: Research and evidence gathering via three MCP
---

# /three:researcher

Use this for codebase evidence, doc lookups, and grounded answers.

## Steps

1. Take the text after the command as the task prompt.

2. Call the MCP tool `mcp__three__info` with (skip if you already validated roles in this thread via `/three:conductor`):
   - `cd`: `.`

   If the role `researcher` is missing or `enabled=false`, stop and explain:
   - the role is missing in `~/.config/three/config.json`
   - list available roles
   - suggest either adding a `researcher` role or choosing a different role and re-running

3. Call the MCP tool `mcp__three__three` with:
   - `PROMPT`: the user's task prompt
   - `cd`: `.`
   - `role`: `researcher`

4. Return the result to the user. If `success=false`, explain the error and suggest a retry with `force_new_session=true`.
