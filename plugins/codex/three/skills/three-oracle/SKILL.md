---
name: three-oracle
description: Consult Oracle for architecture tradeoffs, deep debugging, and high-risk decisions
---

# three-oracle

## Steps

1. Read user task.

2. Validate role availability with `mcp__three__info`:
   - `cd`: `.`
   - `client`: `"codex"`

3. If `oracle` is missing or disabled, stop and report available roles.

4. Call `mcp__three__three`:
   - `PROMPT`: user task
   - `cd`: `.`
   - `role`: `oracle`
   - `client`: `"codex"`
   - `timeout_secs`: `900` (optional)

5. If `success=false`, explain the error and offer retry (`force_new_session=true` if reset is needed).
