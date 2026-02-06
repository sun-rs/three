---
name: three-routing
description: Routing rules for when to use three conductor, role skills, batch, and roundtable in Codex
---

# three-routing

Use this skill to decide delegation strategy before calling three MCP tools.

## Rules

1. Prefer direct local execution for trivial single-file tasks.
2. Use `$three-oracle` for architecture and high-risk tradeoffs.
3. Use `$three-builder` for implementation and bug fixes.
4. Use `$three-researcher` when evidence gathering is the bottleneck.
5. Use `$three-reviewer` for strict quality gates.
6. Use `$three-critic` for adversarial risk checks.
7. Use `$three-sprinter` for rapid option generation.
8. Use `$three-conductor` for multi-role orchestration.
9. Use `$three-roundtable` for complex decisions requiring 1-3 feedback rounds.

## MCP call standards

- Always include `client: "codex"` in `mcp__three__three`, `mcp__three__batch`, `mcp__three__roundtable`, and `mcp__three__info` calls.
- Only call roles returned by `mcp__three__info` where `enabled=true`.
- Pass `conversation_id` when the host can provide a stable main-chat id.
- For patch-producing tasks, require:
  - `contract`: `patch_with_citations`
  - `validate_patch`: `true`
- If using multiple kimi roles in parallel resume mode, set `force_new_session=true` or reduce to one kimi role.
