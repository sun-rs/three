# Roundtable

[![English](https://img.shields.io/badge/lang-English-lightgrey)](README.md)
[![中文](https://img.shields.io/badge/语言-中文-blue)](README.zh-CN.md)

> **Roundtable-first multi-agent orchestration for serious software work**

This repository now has two distinct architecture tracks under one codebase:

## Architecture Tracks

### A) OpenCode Native Plugin Track (stateful, UI-first)

- Runtime: OpenCode + oh-my-opencode plugin runtime
- Core command: `/roundtable`
- Orchestration path: `task(...) + background_output(...)`
- Strength: clickable/traceable child sessions in TUI, strong sub-session continuity
- Focus: roundtable debate/synthesis as the primary workflow

### B) MCP + Prompt Engineering Track (portable, host-agnostic)

- Runtime: `mcp-server-roundtable` + host-specific text plugins/skills (Claude/Codex)
- Claude/Codex entrypoints are `/roundtable:*` and `roundtable-*` skills
- Core MCP tools: `roundtable`, `roundtable-batch`, `info`
- Strength: works across MCP-capable hosts, flexible parallel fan-out, explicit role control
- Focus: portable orchestration where host-native agent/task APIs are unavailable

## Why the split?

Because these two systems optimize different constraints:

| Dimension | OpenCode native track | MCP + prompt track |
|---|---|---|
| Orchestration substrate | Host-native task engine | MCP tool fan-out |
| Session continuity | Native child sessions, UI-visible | Session-store + backend resume |
| Observability | Clickable background tasks | MCP structured outputs/logs |
| Role source | OpenCode/oh-my-opencode agent catalog | `~/.config/roundtable/config*.json` roles |
| Best use case | Deep roundtable discussions | Cross-host portability + scripted fan-out |

## Roundtable-first design

The project is now explicitly **roundtable-first**:

- Roundtable is the core capability and primary product direction.
- Roundtable-batch is a secondary capability mainly for independent fan-out workloads.
- On MCP track, independent fan-out is exposed only as `roundtable-batch`.

## Repo layout

- `mcp-server-roundtable/` — MCP server (Rust). Routes prompts to configured backends with session reuse.
- `plugins/claude-code/roundtable/` — Claude Code plugin (slash commands, `/roundtable:*`).
- `plugins/codex/roundtable/` — Codex skills (`roundtable-*`).
- `plugins/opencode/roundtable/` — OpenCode native plugin (`/roundtable`, native task orchestration).

## OpenCode track quick start

Install local plugin:

```bash
mkdir -p ~/.config/opencode/plugins
ln -sf "$(pwd)/plugins/opencode/roundtable/index.js" \
  ~/.config/opencode/plugins/roundtable-opencode.js
```

Restart OpenCode, then use:

- `/roundtable` — hard-routed prompt contract to `task(...) + background_output(...)`

Policy highlights:

- Participant turns must use `subagent_type` (not `category`).
- Round 2+ must continue previous participant sessions.
- `roundtable_native_roundtable` is soft-locked during `/roundtable` unless `allow_native=true` is explicitly set.

## MCP track quick start

1) Build MCP server:

```bash
cd mcp-server-roundtable
cargo build --release
```

2) Register server in Claude Code:

```bash
claude mcp add roundtable -s user --transport stdio -- \
  "$(pwd)/target/release/mcp-server-roundtable"
```

3) Install Claude plugin:

```bash
claude plugin marketplace add "./plugins/claude-code"
claude plugin install roundtable@roundtable-local
```

4) Use plugin commands (`/roundtable:*`) and MCP tools:

- `/roundtable:conductor <task>`
- `/roundtable:roundtable <topic>`
- `mcp__roundtable__roundtable`
- `mcp__roundtable__roundtable_batch`
- `mcp__roundtable__info`

## Docs index

- `docs/cli-output-modes.md` — authoritative output/stream parsing rules (start here)
- `docs/cli-*.md` — per-CLI flag mapping, session resume, and CLI-specific notes
- `docs/config-schema.md` — config fields, defaults, and role resolution rules

Client-specific configs: `config-<client>.json` is preferred when MCP `client` (or `ROUNDTABLE_CLIENT`) is set.

Note: `examples/config.json` is a technical-only template (no persona overrides).
Personas are built into the MCP server; `roles.<id>.personas` is optional and overrides built-ins.
For Codex-hosted workflows, see `examples/config-codex.json`.

## CLI adapter matrix (MCP track)

All backends are driven by the embedded adapter catalog (MiniJinja `args_template` + `output_parser`).
Models are referenced as `backend/model@variant` (variant optional). Variants override `options`,
and only take effect if the adapter maps those options into CLI flags.

| Backend (CLI) | Filesystem capabilities | Config -> CLI flags | Model id naming | Options/variants mapping | Output parser & session |
|---|---|---|---|---|---|
| codex | read-only, read-write | `--sandbox read-only` / `--sandbox workspace-write` | `codex/<model>@variant` | mapped to `-c key=value` (variants become `-c`) | `json_stream`, session supported |
| claude | read-only, read-write | `--permission-mode plan` / `--dangerously-skip-permissions` | `claude/<model>@variant` | not mapped by default | `json_object`, session supported |
| gemini | read-only, read-write | `--approval-mode plan` + `--sandbox` / `-y` | `gemini/<model>@variant` | not mapped by default | `json_object`, session supported |
| opencode | read-write only | no read-only flag | `opencode/<provider>/<model>@variant` | not mapped by default | `json_stream`, session supported |
| kimi | read-write only | no read-only flag | `kimi/<model>@variant` | not mapped by default | `text` (stateless), no session id |

Adapter notes:

- Adapter catalog is embedded in server (no `adapter.json`).
- `args_template` is a token list; empty tokens are dropped.
- `include_directories` auto-derives from absolute paths in prompt (Gemini).
- Embedded adapters default to `prompt_transport=auto`.
- `json_stream` supports optional fallback parsing (`fallback=codex`) when `message_path` is missing.
- Backends may define `fallback` to retry model-not-found errors.

## Notes

- The MCP server is host-agnostic; any MCP-capable CLI can use it.
- Plugins/skills are host-specific by design.
- Directory names and plugin IDs are unified as `roundtable`; branding is Roundtable-first.
