# Three

[![English](https://img.shields.io/badge/lang-English-lightgrey)](README.md)
[![中文](https://img.shields.io/badge/语言-中文-blue)](README.zh-CN.md)

面向 Codex、Gemini、Claude 的多智能体、多大模型 Vibe‑Coding CLI 系统（MCP server + plugins）。

你可以获得：
- 角色化代理 + 会话复用 + 安全能力映射
- 并行分发与圆桌共识，交叉验证并更快收敛
- 更低的提示词开销与可审计输出

通过 `/three:conductor` 将复杂指令拆分并分派给多个 LLM，通过 `/three:roundtable` 发起多轮讨论以汇聚复杂方案与决策。

## 目录结构

- `mcp-server-three/` — MCP Server（Rust）。负责将请求路由到配置的后端，并进行会话复用。
- `plugins/claude-code/three/` — Claude Code 插件（斜杠命令 + 路由技能）。

## 文档索引

- `docs/cli-output-modes.md` — 输出/流式解析规则（权威入口）
- `docs/cli-*.md` — 各 CLI 的参数映射、会话续接和特性说明
- `docs/config-schema.md` — 配置字段、默认值与解析规则

## CLI 适配矩阵

说明：`examples/config.json` 是“技术配置-only”模板（不包含 persona 覆盖）。
内置 persona 可通过 `roles.<id>.personas` 覆盖（最小示例见 `docs/config-schema.md`）。
Personas 内置于 MCP Server；`roles.<id>.personas` 为可选覆盖项。

所有后端都由内置的 adapter catalog 驱动（MiniJinja `args_template` + `output_parser`）。  
模型写法为 `backend/model@variant`（variant 可选）。variant 会覆盖 `options`，
但只有当 adapter 将这些 options 映射成 CLI 参数时才会生效。若 adapter 声明了
`filesystem_capabilities`，不支持的值会在 **按 role 解析**时失败。

| Backend（CLI） | 可控读写能力 | 配置 → CLI 参数 | 模型 ID 命名 | options/variants 映射 | 输出解析与会话 |
|---|---|---|---|---|---|
| codex | read-only, read-write | `--sandbox read-only` / `--sandbox workspace-write` | `codex/<model>@variant` | 映射为 `-c key=value`（variant 最终变成 `-c`），如 `model_reasoning_effort`、`text_verbosity` | `json_stream`（`thread_id`, `item.text`），支持 session |
| claude | read-only, read-write | `--permission-mode plan` / `--dangerously-skip-permissions` | `claude/<model>@variant` | 默认不映射（需扩展 adapter） | `json_object`（`session_id`, `result`），支持 session |
| gemini | read-only, read-write | `--approval-mode plan` + `--sandbox` / `-y` | `gemini/<model>@variant` | 默认不映射（需扩展 adapter） | `json_object`（`session_id`, `response`），支持 session |
| opencode | 仅 read-write | 无 read-only 参数（read-only 会被拒绝） | `opencode/<provider>/<model>@variant` | 默认不映射（需扩展 adapter） | `json_stream`（`part.sessionID`, `part.text`），支持 session |
| kimi | 仅 read-write | 无 read-only 参数（read-only 会被拒绝） | `kimi/<model>@variant` | 默认不映射（需扩展 adapter） | `text`（stateless），不支持 session id |

Adapter 说明：
- adapter catalog 内置于服务器（不再使用 `adapter.json` 配置文件）。
- `args_template` 是 token 列表，空 token 会被丢弃。
- `include_directories` 会从 prompt 中的绝对路径自动推导（Gemini）。
- 所有内置适配器默认启用 `prompt_transport=auto`：长 prompt 会改用 `stdin` 传递（不混用 argv+stdin）。
- `json_stream` 可选启用 `fallback=codex`，用于在缺失 `message_path` 时回退解析。
- `backend.<id>.fallback` 可在模型不存在时自动回退（可跨 backend）。

## 快速开始

1) 构建 MCP Server：

```bash
cd mcp-server-three
cargo build --release
```

注意：编译产物为 `target/release/mcp-server-three`，注册的 MCP server 名称仍可使用 `three`。

2) 在 Claude Code 注册 MCP Server：

```bash
claude mcp add three -s user --transport stdio -- \
  "$(pwd)/target/release/mcp-server-three"
```

3) 安装 Claude Code 插件：

```bash
claude plugin marketplace add "./plugins/claude-code"
claude plugin install three@three-local
```

4) 使用插件命令：
- `/three:conductor <task>` 进行任务编排
- `/three:roundtable <topic>` 进行多角色讨论
- `/three:oracle|builder|researcher|reviewer|critic|sprinter <task>` 直接调用专业角色

并行扇出：使用 `mcp__three__batch` 在一次 MCP 调用中并行执行多个独立任务（即使部分失败也会返回结果）。

## 说明

- MCP Server 是宿主无关的，只要 CLI 支持 MCP 就能使用。
- 插件是宿主特定的，建议新增在 `plugins/<cli>/`。
