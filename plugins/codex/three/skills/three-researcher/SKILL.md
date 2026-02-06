---
name: three-researcher
description: Use Researcher for evidence from codebase/docs/web with concrete references
---

# three-researcher

## Steps

1. Read user task.

2. Validate role availability with `mcp__three__info`:
   - `cd`: `.`
   - `client`: `"codex"`

3. If `researcher` is missing or disabled, stop and report available roles.

4. Call `mcp__three__three`:
   - `PROMPT`: user task
   - `cd`: `.`
   - `role`: `researcher`
   - `client`: `"codex"`

5. Return result with key citations and references.
