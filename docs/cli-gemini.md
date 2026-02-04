# Gemini CLI 操纵规则（three）

本文件描述 **three** 在 `backend.gemini` 下对 Gemini CLI 的参数映射、会话控制与输出解析规则。它只针对「直接调用 Gemini CLI」的路径，不覆盖 opencode 等二级封装。

模板配置来源：`~/.config/three/adapter.json`（或 `$XDG_CONFIG_HOME/three/adapter.json`）。

## 适用范围

- 适用于 `three` 的 **Gemini CLI 后端**（`backend: gemini`）。
- 采用 **headless** 模式，默认输出格式为 `json`（便于自动化解析）。
- 本规则以 `examples/config.json` 与 `~/.config/three/config.json` 的当前模板为准。

## 当前命令模板（概念版）

等价于如下模板逻辑（顺序很重要）：

```
--output-format json
{% if capabilities.filesystem == 'read-only' %}--approval-mode plan{% endif %}
{% if capabilities.filesystem != 'read-only' %}-y{% endif %}
{% if model != 'default' %}-m {{ model }}{% endif %}
{% if capabilities.filesystem == 'read-only' %}--sandbox{% endif %}
{% if include_directories %}--include-directories {{ include_directories }}{% endif %}
{% if session_id %}--resume {{ session_id }}{% endif %}
--prompt {{ prompt }}
```

> 说明：`--prompt` 是显式使用 headless 标准参数；`-y` 用于自动批准，避免交互阻塞。

## 参数映射规则

### 1) 模型
- `brains.<name>.model` → 解析为 `backend/model@variant`
- Gemini 后端实际传参：`-m {{ model }}`
  - 若 `model == "default"`，则 **不传 `-m`**，使用 CLI 默认模型

### 2) Prompt
- `prompt` 会被 three 包装（包含 persona + 合同约束等），然后通过：
  `--prompt "{{ prompt }}"` 传入 Gemini CLI。

> Gemini 使用 `--prompt` 明确传参，因此不需要额外的 `--` 边界。

### 3) 会话（session）
- 若已存在 `session_id`：传 `--resume {{ session_id }}`
- 未提供时不传，CLI 自动新建会话

### 4) 读写能力（capabilities）
- `filesystem = read-only` → **传 `--sandbox`**
- `filesystem = read-write` → **不传任何参数**

> 注意：Gemini CLI 只有 `sandbox` 开关，没有更细粒度的读写控制。

### 5) 工作区访问（include-directories）
- 当 `prompt` 中包含 **workspace 外的绝对路径** 时，three 会自动追加：
  `--include-directories <dir1,dir2,...>`
- 目录集合来自 `prompt` 中识别到的绝对路径（自动去重）。

### 6) 审批与交互
- `filesystem = read-only` → 传 `--approval-mode plan`（Gemini CLI 的只读模式）
- 其他情况 → 传 `-y`（YOLO / auto-approve），避免 MCP 进入交互卡死。

## Brain 可影响的参数（当前）

- `model` → `-m`
  - 若 `model == "default"`，则 **不传 `-m`**，使用 CLI 默认模型
- `capabilities.filesystem` → `--approval-mode plan` / `-y` / `--sandbox`
- `personas.prompt` → 已合并进最终 `prompt`
- `options/variants` → **当前模板未映射**（需要显式扩展 adapter）

## 输出解析规则

- 输出格式：`json`（一次性结构化 JSON）
- 解析字段：
  - `agent_messages`：取 `response`
  - `session_id`：取 `session_id`
- 会话：在当前 Gemini CLI 版本中，`json` **包含** `session_id`，因此可以继续续接。若未来版本不再提供，请移除 `session_id_path` 或回退到 `stream-json`。

## 限制与注意事项

1) **不支持 stdin 输入**：three 在后端调用时使用 `stdin = null`。因此 `echo "..." | gemini` 这种模式不适用。
2) **`json` 更适合自动化解析**：它返回一次性结构化 JSON，便于机器消费；在当前 Gemini CLI 版本中也**包含 `session_id`**，因此可用于会话复用。  
   若你的版本不提供 `session_id`，请移除 `session_id_path`，或回退到 `stream-json`。
3) **`--debug` 未映射**：若需要，需显式扩展 `adapter.args_template`。

## Model 默认值（重要）

当 `model == "default"` 时，three **不会传 `-m`**，Gemini CLI 将使用其配置文件中的默认模型。  
若本机未配置默认模型，CLI 可能会报错或使用内置默认值。

## 示例片段（摘自 examples/config.json）

```json
"gemini": {
  "adapter": {
    "args_template": [
      "--output-format",
      "json",
      "{% if capabilities.filesystem == 'read-only' %}--approval-mode{% endif %}",
      "{% if capabilities.filesystem == 'read-only' %}plan{% endif %}",
      "{% if capabilities.filesystem != 'read-only' %}-y{% endif %}",
      "-m",
      "{{ model }}",
      "{% if capabilities.filesystem == 'read-only' %}--sandbox{% endif %}",
      "{% if include_directories %}--include-directories{% endif %}",
      "{{ include_directories }}",
      "{% if session_id %}--resume{% endif %}",
      "{% if session_id %}{{ session_id }}{% endif %}",
      "--prompt",
      "{{ prompt }}"
    ],
    "output_parser": {
      "type": "json_object",
      "session_id_path": "session_id",
      "message_path": "response"
    }
  }
}
```

## 与 opencode 的差异（关于 high/low thinking）

你给的 opencode provider 配置中出现 `thinkingConfig` / `thinkingLevel`（high/low/medium），这是 **opencode 的 Google SDK 层能力**，并不是 Gemini CLI headless 的标准参数。

结论：
- **Gemini CLI 后端**不支持这些 high/low 变体；
- **Gemini API（经 opencode/SDK）**可能支持这些变体（由 opencode 把选项传给 SDK）。

如需把这些变体纳入 three，请在 **opencode 后端** 的 `options/variants` 里实现，而不是 gemini CLI。 
