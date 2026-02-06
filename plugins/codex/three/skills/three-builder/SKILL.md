---
name: three-builder
description: Use Builder for implementation and bug fixing with optional patch contract enforcement
---

# three-builder

## Steps

1. Read user task.

2. Validate role availability with `mcp__three__info`:
   - `cd`: `.`
   - `client`: `"codex"`

3. If `builder` is missing or disabled, stop and report available roles.

4. Detect whether this is a code-change request.

5. Call `mcp__three__three`:
   - `PROMPT`: user task
   - `cd`: `.`
   - `role`: `builder`
   - `client`: `"codex"`

6. If code-change request, also set:
   - `contract`: `patch_with_citations`
   - `validate_patch`: `true`

7. If tool returns failure, do not guess. Ask for clarification or narrower scope.
