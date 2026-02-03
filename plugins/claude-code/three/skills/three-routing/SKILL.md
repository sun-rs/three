---
name: three-routing
description: Routing rules for multi-LLM vibe coding with the three MCP server
---

# three-routing

This skill provides orchestration rules. Use it automatically when the user asks for analysis/search modes, multi-agent debate, or requests an "oracle/sisyphus" style workflow.

## Shared Config

- User config: `~/.config/three/config.json`
- Project override: `./.three/config.json` (preferred) or `./.three.json`

Roles and permissions live in config. Always prefer passing `role` to `mcp__three__three` so the MCP server can:

- pick the correct brain profile (backend/model/effort)
- apply role policy (codex sandbox + approval)
- inject persona prompt when configured

## Mode Triggers

If the user message contains:

- `[analyze-mode]`: gather context first (read key files, summarize, propose plan). If multiple competing designs exist, use `/three:roundtable`.
- `[search-mode]`: prioritize breadth and evidence. Use repo search tools and, when large context is needed, call `mcp__three__three` with `role=reader`.

## Delegation Gate (avoid over-delegation)

- If the task is simple and local (single file, clear fix), do it directly.
- If multi-file / risky / tradeoffs, consult `oracle`.
- If you need multiple perspectives, run a roundtable.

## Output Discipline

- For implementation/review: require `PATCH + CITATIONS`. Use the MCP contract gate (`contract=patch_with_citations`, `validate_patch=true`).
- Do not claim tests pass unless you ran them.

## Claude Code UI Note

When a tool call runs (especially long-running codex/gemini calls), Claude Code may keep focus in the tool panel. If you don't see the assistant message yet, press `Esc` to return to the chat view.
