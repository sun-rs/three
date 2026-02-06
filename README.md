# Three

[![English](https://img.shields.io/badge/lang-English-lightgrey)](README.md)
[![中文](https://img.shields.io/badge/语言-中文-blue)](README.zh-CN.md)

> **Multi-agent, multi-LLM orchestration system for complex software tasks**

Three is a multi-agent, multi-LLM vibe-coding CLI system (MCP server + plugins) for Codex, Gemini, and Claude.

It helps you run serious engineering workflows with less prompt overhead:
- Use `/three:conductor` to break down complex work and dispatch it across multiple specialist roles.
- Use `/three:roundtable` to run 1-3 feedback rounds for hard tradeoffs, then synthesize a decision in the main CLI.
- Use `mcp__three__batch` to parallelize independent tasks and still get partial results if some tasks fail.
- Keep child-session reuse scoped by `client` + `conversation_id` (when provided), reducing cross-chat contamination.

## Why Three?

Effective engineering requires multiple perspectives:

- **🔮 Oracle** — Architecture, trade-offs, long-term risks
- **🔨 Builder** — Implementation feasibility, correct execution
- **🔍 Researcher** — Codebase and documentation grounding
- **✅ Reviewer** — Quality and correctness checks
- **⚡ Critic** — Contrarian risk analysis
- **🚀 Sprinter** — Fast idea generation

## Key Features

### 🎯 Role-Based Agents
Specialized agents with session-aware reuse and safe capability controls (filesystem, shell, network, tools).

### 🔄 Cross-Model Validation
Split complex tasks across multiple LLMs, cross-check results, and converge faster with less prompt overhead.

### 🤝 Roundtable Consensus
Run multi-round discussions where different models debate and synthesize decisions for tough architectural choices.

### ⚡ Parallel Fan-Out
Execute independent tasks concurrently with partial failure handling and real-time role completion logs.

## Quick Commands

- **`/three:conductor <task>`** — Orchestrate complex work and decide when to use roundtable
- **`/three:roundtable <topic>`** — Multi-role consensus workflow (1-3 rounds)
- **`/three:oracle <task>`** — Architecture and trade-off analysis
- **`/three:builder <task>`** — Implementation and debugging
- **`/three:reviewer <request>`** — Code review and risk finding

## Repo layout

- `mcp-server-three/` — MCP server (Rust). Routes prompts to configured backends with session reuse.
- `plugins/claude-code/three/` — Claude Code plugin (slash commands + routing skill).

## Docs index

- `docs/cli-output-modes.md` — authoritative output/stream parsing rules (start here)
- `docs/cli-*.md` — per-CLI flag mapping, session resume, and CLI-specific notes
- `docs/config-schema.md` — config fields, defaults, and role resolution rules

Client-specific configs: `config-<client>.json` is preferred when the MCP `client` param (or `THREE_CLIENT`) is set.

Note: `examples/config.json` is a technical-only template (no persona overrides).
Personas are built into the MCP server; `roles.<id>.personas` is optional and overrides the built-in persona for that role
(see `docs/config-schema.md` for a minimal override example).
For Codex-hosted workflows, see `examples/config-codex.json` (roles avoid self-calling Codex by default).

## CLI adapter matrix

All backends are driven by the embedded adapter catalog (MiniJinja `args_template` + `output_parser`).
Models are referenced as `backend/model@variant` (variant optional). Variants override `options`,
and only take effect if the adapter maps those options into CLI flags. If an adapter declares
`filesystem_capabilities`, unsupported values fail **per role** during `resolve_profile`.

| Backend (CLI) | Filesystem capabilities | Config → CLI flags | Model id naming | Options/variants mapping | Output parser & session |
|---|---|---|---|---|---|
| codex | read-only, read-write | `--sandbox read-only` / `--sandbox workspace-write` | `codex/<model>@variant` | Mapped to `-c key=value` (variants become `-c`), e.g. `model_reasoning_effort`, `text_verbosity` | `json_stream` (`thread_id`, `item.text`), session supported |
| claude | read-only, read-write | `--permission-mode plan` / `--dangerously-skip-permissions` | `claude/<model>@variant` | Not mapped by default (extend adapter) | `json_object` (`session_id`, `result`), session supported |
| gemini | read-only, read-write | `--approval-mode plan` + `--sandbox` / `-y` | `gemini/<model>@variant` | Not mapped by default (extend adapter) | `json_object` (`session_id`, `response`), session supported |
| opencode | read-write only | no read-only flag (read-only rejected) | `opencode/<provider>/<model>@variant` | Not mapped by default (extend adapter) | `json_stream` (`part.sessionID`, `part.text`), session supported |
| kimi | read-write only | no read-only flag (read-only rejected) | `kimi/<model>@variant` | Not mapped by default (extend adapter) | `text` (stateless), no session id |

Adapter notes:
- The adapter catalog is embedded in the server (no `adapter.json` config file).
- `args_template` is a list of tokens; empty tokens are dropped.
- `include_directories` is auto-derived from absolute paths in the prompt (Gemini).
- All embedded adapters default to `prompt_transport=auto`: long prompts are sent via `stdin` instead of argv (no mixed transport).
- `json_stream` supports optional fallback parsing (`fallback=codex`) when `message_path` is missing.
- Backends may define `fallback` to retry on model-not-found errors (can span backends).

## Quick start

1) Build the MCP server:

```bash
cd mcp-server-three
cargo build --release
```

Note: the compiled binary is `target/release/mcp-server-three`. The MCP server name you register can still be `three`.

2) Register the MCP server with Claude Code:

```bash
claude mcp add three -s user --transport stdio -- \
  "$(pwd)/target/release/mcp-server-three"
```

3) Install the Claude Code plugin:

```bash
claude plugin marketplace add "./plugins/claude-code"
claude plugin install three@three-local
```

4) Use the plugin commands:
- `/three:conductor <task>` for orchestration
- `/three:roundtable <topic>` for multi-agent consensus
- `/three:oracle|builder|researcher|reviewer|critic|sprinter <task>` for specialist roles

Parallel fan-out: use `mcp__three__batch` to run multiple independent tasks in one MCP call (partial failures are returned).

## Notes

- The MCP server is host-agnostic; any CLI that supports MCP can use it.
- Plugins are CLI-specific; add new ones under `plugins/<cli>/`.
