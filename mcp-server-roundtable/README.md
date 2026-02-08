# Roundtable MCP Server

**Roundtable MCP Server** (directory name still `mcp-server-roundtable`) is the portable orchestration core for multi-role coding workflows. It lets you delegate tasks to specialist agents (Oracle, Builder, Researcher, Reviewer, Critic, Sprinter) powered by different backend models (Codex, Gemini, Claude) while maintaining a single, coherent conversation context.

## 🌟 Why Roundtable-first?

Because effective engineering often requires roundtable core perspectives:
1. **Oracle** (architecture, trade-offs, long-term risks)
2. **Builder** (implementation feasibility, correct execution)
3. **Researcher** (codebase and documentation grounding)

Optional roles add extra coverage:
- **Reviewer** (quality and correctness)
- **Critic** (contrarian risk analysis)
- **Sprinter** (fast idea generation)

---

## 🏗 Architecture

```mermaid
graph TD
    User[User (Claude Code CLI)] -->|/roundtable:conductor| Plugin[Claude Code Plugin]
    Plugin -->|MCP Protocol| Roundtable[Roundtable MCP Server (Rust)]
    
    subgraph "Roundtable Engine"
        Roundtable -->|Read Config| Config[~/.config/roundtable/config.json]
        Roundtable -->|Load/Save| Sessions[~/.local/share/roundtable/sessions.json]
    end
    
    subgraph "Backend Adapters"
        Roundtable -->|Spawn| Codex[Codex CLI (Local)]
        Roundtable -->|Spawn| Gemini[Gemini CLI (Local)]
        Roundtable -->|Sampling| Host[Host Claude (MCP Sampling)]
    end
    
    Codex -->|Direct File Access| Repo[Local Repository]
    Gemini -->|Direct File Access| Repo
```

### Key Concepts

-   **Backend**: A CLI tool or API provider (e.g., `codex`, `gemini`).
-   **Model (Brain)**: A specific configuration of a backend (e.g., `gpt-5.2` with `reasoning_effort=high`).
-   **Role**: A named profile with model, capabilities, and optional persona override (e.g., `oracle`).
-   **Persona**: Built-in role prompt injected by the MCP server only on *new* sessions. `roles.<id>.personas` can override.
-   **Session**: Persisted conversation state keyed by `(repo_root, role, model)`. Switching roles switches context.

---

## ✨ Features

- **Session Reuse**: Session IDs are stored locally and resumed automatically per role.
- **Native File Access**: External CLIs run *inside* your repo directory and read files directly from disk.
- **Role Capabilities**: Configure `filesystem`, `shell`, `network`, and `tools` per role (defaults are permissive). Backends like Kimi/OpenCode reject read-only.
- **Built-in Personas**: The server injects persona prompts only when a new session is created.
- **Roundtable**: Plugin workflow that runs multi-role discussions and converges on a result.
- **Configurable**: A single JSON file defines models, roles, and overrides.
- **Prompt Transport Auto**: All adapters switch to stdin for long prompts to avoid argv limits (no mixed transport).
- **Model Fallback**: Roles can declare fallback models to retry on model-not-found errors.

---

## 🚀 Installation & Setup

### 1. Prerequisites
-   **Rust**: `cargo` installed.
-   **Backends**:
    -   `codex` CLI installed and authenticated.
    -   `gemini` CLI installed and authenticated.
-   **Claude Code**: Installed.

### 2. Build & Install MCP Server

```bash
# In the mcp-server-roundtable/ directory
cargo build --release

Note: the compiled binary is `target/release/mcp-server-roundtable`. The MCP server name you register can still be `roundtable`.

# Register with Claude Code
claude mcp add roundtable -s user --transport stdio -- \
  "$(pwd)/target/release/mcp-server-roundtable"
```

### 3. Configure (`~/.config/roundtable/config.json`)

Create your unified config. This defines which models you use and what roles exist.

```bash
mkdir -p ~/.config/roundtable
cp examples/config.json ~/.config/roundtable/config.json
```

**Minimal Config Example:**

```json
{
  "backend": {
    "codex": {
      "models": {
        "gpt-5.2-codex": {
          "options": { "model_reasoning_effort": "high" }
        }
      }
    }
  },
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2-codex",
      "capabilities": { "filesystem": "read-only" }
    }
  }
}
```

Notes:
- Personas are built into the MCP server. `roles.<id>.personas` is optional and overrides the built-in persona for that role.
- `roles.<id>.enabled` defaults to `true` and disables a role when set to `false`.
- `backend.<id>.fallback` retries on model-not-found errors (can span backends).
- See `docs/config-schema.md` for full details.

### 4. Install Claude Code Plugin

This adds the `/roundtable:*` slash commands to your chat.

```bash
# Add local marketplace
claude plugin marketplace add "./plugins/claude-code"

# Install plugin
claude plugin install roundtable@roundtable-local
```

---

## 🎮 Usage

### Commands

| Command | Description |
| :--- | :--- |
| `/roundtable:conductor <task>` | Orchestrate work and decide when to use roundtable. |
| `/roundtable:roundtable <topic>` | Multi-role consensus workflow (1–3 rounds). |
| `roundtable-batch` (`mcp__roundtable__roundtable_batch`) | Independent fan-out with session reuse. |
| `/roundtable:oracle <task>` | Architecture and trade-off analysis. |
| `/roundtable:builder <task>` | Implementation and debugging. |
| `/roundtable:researcher <task>` | Codebase/doc search and grounding. |
| `/roundtable:reviewer <request>` | Code review and risk finding. |
| `/roundtable:critic <request>` | Contrarian risk analysis. |
| `/roundtable:sprinter <task>` | Fast idea generation. |
| `/roundtable:info` | Troubleshooting view of roles/models/capabilities. |

### Advanced: Session Management

You don't need to manage session IDs.
- **Same role + Same repo = Same session.**
- To reset a conversation, use `force_new_session` in tool calls.

Notes:
- `roundtable-batch` and `roundtable` return partial results even if some tasks fail.
- Kimi has no session id. Parallel *resuming* across multiple Kimi roles in the same repo is rejected. Use `force_new_session=true` or a single Kimi role.

---

## ⚙️ Configuration Reference

### Backend Models
Define available models under `backend.<provider>.models`.
-   `id`: The actual model string passed to the CLI (e.g., `gpt-5.2`).
-   `options`: Provider-specific flags (e.g., `reasoningEffort`).
-   `fallback` (optional, backend-level): fallback model + patterns for model-not-found errors.

### Roles
Define agents under `roles.<name>`.
-   `model`: Reference to a model using `backend/model` syntax (e.g., `codex/gpt-5.2-codex`).
-   `capabilities`: `filesystem`, `shell`, `network`, `tools` (defaults are permissive).
-   `timeout_secs`: Execution timeout (default 600s).
-   `personas` (optional): override the built-in persona.
-   `enabled` (optional): disable a role without deleting it.

See `docs/config-schema.md` for the full schema and behavior.

---

## 🛠 Development

### Project Structure
-   `mcp-server-roundtable/`: Rust MCP server source code.
-   `plugins/claude-code/roundtable/`: Claude Code plugin definition (commands, skills).

### Testing
```bash
cargo test
```
