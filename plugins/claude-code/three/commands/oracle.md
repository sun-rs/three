---
description: Consult Oracle (deep reasoning) via three MCP
---

# /three:oracle

Use this for architecture tradeoffs, hard debugging, or high-risk decisions.

## Steps

1. Take the text after the command as the task prompt.

2. Call the MCP tool `mcp__three__three` with:
   - `PROMPT`: the user's task prompt
   - `cd`: `.`
   - `role`: `oracle`
   - `timeout_secs`: `900` (optional; prefer role config if set)
   - `force_new_session`: `true` only if the user explicitly asks to reset, or if the topic is clearly unrelated to the current thread.

3. Return the result to the user. If `success=false`, explain the error and suggest a retry with `force_new_session=true`.
