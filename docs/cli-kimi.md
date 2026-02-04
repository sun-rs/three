# Kimi CLI 操纵规则（three）

本文件描述 **three** 在 `backend.kimi` 下对 Kimi Code CLI 的参数映射、会话控制与输出解析规则。它只针对「直接调用 Kimi CLI」的路径，不覆盖 wire/acp 等二级封装。

模板配置来源：`~/.config/three/adapter.json`（或 `$XDG_CONFIG_HOME/three/adapter.json`）。

## 适用范围

- 适用于 `three` 的 **Kimi CLI 后端**（`backend: kimi`）。
- 采用 **print 模式**（非交互）。
- 输出格式使用 **text**，并启用 `--final-message-only`。

## 当前命令模板（概念版）

```
--print
--thinking
--output-format text
--final-message-only
--work-dir {{ workdir }}
{% if model != 'default' %}--model {{ model }}{% endif %}
{% if session_id %}--session {{ session_id }}{% endif %}
--prompt {{ prompt_or_guardrail }}
```

其中 `prompt_or_guardrail` 为：

```
{% if capabilities.filesystem == 'read-only' %}
{{ prompt }}
不允许写文件
{% else %}
{{ prompt }}
{% endif %}
```

## Prompt & 参数边界

- three 将最终 prompt 作为 `--prompt` 的 **单个参数** 传入。
- 不使用 stdin，不自动插入 `--` 作为参数边界。
- 如需其它 CLI 行为，请显式扩展 `adapter.args_template`。

## 会话控制说明

- Kimi CLI 支持 `--session <id>` 和 `--continue`。
- three 仅在有显式 session_id 时使用 `--session`，**不会使用 `--continue`**。
- Kimi 的 text 输出 **不包含 session_id**，因此 three 默认视为 `stateless`，无法自动续接。

## 输出解析规则

Kimi 的 `--output-format text` 为纯文本输出，three 使用 `text` 解析器：

```
"output_parser": {
  "type": "text"
}
```

该解析器会把 stdout trim 后作为 `agent_messages`，并将 session 设为 `stateless`。

## 读写权限（重要）

- `--print` 模式会**隐式开启 yolo/auto-approve**，Kimi CLI 没有可用的 `--no-yolo` 或等价关闭方式。
- 因此 **read-only 无法被强制**。
- three 通过 adapter 的 `filesystem_capabilities` 做**按 brain 校验**：
  - `kimi` 仅声明 `read-write`，因此 `filesystem: read-only` 会在解析 brain 时失败。

结论：**Kimi 不支持 read‑only**。如需软约束，请使用 `read-write` 并在 prompt 中自行加 guardrail。

## Model 默认值（重要）

当 `model == "default"` 时，three **不会传 `--model`**，让 Kimi 使用其配置文件中的默认模型。
这同样适用于其他 CLI 后端（claude/codex/gemini/opencode）。
