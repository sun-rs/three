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

## Steps

1. Call `mcp__three__info` with:
   - `cd`: `.`
   - `client`: `"claude"`

   Use this to list enabled roles and confirm availability.
   Treat this list as the source of truth; do not call roles that are not enabled.

2. Choose a delegation pattern:
   - **Single expert**: call `mcp__three__three` with `role=<enabled-role>` and `client="claude"`
   - **Parallel tasks**: call `mcp__three__batch` for independent work items and include `client="claude"`
   - If available, pass `conversation_id` to keep session reuse scoped to the current main chat
   - **Multi-role discussion**: use `/three:roundtable` **only when** the task is complex, ambiguous, or has major tradeoffs.

3. If delegating to `builder` for code changes, enforce:
   - `contract`: `patch_with_citations`
   - `validate_patch`: `true`

4. Collect outputs and synthesize:
   - highlight consensus and disagreements
   - if any batch tasks fail, report partial success and list failures
   - provide a clear next action

## Tips

- Prefer `oracle` for architecture tradeoffs.
- Prefer `builder` for implementation plans and fixes.
- Use `researcher` to ground decisions with evidence.
- Use `reviewer` or `critic` to stress-test proposals.
- If multiple Kimi roles are involved, use `force_new_session=true` or avoid parallel resume.
