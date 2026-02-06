---
name: three-routing
description: Routing rules for multi-LLM vibe coding with the three MCP server
---

# three-routing

This skill provides orchestration rules. Use it automatically when the user asks for analysis/search modes, multi-agent debate, or requests an "oracle/builder" style workflow.

## Shared Config

- User config: `~/.config/three/config.json`
- Project override: `./.three/config.json` (preferred) or `./.three.json`

Roles and permissions live in config (model, timeout, capabilities). Always prefer passing `role` to `mcp__three__three` so the MCP server can:

- pick the correct role profile (backend/model/effort)
- apply role policy (codex sandbox + approval)
- inject the built-in persona prompt (config personas can override but are optional)

Use `mcp__three__info` to see which roles are enabled and their summaries.
Only call roles where `enabled=true`; do not assume preset roles are always present.
For Claude Code calls, always include `client: "claude"` in MCP tool parameters.
If available, pass `conversation_id` so session reuse stays scoped to the current main chat.

## Conductor (you)

You are the Conductor: orchestrate tasks, delegate to roles, and synthesize the final output.
If you need the current role list, call `mcp__three__info`.
If the user wants explicit orchestration, suggest `/three:conductor`.

## Role summaries (short)

- oracle: architecture, tech choices, long-term tradeoffs
- builder: implementation, debugging, practical feasibility
- researcher: evidence in code/docs/web with citations
- reviewer: adversarial review for correctness and risk
- critic: contrarian risk analysis and failure modes
- sprinter: fast ideation, quick options, not exhaustive

## Mode Triggers

If the user message contains:

- `[analyze-mode]`: gather context first (read key files, summarize, propose plan). If multiple competing designs exist, use `/three:roundtable`.
- `[search-mode]`: prioritize breadth and evidence. Use repo search tools and, when large context is needed, call `mcp__three__three` with `role=researcher`.

## Delegation Gate (avoid over-delegation)

- If the task is simple and local (single file, clear fix), do it directly.
- If multi-file / risky / tradeoffs, consult `oracle`.
- If you need implementation depth, consult `builder`.
- If you need multiple perspectives, run a roundtable.
- If you need parallel, independent work items, use `mcp__three__batch`.
- If you need quick options, consult `sprinter`.
- If you need evidence or docs, consult `researcher`.
- If you need contrarian risk checks, consult `critic`.

## Output Discipline

- For implementation/review: require `PATCH + CITATIONS`. Use the MCP contract gate (`contract=patch_with_citations`, `validate_patch=true`).
- For `batch`, report partial failures and continue synthesizing results.
- Do not claim tests pass unless you ran them.

## Kimi Parallel Resume

- Kimi has no session id. Parallel *resuming* across multiple Kimi roles in the same repo is rejected.
- Use a single Kimi role or set `force_new_session=true` for all Kimi tasks in `batch`/`roundtable`.

## Claude Code UI Note

When a tool call runs (especially long-running codex/gemini calls), Claude Code may keep focus in the tool panel. If you don't see the assistant message yet, press `Esc` to return to the chat view.
