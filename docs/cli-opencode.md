# OpenCode CLI 操纵规则（three）

本文件描述 **three** 在 `backend.opencode` 下对 OpenCode CLI 的参数映射、会话控制与输出解析规则。它只针对「直接调用 opencode CLI」的路径，不覆盖 TUI/serve/attach 等二级封装。

模板配置来源：`~/.config/three/adapter.json`（或 `$XDG_CONFIG_HOME/three/adapter.json`）。

## 适用范围

- 适用于 `three` 的 **OpenCode CLI 后端**（`backend: opencode`）。
- 采用 **opencode run** 的非交互模式。
- 输出格式固定为 **json 事件流**（`--format json`）。

## 当前命令模板（概念版）

等价于如下模板逻辑（顺序很重要）：

```
run
{% if model != 'default' %}-m {{ model }}{% endif %}
{% if session_id %}-s {{ session_id }}{% endif %}
--format json
{{ prompt }}
```

## Prompt & 参数边界

- prompt 作为 **最后一个位置参数** 传入，不使用 `--prompt`。
- three 使用 **进程 current_dir** 设置工作目录；`--dir` 仅适用于 TUI/attach，不用于 `opencode run`。
- 当前模板 **不自动插入 `--`** 作为参数边界。

## 会话控制说明

- three 仅在已有 `session_id` 时传 `-s`，**不会使用 `--continue`**。
- 只有 `--format json` 才会输出 session id；`--format default` **不包含** session id。

## 输出解析规则

- `--format json` 输出为 **JSONL 事件流**。
- `session_id_path` 采用 `part.sessionID`（事件内字段）。
- `message_path` 采用 `part.text`（文本事件中的内容）。

对应 adapter 配置示例：

```
"output_parser": {
  "type": "json_stream",
  "session_id_path": "part.sessionID",
  "message_path": "part.text",
  "pick": "last"
}
```

## Capabilities 映射

- OpenCode CLI 的 `run` 模式 **没有** 与 `read-only / shell / network` 直接对应的 flags。
- three 通过 adapter 的 `filesystem_capabilities` 做**按 brain 校验**：
  - `opencode` 仅声明 `read-write`，因此 `filesystem: read-only` 会在解析 brain 时失败。
- 如需软约束，请通过 prompt guardrail 或自定义 adapter 实现。

## Model 默认值（重要）

当 `model == "default"` 时，three **不会传 `-m`**，让 OpenCode 使用其默认模型或会话内默认设置。

`-m` 传入的模型名应为 **provider/model**（可用 `opencode models` 查看），例如 `cchGemini/gemini-3-flash-preview`。
因此配置里通常写成 `opencode/cchGemini/gemini-3-flash-preview`。
