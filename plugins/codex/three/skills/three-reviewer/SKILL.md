---
name: three-reviewer
description: Use Reviewer for adversarial code review with patch and citations
---

# three-reviewer

## Steps

1. Read review request.

2. Validate role availability with `mcp__three__info`:
   - `cd`: `.`
   - `client`: `"codex"`

3. If `reviewer` is missing or disabled, stop and report available roles.

4. Call `mcp__three__three`:
   - `PROMPT`: review request
   - `cd`: `.`
   - `role`: `reviewer`
   - `client`: `"codex"`
   - `contract`: `patch_with_citations`
   - `validate_patch`: `true`
   - `timeout_secs`: `180` (optional)

5. Present findings first, then patch output.
