---
description: Review a change and propose fixes (PATCH + CITATIONS) via three MCP
---

# /three:review

Use this to get an adversarial review that focuses on regressions and correctness.

## Steps

1. Take the text after the command as the review prompt.

2. Call the MCP tool `mcp__three__three` with:
   - `PROMPT`: the user's prompt
   - `cd`: `.`
   - `role`: `reviewer`
   - `contract`: `patch_with_citations`
   - `validate_patch`: `true`
   - `timeout_secs`: `180` (optional; prefer role config if set)

3. Summarize findings, then include the patch output.
