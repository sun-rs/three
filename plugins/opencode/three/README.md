# roundtable-opencode (MVP, directory keeps `three`)

A true OpenCode plugin for roundtable-first orchestration using OpenCode's native plugin/runtime APIs.

## What it provides

- `three_native_roundtable` - run multi-round discussion (`rounds >= 1`) on one shared topic.
- Tool metadata restoration for native plugin tools (`sessionId/sessionIds`) so UIs can jump into child sessions when native tools are called directly.
- Built-in slash command:
  - `/roundtable`

Slash command routing policy:
- `/roundtable` is hard-routed by prompt contract to `task(...)` + `background_output(...)` for clickable, traceable child sessions in TUI.
- Commands must target explicit participants via `subagent_type`; they must not route via `category` or silently switch to `three_native_roundtable`.
- Command definitions are refreshed on each config load so updated routing rules replace stale templates from older plugin versions.
- During `/roundtable`, native tools are soft-locked for that parent session to prevent silent fallback; explicit override requires `allow_native=true`.
- Legacy command aliases (`three-batch`, `three_batch`, `three-roundtable`, `three_roundtable`, `three:*`) are removed from command config by this plugin.

This plugin is sisyphus-first: the main CLI agent remains `sisyphus` and delegates to OpenCode subagents.

## Install (local plugin)

Option A: project scope

```bash
mkdir -p .opencode/plugins
ln -sf "$(git rev-parse --show-toplevel)/plugins/opencode/three/index.js" .opencode/plugins/three-opencode.js
```

Option B: user scope

```bash
mkdir -p ~/.config/opencode/plugins
ln -sf "$(git rev-parse --show-toplevel)/plugins/opencode/three/index.js" ~/.config/opencode/plugins/three-opencode.js
```

Restart OpenCode after linking.

## Session behavior

- Parent key: current OpenCode `sessionID` (optionally `conversation_id` suffix).
- Child sessions are created with `parentID=<current sessionID>`.
- Roundtable emits child `sessionId/sessionIds` metadata on completion (and restores it after plugin truncation wrapper).
- Per-agent child session IDs persist at `<worktree>/.three/opencode-session-store.json`.
- `three_native_roundtable` rules:
  - Round 1 follows caller policy (`round1_force_new_session` or participant-level override).
  - Round 2+ force reuse (`force_new_session=false`) by design.
  - Round 2+ receives substantial peer viewpoints (rich excerpts, not one-line summaries).
  - By default, only round-1 successful participants continue to round 2+ (`round2_only_stage1_success=true`).
  - Each round uses stage timeout/min-success policy inspired by council flows (`round_stage_timeout_secs`, `round_stage_min_successes`).
  - Optional anonymized carryover (`round_anonymous_viewpoints=true`) for reduced role-bias in debate.
  - Round evidence persists to `.three/roundtable-artifacts/...` by default (`persist_round_artifacts=true`).
  - Context carryover can be tuned via `round_context_level`, `round_context_max_chars`, `per_agent_context_max_chars`.

## Notes

- This plugin is OpenCode-native and does not require the MCP track for orchestration.
- Agent names come from `client.app.agents()` (typically from OpenCode + oh-my-opencode).
- Primary agents (for example `sisyphus`) are not callable as roundtable participants.
