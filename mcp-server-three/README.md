# Three: The Multi-LLM "Vibe Coding" Router

**Three** is a unified orchestration system that turns Claude Code into a multi-soul coding cockpit. It allows you to delegate tasks to specialist agents (Oracle, Builder, Researcher) powered by different backend models (Codex, Gemini, Claude) while maintaining a single, coherent conversation context.

## 🌟 Why "Three"?

Because effective engineering often requires three perspectives:
1.  **The Architect** (Deep reasoning, trade-offs) -> *e.g. OpenAI o1 / Codex xhigh*
2.  **The Builder** (Fast, correct implementation) -> *e.g. Codex high / Sonnet 3.5*
3.  **The Critic/Reader** (Massive context, auditing) -> *e.g. Gemini 1.5 Pro*

Three unifies these into one CLI experience.

---

## 🏗 Architecture

```mermaid
graph TD
    User[User (Claude Code CLI)] -->|/three:oracle| Plugin[Claude Code Plugin]
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
-   **Role**: A named profile with model, capabilities, and optional persona override (e.g., `oracle` = read-only + high reasoning).
-   **Session**: Persisted conversation state keyed by `(repo_root, role, model)`. Switching roles automatically switches context.

---

## ✨ Features

-   **Session Reuse**: Doesn't waste tokens re-sending context. Session IDs are stored locally and resumed automatically per role.
-   **Native File Access**: External CLIs run *inside* your repo directory. They read files directly from disk, saving massive amounts of input tokens compared to pasting code into chat.
-   **Role Policies**: Enforce capability boundaries.
    -   *Example*: `builder` is `read-only`. `oracle` has `read-write`.
-   **Roundtable**: Run concurrent debates between multiple models (e.g., "Have Oracle and Reader debate this architecture").
-   **Configurable**: A single JSON file defines your entire agent fleet.

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
| `/three:oracle <task>` | Ask the "Oracle" role (high reasoning, deeper thought). |
| `/three:builder <task>` | Ask the "Builder" role (fast execution, implementation). |
| `/three:reviewer <request>` | Ask the "Reviewer" role to critique code. |
| `/three:roundtable <topic>`| Start a multi-model debate on a topic. |
| `/three:info` | Show current roles, models, and capability configuration. |

### Advanced: Session Management

You don't need to manage session IDs.
-   **Same role + Same repo = Same session.**
-   To reset a conversation (forget context), tell Claude: "Reset oracle session" or use `force_new_session` in tool calls.

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
