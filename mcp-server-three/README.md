# Three: The Multi-LLM "Vibe Coding" Router

**Three** is a unified orchestration system that turns Claude Code into a multi-role coding cockpit. It lets you delegate tasks to specialist agents (Oracle, Builder, Researcher, Reviewer, Critic, Sprinter) powered by different backend models (Codex, Gemini, Claude) while maintaining a single, coherent conversation context.

## 🌟 Why "Three"?

Because effective engineering often requires three core perspectives:
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
    User[User (Claude Code CLI)] -->|/three:conductor| Plugin[Claude Code Plugin]
    Plugin -->|MCP Protocol| Three[Three MCP Server (Rust)]
    
    subgraph "Three Engine"
        Three -->|Read Config| Config[~/.config/three/config.json]
        Three -->|Load/Save| Sessions[~/.local/share/three/sessions.json]
    end
    
    subgraph "Backend Adapters"
        Three -->|Spawn| Codex[Codex CLI (Local)]
        Three -->|Spawn| Gemini[Gemini CLI (Local)]
        Three -->|Sampling| Host[Host Claude (MCP Sampling)]
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
# In the mcp-server-three/ directory
cargo build --release

Note: the compiled binary is `target/release/mcp-server-three`. The MCP server name you register can still be `three`.

# Register with Claude Code
claude mcp add three -s user --transport stdio -- \
  "$(pwd)/target/release/mcp-server-three"
```

### 3. Configure (`~/.config/three/config.json`)

Create your unified config. This defines which models you use and what roles exist.

```bash
mkdir -p ~/.config/three
cp examples/config.json ~/.config/three/config.json
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
- See `docs/config-schema.md` for full details.

### 4. Install Claude Code Plugin

This adds the `/three:*` slash commands to your chat.

```bash
# Add local marketplace
claude plugin marketplace add "./plugins/claude-code"

# Install plugin
claude plugin install three@three-local
```

---

## 🎮 Usage

### Commands

| Command | Description |
| :--- | :--- |
| `/three:conductor <task>` | Orchestrate work and decide when to use roundtable. |
| `/three:roundtable <topic>` | Multi-role consensus workflow (1–3 rounds). |
| `mcp__three__batch` | Parallel fan-out for independent tasks (partial failures returned). |
| `/three:oracle <task>` | Architecture and trade-off analysis. |
| `/three:builder <task>` | Implementation and debugging. |
| `/three:researcher <task>` | Codebase/doc search and grounding. |
| `/three:reviewer <request>` | Code review and risk finding. |
| `/three:critic <request>` | Contrarian risk analysis. |
| `/three:sprinter <task>` | Fast idea generation. |
| `/three:info` | Troubleshooting view of roles/models/capabilities. |

### Advanced: Session Management

You don't need to manage session IDs.
- **Same role + Same repo = Same session.**
- To reset a conversation, use `force_new_session` in tool calls.

Notes:
- `batch` and `roundtable` return partial results even if some tasks fail.
- Kimi has no session id. Parallel *resuming* across multiple Kimi roles in the same repo is rejected. Use `force_new_session=true` or a single Kimi role.

---

## ⚙️ Configuration Reference

### Backend Models
Define available models under `backend.<provider>.models`.
-   `id`: The actual model string passed to the CLI (e.g., `gpt-5.2`).
-   `options`: Provider-specific flags (e.g., `reasoningEffort`).

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
-   `mcp-server-three/`: Rust MCP server source code.
-   `plugins/claude-code/three/`: Claude Code plugin definition (commands, skills).

### Testing
```bash
cargo test
```
