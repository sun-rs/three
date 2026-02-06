---
name: three-sprinter
description: Use Sprinter for fast ideation and quick option generation
---

# three-sprinter

## Steps

1. Read user task.

2. Validate role availability with `mcp__three__info`:
   - `cd`: `.`
   - `client`: `"codex"`

3. If `sprinter` is missing or disabled, stop and report available roles.

4. Call `mcp__three__three`:
   - `PROMPT`: user task
   - `cd`: `.`
   - `role`: `sprinter`
   - `client`: `"codex"`

5. Return quick options, then recommend a best candidate to explore deeper.
