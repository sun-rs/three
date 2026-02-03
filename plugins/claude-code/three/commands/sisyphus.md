---
description: Implementation pass (PATCH + CITATIONS) via three MCP
---

# /three:sisyphus

Use this for Sisyphus-style execution.

Behavior:

- If the request is clearly about making a code change, enforce `PATCH + CITATIONS` and validate with `git apply --check`.
- If the request is informational (e.g. "what model are you?", "explain this module"), do NOT require a patch.

## Steps

1. Take the text after the command as the task prompt.

2. Decide whether this is a code-change request.

   Treat as code-change if the user asks to: implement, fix, refactor, rename, add, remove, update, change files, or provides a diff/stacktrace and asks for a fix.

3. Call the MCP tool `mcp__three__three` with:

   Always:
   - `PROMPT`: the user's task prompt
   - `cd`: `.`
   - `role`: `sisyphus`

   If code-change:
   - `contract`: `patch_with_citations`
   - `validate_patch`: `true`

3. If the tool returns `success=false`, do NOT guess. Ask for clarification or rerun with a narrower scope.
