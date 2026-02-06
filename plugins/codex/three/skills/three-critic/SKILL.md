---
name: three-critic
description: Use Critic to challenge assumptions, expose failure modes, and stress-test plans
---

# three-critic

## Steps

1. Read user task.

2. Validate role availability with `mcp__three__info`:
   - `cd`: `.`
   - `client`: `"codex"`

3. If `critic` is missing or disabled, stop and report available roles.

4. Call `mcp__three__three`:
   - `PROMPT`: user task
   - `cd`: `.`
   - `role`: `critic`
   - `client`: `"codex"`

5. Return contrarian risks and concrete safeguards.
