# Three

[![English](https://img.shields.io/badge/lang-English-lightgrey)](README.md)
[![中文](https://img.shields.io/badge/语言-中文-blue)](README.zh-CN.md)

Multi-agent, multi-LLM vibe-coding CLI system (MCP server + plugins) for Codex, Gemini, and Claude.

## Repo layout

- `mcp-server-three/` — MCP server (Rust). Routes prompts to configured backends with session reuse.
- `plugins/claude-code/three/` — Claude Code plugin (slash commands + routing skill).

## Docs index

- `docs/cli-output-modes.md` — authoritative output/stream parsing rules (start here)
- `docs/cli-*.md` — per-CLI flag mapping, session resume, and CLI-specific notes

Note: `examples/config.json` includes `kimi_reader` / `opencode_reader` counterexamples used to test capability validation.
See `docs/config-schema.md` for MCP parameter behavior details.

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

## Notes

- The MCP server is host-agnostic; any CLI that supports MCP can use it.
- Plugins are CLI-specific; add new ones under `plugins/<cli>/`.
