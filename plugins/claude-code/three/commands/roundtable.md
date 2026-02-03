---
description: Run a multi-brain roundtable and synthesize a decision
---

# /three:roundtable

Use this when the question is ambiguous, multi-tradeoff, or benefits from multiple "souls".

## Steps

1. Take the text after the command as `TOPIC`.

2. Call the MCP tool `mcp__three__roundtable` with:
   - `TOPIC`: the user's topic
   - `cd`: `.`
   - `timeout_secs`: `300` (optional; per-participant default)
   - `participants`: at minimum:
     - `{ "name": "Oracle", "role": "oracle" }`
     - `{ "name": "Sisyphus", "role": "sisyphus" }`
     - `{ "name": "Reader", "role": "reader" }`
   - `moderator`: `{ "role": "moderator" }`

3. Present:
   - `synthesis` (if present)
   - notable disagreements
   - next actions
