---
description: Conductor mode (orchestrate roles via three MCP)
---

# /three:conductor

Use this when you need to orchestrate multiple roles, delegate work, and synthesize results.

## Your role

You are the Conductor. You:
- break down the task
- choose which roles to consult
- gather responses
- synthesize a single coherent answer
- if running a roundtable, drive 1-3 rounds and feed disagreements back to every participant

You do **not** need to include persona text. The MCP server injects built-in personas.

## Default role pool (only if enabled in config)

| Role | Summary |
| --- | --- |
| `oracle` | Architecture, tech choices, long-term tradeoffs. |
| `builder` | Implementation, debugging, practical feasibility. |
| `researcher` | Evidence in code/docs/web with citations. |
| `reviewer` | Adversarial review for correctness and risk. |
| `critic` | Contrarian risk analysis and failure modes. |
| `sprinter` | Fast ideation and quick options (not exhaustive). |

## Non-negotiable rules

- If user asks the **same question** to multiple/all roles, do **one** `mcp__three__batch` call.
- Do **not** loop serial `mcp__three__three` calls for fan-out.
- Default session behavior is **continue memory** (`force_new_session=false`).
- Set `force_new_session=true` only when user explicitly asks reset/new clean session.
- If user asks recall/continue/follow-up, keep `force_new_session=false`.

## Steps

1. Call `mcp__three__info` with:
   - `cd`: `.`
   - `client`: `"claude"`

   Use this to list enabled roles and confirm availability.
   Treat this list as the source of truth; do not call roles that are not enabled.

2. Choose a delegation pattern:
   - **Single expert**: call `mcp__three__three` with `role=<enabled-role>` and `client="claude"`
   - **Same question to all/many roles**: call **one** `mcp__three__batch`
   - **Parallel independent tasks**: call `mcp__three__batch`
   - If available, pass `conversation_id` to keep session reuse scoped to the current main chat
   - **Multi-role discussion**: use `/three:roundtable` only when the task is complex, ambiguous, or has major tradeoffs.

3. Batch fan-out template for "ask all members":

```json
{
  "cd": ".",
  "client": "claude",
  "tasks": [
    {"name": "oracle", "role": "oracle", "PROMPT": "<same question>", "force_new_session": false},
    {"name": "builder", "role": "builder", "PROMPT": "<same question>", "force_new_session": false},
    {"name": "researcher", "role": "researcher", "PROMPT": "<same question>", "force_new_session": false}
  ]
}
```

Only include roles that are enabled in `info.roles`.

4. If delegating to `builder` for code changes, enforce:
   - `contract`: `patch_with_citations`
   - `validate_patch`: `true`

5. Collect outputs and synthesize:
   - highlight consensus and disagreements
   - if any batch tasks fail, report partial success and list failures
   - provide a clear next action

## Tips

- Prefer `oracle` for architecture tradeoffs.
- Prefer `builder` for implementation plans and fixes.
- Use `researcher` to ground decisions with evidence.
- Use `reviewer` or `critic` to stress-test proposals.
- If multiple Kimi roles are involved, use `force_new_session=true` or avoid parallel resume.
