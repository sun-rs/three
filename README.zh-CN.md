# Roundtable

[![English](https://img.shields.io/badge/lang-English-lightgrey)](README.md)
[![中文](https://img.shields.io/badge/语言-中文-blue)](README.zh-CN.md)

> **以圆桌协商为核心的多智能体工程编排系统**

当前仓库由两条架构路线组成（同仓共存）：

## 架构双轨

### A）OpenCode 原生插件路线（强状态、强可视化）

- 运行时：OpenCode + oh-my-opencode
- 核心命令：`/roundtable`
- 编排路径：`task(...) + background_output(...)`
- 优势：子会话可点击、可追踪，续接语义强
- 目标：把 roundtable 讨论/收敛作为主流程

### B）MCP + 提示词工程路线（跨宿主、可移植）

- 运行时：`mcp-server-roundtable` + Claude/Codex 文本插件/skills
- Claude/Codex 入口为 `/roundtable:*` 与 `roundtable-*`
- 核心 MCP 工具：`roundtable`、`roundtable-batch`、`info`
- 优势：可在各类 MCP 宿主复用，显式 role 路由灵活
- 目标：在缺少宿主原生多 agent 能力时提供统一编排

## 为什么拆成两条路线？

两条路线优化目标不同：

| 维度 | OpenCode 原生路线 | MCP + 提示词路线 |
|---|---|---|
| 编排底座 | 宿主原生 task 引擎 | MCP 工具 fan-out |
| 会话连续性 | 原生子会话，UI 可见 | 本地 session store + backend resume |
| 可观测性 | 后台任务可点击追踪 | MCP 结构化输出与日志 |
| 角色来源 | OpenCode/oh-my-opencode agent catalog | `~/.config/roundtable/config*.json` roles |
| 最适场景 | 深度 roundtable 讨论 | 跨宿主可移植协作 |

## Roundtable-first 设计

项目正式转为 **roundtable-first**：

- Roundtable 是核心能力与主产品方向。
- roundtable-batch 是次要能力，主要用于独立任务并行扇出。
- MCP 路线上，独立任务并行只保留 `roundtable-batch`。

## 仓库结构

- `mcp-server-roundtable/` — MCP Server（Rust），负责路由与会话复用。
- `plugins/claude-code/roundtable/` — Claude Code 插件（`/roundtable:*`）。
- `plugins/codex/roundtable/` — Codex skills（`roundtable-*`）。
- `plugins/opencode/roundtable/` — OpenCode 原生插件（`/roundtable`，原生 task 编排）。

## OpenCode 路线快速开始

安装本地插件：

```bash
mkdir -p ~/.config/opencode/plugins
ln -sf "$(pwd)/plugins/opencode/roundtable/index.js" \
  ~/.config/opencode/plugins/roundtable-opencode.js
```

重启 OpenCode 后使用：

- `/roundtable` — 按提示词契约硬路由到 `task(...) + background_output(...)`

策略重点：

- 参与者轮次必须用 `subagent_type`（不能走 `category`）。
- 第 2 轮起必须续接既有参与者 session。
- `/roundtable` 执行期间默认软锁 `roundtable_native_roundtable`，仅在显式 `allow_native=true` 时放行。

## MCP 路线快速开始

1）构建 MCP Server：

```bash
cd mcp-server-roundtable
cargo build --release
```

2）在 Claude Code 注册 MCP Server：

```bash
claude mcp add roundtable -s user --transport stdio -- \
  "$(pwd)/target/release/mcp-server-roundtable"
```

3）安装 Claude 插件：

```bash
claude plugin marketplace add "./plugins/claude-code"
claude plugin install roundtable@roundtable-local
```

4）使用 `/roundtable:*` 与 MCP 工具：

- `/roundtable:conductor <task>`
- `/roundtable:roundtable <topic>`
- `mcp__roundtable__roundtable`
- `mcp__roundtable__roundtable_batch`
- `mcp__roundtable__info`

## 文档索引

- `docs/cli-output-modes.md` — 输出/流式解析规则（权威入口）
- `docs/cli-*.md` — 各 CLI 参数映射、续接策略与特性说明
- `docs/config-schema.md` — 配置字段、默认值与 role 解析规则

当 MCP `client`（或 `ROUNDTABLE_CLIENT`）存在时，优先加载 `config-<client>.json`。

说明：`examples/config.json` 是技术模板（不含 persona 覆盖）；
persona 内置在 MCP Server，`roles.<id>.personas` 为可选覆盖项。

## CLI 适配矩阵（MCP 路线）

所有 backend 都由内置 adapter catalog 驱动（MiniJinja `args_template` + `output_parser`）。
模型写法为 `backend/model@variant`（variant 可选）。

| Backend（CLI） | 可控读写能力 | 配置 -> CLI 参数 | 模型 ID 命名 | options/variants 映射 | 输出解析与会话 |
|---|---|---|---|---|---|
| codex | read-only, read-write | `--sandbox read-only` / `--sandbox workspace-write` | `codex/<model>@variant` | 映射到 `-c key=value` | `json_stream`，支持 session |
| claude | read-only, read-write | `--permission-mode plan` / `--dangerously-skip-permissions` | `claude/<model>@variant` | 默认不映射 | `json_object`，支持 session |
| gemini | read-only, read-write | `--approval-mode plan` + `--sandbox` / `-y` | `gemini/<model>@variant` | 默认不映射 | `json_object`，支持 session |
| opencode | 仅 read-write | 无 read-only 参数 | `opencode/<provider>/<model>@variant` | 默认不映射 | `json_stream`，支持 session |
| kimi | 仅 read-write | 无 read-only 参数 | `kimi/<model>@variant` | 默认不映射 | `text`（stateless），无 session id |

Adapter 说明：

- adapter catalog 内置于 server（不需要 `adapter.json`）。
- `args_template` 是 token 列表，空 token 会被自动丢弃。
- `include_directories` 可从 prompt 绝对路径自动推导（Gemini）。
- 内置适配器默认 `prompt_transport=auto`。
- `json_stream` 支持 `fallback=codex` 回退解析。
- backend 可配置 `fallback` 处理 model-not-found。

## 说明

- MCP Server 是宿主无关组件，任何 MCP 宿主都可接入。
- 插件/skills 是宿主相关组件。
- 目录与插件 id 统一使用 `roundtable` 命名；品牌定位为 Roundtable-first。
