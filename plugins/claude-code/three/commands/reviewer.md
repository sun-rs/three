---
description: Review a change and propose fixes (PATCH + CITATIONS) via three MCP
---

# /three:reviewer

Use this to get an adversarial review that focuses on regressions and correctness.

## Steps

1. Take the text after the command as the review prompt.

2. Call the MCP tool `mcp__three__info` with (skip if you already validated roles in this thread via `/three:conductor`):
   - `cd`: `.`

   If the role `reviewer` is missing or `enabled=false`, stop and explain:
   - the role is missing in `~/.config/three/config.json`
   - list available roles
   - suggest either adding a `reviewer` role or choosing a different role and re-running

3. Call the MCP tool `mcp__three__three` with:
   - `PROMPT`: the user's prompt
   - `cd`: `.`
   - `role`: `reviewer`
   - `contract`: `patch_with_citations`
   - `validate_patch`: `true`
   - `timeout_secs`: `180` (optional; prefer role config if set)

4. Summarize findings, then include the patch output.
