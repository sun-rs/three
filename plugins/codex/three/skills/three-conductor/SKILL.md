---
name: three-conductor
description: Orchestrate multiple three roles, delegate work, and synthesize a single answer
---

# three-conductor

Use this when a task needs orchestration across multiple specialist roles.

## Your role

You are the Conductor. You:
- break down the task
- choose which roles to consult
- gather responses
- synthesize one coherent output
- if running a roundtable, drive 1-3 rounds and feed disagreements back to every participant

Do not include persona text yourself. The MCP server injects built-in personas.

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

- If user asks the same question to multiple/all roles, do **one** `mcp__three__batch` call.
- Do **not** loop serial `mcp__three__three` calls for fan-out.
- Default session behavior is **continue memory** (`force_new_session=false`).
- Set `force_new_session=true` only when user explicitly asks reset/new clean session.
- If user asks recall/continue/follow-up, keep `force_new_session=false`.

## Steps

1. Call `mcp__three__info` with:
   - `cd`: `.`
   - `client`: `"codex"`

2. Treat `info.roles` (`enabled=true`) as the only callable role set.

3. Choose a delegation pattern:
   - Single expert: call `mcp__three__three` with `role=<enabled-role>` and `client="codex"`
   - Same question to all/many roles: call one `mcp__three__batch`
   - Parallel independent tasks: call `mcp__three__batch`
   - Complex ambiguous tradeoffs with cross-feedback: use `$three-roundtable`
   - Pass `conversation_id` when host can provide a stable main-chat id

4. Batch fan-out template for "ask all members":

```json
{
  "cd": ".",
  "client": "codex",
  "tasks": [
    {"name": "oracle", "role": "oracle", "PROMPT": "<same question>", "force_new_session": false},
    {"name": "builder", "role": "builder", "PROMPT": "<same question>", "force_new_session": false},
    {"name": "researcher", "role": "researcher", "PROMPT": "<same question>", "force_new_session": false}
  ]
}
```

Only include roles that are enabled in `info.roles`.

5. If delegating code changes to `builder`, enforce:
   - `contract`: `patch_with_citations`
   - `validate_patch`: `true`

6. Synthesize outputs:
   - show consensus and disagreements
   - report partial failures if any batch task fails
   - propose clear next actions

7. Only invoke `$three-roundtable` when user asks for it or the task clearly needs multi-round debate.
