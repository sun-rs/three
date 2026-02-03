use crate::{
    backends,
    config::{Backend, CodexSandboxPolicy, ReasoningEffort},
    config_loader::ConfigLoader,
    contract,
    session_store::{now_unix_secs, SessionRecord, SessionStore},
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, Peer, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Input parameters for the three tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct VibeArgs {
    /// Task instruction
    #[serde(rename = "PROMPT")]
    pub prompt: String,

    /// Working directory (repo root recommended)
    pub cd: String,

    /// Role name (used in session key + optional config mapping)
    #[serde(default)]
    pub role: Option<String>,

    /// Brain profile id (optional, resolved via config when present)
    #[serde(default)]
    pub brain: Option<String>,

    /// Backend override when not using config (codex|gemini)
    #[serde(default)]
    pub backend: Option<String>,

    /// Model override when not using config
    #[serde(default)]
    pub model: Option<String>,

    /// Reasoning effort override for codex when not using config (low|medium|high|xhigh)
    #[serde(default)]
    pub reasoning_effort: Option<String>,

    /// Resume an existing backend session id (manual override)
    #[serde(rename = "SESSION_ID", default)]
    pub session_id: Option<String>,

    /// Ignore stored session and force a new one
    #[serde(default)]
    pub force_new_session: bool,

    /// Explicit session key override (advanced). If provided, this key is used for persistence/locking.
    #[serde(default)]
    pub session_key: Option<String>,

    /// Backend timeout in seconds (default: 600)
    #[serde(default)]
    pub timeout_secs: Option<u64>,

    /// Output contract enforcement (optional)
    #[serde(default)]
    pub contract: Option<OutputContract>,

    /// If true, run `git apply --check` on extracted unified diff patches.
    #[serde(default)]
    pub validate_patch: bool,
}

/// Input parameters for the roundtable tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RoundtableArgs {
    /// Topic/question for the roundtable
    #[serde(rename = "TOPIC")]
    pub topic: String,

    /// Working directory (repo root recommended)
    pub cd: String,

    /// Participant list
    pub participants: Vec<RoundtableParticipant>,

    /// Optional moderator (synthesis brain). If omitted, returns contributions only.
    #[serde(default)]
    pub moderator: Option<RoundtableModerator>,

    /// Default timeout in seconds for each participant (default: 600)
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Input parameters for the info tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InfoArgs {
    /// Working directory (repo root recommended)
    pub cd: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct RoundtableParticipant {
    pub name: String,

    #[serde(default)]
    pub role: Option<String>,

    #[serde(default)]
    pub brain: Option<String>,

    #[serde(default)]
    pub backend: Option<String>,

    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub reasoning_effort: Option<String>,

    #[serde(default)]
    pub force_new_session: bool,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct RoundtableModerator {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub brain: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub force_new_session: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputContract {
    PatchWithCitations,
}

#[derive(Debug, Serialize)]
struct VibeOutput {
    success: bool,
    backend: String,
    role: String,
    brain_id: String,
    model: Option<String>,
    reasoning_effort: Option<String>,
    codex_sandbox: Option<String>,
    codex_ask_for_approval: Option<String>,
    codex_dangerously_bypass_approvals_and_sandbox: Option<bool>,
    codex_skip_git_repo_check: Option<bool>,
    session_key: String,
    resumed: bool,
    backend_session_id: String,
    agent_messages: String,
    warnings: Option<String>,
    contract: Option<String>,
    contract_errors: Vec<String>,
    patch_format: Option<String>,
    patch_apply_check_ok: Option<bool>,
    patch_apply_check_output: Option<String>,
    error: Option<String>,
}

fn codex_sandbox_str(p: CodexSandboxPolicy) -> &'static str {
    match p {
        CodexSandboxPolicy::ReadOnly => "read-only",
        CodexSandboxPolicy::WorkspaceWrite => "workspace-write",
        CodexSandboxPolicy::DangerFullAccess => "danger-full-access",
    }
}

fn codex_approval_str(p: crate::config::CodexApprovalPolicy) -> &'static str {
    use crate::config::CodexApprovalPolicy;
    match p {
        CodexApprovalPolicy::Untrusted => "untrusted",
        CodexApprovalPolicy::OnFailure => "on-failure",
        CodexApprovalPolicy::OnRequest => "on-request",
        CodexApprovalPolicy::Never => "never",
    }
}

#[derive(Debug, Serialize)]
struct RoundtableOutput {
    success: bool,
    topic: String,
    cd: String,
    contributions: Vec<RoundtableContribution>,
    synthesis: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct InfoOutput {
    success: bool,
    cd: String,
    config_sources: Vec<String>,
    roles: Vec<InfoRole>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct InfoRole {
    role: String,
    description: Option<String>,
    brain: String,
    backend: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    codex_sandbox: Option<String>,
    codex_ask_for_approval: Option<String>,
    codex_dangerously_bypass_approvals_and_sandbox: Option<bool>,
    codex_skip_git_repo_check: Option<bool>,
    timeout_secs: Option<u64>,
    persona_source: String,
    prompt_present: bool,
    prompt_len: Option<usize>,
    prompt_preview: Option<String>,
}

#[derive(Debug, Serialize)]
struct RoundtableContribution {
    name: String,
    role: String,
    backend: String,
    brain_id: String,
    resumed: bool,
    backend_session_id: String,
    agent_messages: String,
    error: Option<String>,
}

#[derive(Clone)]
pub struct VibeServer {
    tool_router: ToolRouter<VibeServer>,
    config_loader: ConfigLoader,
    store: SessionStore,
}

impl VibeServer {
    pub fn new(config_loader: ConfigLoader, store: SessionStore) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config_loader,
            store,
        }
    }
}

#[tool_router]
impl VibeServer {
    /// Route a prompt to a configured backend (codex|gemini) with session reuse.
    ///
    /// Best practice: pass `cd` as your repo root and provide `role` + `brain` (or `role` only when config maps it).
    #[tool(name = "three", description = "Route a prompt to configured backends with session reuse")]
    async fn vibe(
        &self,
        peer: Peer<RoleServer>,
        Parameters(args): Parameters<VibeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let out = self.run_vibe_internal(Some(peer), args).await?;
        let json = serde_json::to_string(&out)
            .map_err(|e| McpError::internal_error(format!("failed to serialize output: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Run a multi-brain discussion on a topic and optionally synthesize.
    #[tool(
        name = "roundtable",
        description = "Fan-out a topic to multiple brains and optionally synthesize a conclusion"
    )]
    async fn roundtable(
        &self,
        peer: Peer<RoleServer>,
        Parameters(args): Parameters<RoundtableArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.topic.trim().is_empty() {
            return Err(McpError::invalid_params(
                "TOPIC is required and must be a non-empty string",
                None,
            ));
        }
        if args.cd.trim().is_empty() {
            return Err(McpError::invalid_params(
                "cd is required and must be a non-empty string",
                None,
            ));
        }
        if args.participants.is_empty() {
            return Err(McpError::invalid_params(
                "participants must be a non-empty array",
                None,
            ));
        }

        // Canonicalize cd once to validate it's usable.
        let cd = PathBuf::from(args.cd.as_str());
        let repo_root = cd.canonicalize().map_err(|e| {
            McpError::invalid_params(
                format!(
                    "working directory does not exist or is not accessible: {} ({})",
                    cd.display(),
                    e
                ),
                None,
            )
        })?;
        if !repo_root.is_dir() {
            return Err(McpError::invalid_params(
                format!("working directory is not a directory: {}", repo_root.display()),
                None,
            ));
        }

        let RoundtableArgs {
            topic,
            participants,
            moderator,
            timeout_secs,
            cd: _,
        } = args;

        let topic_trimmed = topic.trim().to_string();
        let repo_cd = repo_root.to_string_lossy().to_string();
        let timeout_override = timeout_secs;

        let mut contributions = Vec::new();
        let mut any_error = false;

        // Run participants concurrently; collect partial results even if some fail.
        let mut joinset: tokio::task::JoinSet<(
            String,
            String,
            std::result::Result<VibeOutput, McpError>,
        )> = tokio::task::JoinSet::new();

        for p in participants {
            if p.name.trim().is_empty() {
                return Err(McpError::invalid_params(
                    "participant.name must be non-empty",
                    None,
                ));
            }

            let name = p.name.clone();
            let role = p
                .role
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| name.trim().to_string());

            let prompt = format!(
                "TOPIC:\n{}\n\nYou are a roundtable participant named '{}' (role: {}).\n\nReply with:\n1) Position (1-2 sentences)\n2) Arguments (bullets)\n3) Risks/edge cases (bullets)\n4) Recommendation (actionable)\n\nConstraints:\n- Do not claim to have run commands unless you actually did.\n- Prefer referencing repo paths when relevant.\n",
                topic_trimmed,
                name.trim(),
                role
            );

            let server = VibeServer::new(self.config_loader.clone(), self.store.clone());
            let cd = repo_cd.clone();
            let role_for_out = role.clone();
            let peer = peer.clone();

            joinset.spawn(async move {
                let out = server
                    .run_vibe_internal(Some(peer), VibeArgs {
                        prompt,
                        cd,
                        role: Some(role_for_out.clone()),
                        brain: p.brain,
                        backend: p.backend,
                        model: p.model,
                        reasoning_effort: p.reasoning_effort,
                        session_id: None,
                        force_new_session: p.force_new_session,
                        session_key: None,
                        timeout_secs: timeout_override,
                        contract: None,
                        validate_patch: false,
                    })
                    .await;
                (name, role_for_out, out)
            });
        }

        while let Some(joined) = joinset.join_next().await {
            match joined {
                Ok((name, _role, Ok(out))) => {
                    if out.error.is_some() {
                        any_error = true;
                    }
                    contributions.push(RoundtableContribution {
                        name,
                        role: out.role.clone(),
                        backend: out.backend.clone(),
                        brain_id: out.brain_id.clone(),
                        resumed: out.resumed,
                        backend_session_id: out.backend_session_id.clone(),
                        agent_messages: out.agent_messages.clone(),
                        error: out.error.clone(),
                    });
                }
                Ok((name, role, Err(e))) => {
                    any_error = true;
                    contributions.push(RoundtableContribution {
                        name,
                        role,
                        backend: "error".to_string(),
                        brain_id: "".to_string(),
                        resumed: false,
                        backend_session_id: "".to_string(),
                        agent_messages: "".to_string(),
                        error: Some(e.to_string()),
                    });
                }
                Err(e) => {
                    any_error = true;
                    contributions.push(RoundtableContribution {
                        name: "".to_string(),
                        role: "".to_string(),
                        backend: "error".to_string(),
                        brain_id: "".to_string(),
                        resumed: false,
                        backend_session_id: "".to_string(),
                        agent_messages: "".to_string(),
                        error: Some(format!("join error: {e}")),
                    });
                }
            }
        }

        let mut synthesis: Option<String> = None;
        if let Some(m) = moderator {
            let role = m
                .role
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "moderator".to_string());

            let mut transcript = String::new();
            for c in &contributions {
                transcript.push_str("---\n");
                transcript.push_str(&format!("participant: {}\nrole: {}\nbackend: {}\n\n{}\n\n", c.name, c.role, c.backend, c.agent_messages));
            }

            let prompt = format!(
                "You are the moderator. Synthesize the roundtable into a single decision.\n\nTOPIC:\n{}\n\nCONTRIBUTIONS:\n{}\n\nOutput:\n- Conclusion (1 paragraph)\n- Tradeoffs (bullets)\n- Next actions (bullets)\n- Open questions (bullets, optional)\n",
                topic_trimmed,
                transcript
            );

            let out = self
                .run_vibe_internal(Some(peer.clone()), VibeArgs {
                    prompt,
                    cd: repo_root.to_string_lossy().to_string(),
                    role: Some(role),
                    brain: m.brain,
                    backend: m.backend,
                    model: m.model,
                    reasoning_effort: m.reasoning_effort,
                    session_id: None,
                    force_new_session: m.force_new_session,
                    session_key: None,
                    timeout_secs: timeout_override,
                    contract: None,
                    validate_patch: false,
                })
                .await;

            match out {
                Ok(out) => {
                    synthesis = Some(out.agent_messages);
                    if out.error.is_some() {
                        any_error = true;
                    }
                }
                Err(e) => {
                    any_error = true;
                    synthesis = Some(format!("moderator error: {e}"));
                }
            }
        }

        let out = RoundtableOutput {
            success: !any_error,
            topic,
            cd: repo_root.to_string_lossy().to_string(),
            contributions,
            synthesis,
            error: if any_error {
                Some("one or more participants/moderator returned an error".to_string())
            } else {
                None
            },
        };

        let json = serde_json::to_string(&out)
            .map_err(|e| McpError::internal_error(format!("failed to serialize output: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Show effective config (roles -> brains -> models) without calling any LLM.
    #[tool(name = "info", description = "Show effective three role/model mapping for this directory")]
    async fn info(
        &self,
        Parameters(args): Parameters<InfoArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.cd.trim().is_empty() {
            return Err(McpError::invalid_params(
                "cd is required and must be a non-empty string",
                None,
            ));
        }

        let cd = PathBuf::from(args.cd.as_str());
        let repo_root = cd.canonicalize().map_err(|e| {
            McpError::invalid_params(
                format!(
                    "working directory does not exist or is not accessible: {} ({})",
                    cd.display(),
                    e
                ),
                None,
            )
        })?;
        if !repo_root.is_dir() {
            return Err(McpError::invalid_params(
                format!("working directory is not a directory: {}", repo_root.display()),
                None,
            ));
        }

        let mut sources: Vec<String> = Vec::new();
        if let Some(p) = self.config_loader.user_config_path() {
            if p.exists() {
                sources.push(p.display().to_string());
            }
        }
        for p in ConfigLoader::project_config_paths(&repo_root) {
            if p.exists() {
                sources.push(p.display().to_string());
                break;
            }
        }

        let cfg = self
            .config_loader
            .load_for_repo(&repo_root)
            .map_err(|e| McpError::internal_error(format!("failed to load config: {e}"), None))?;

        let Some(cfg) = cfg else {
            let out = InfoOutput {
                success: false,
                cd: repo_root.to_string_lossy().to_string(),
                config_sources: sources,
                roles: Vec::new(),
                error: Some("no config found (create ~/.config/three/config.json)".to_string()),
            };
            let json = serde_json::to_string(&out).map_err(|e| {
                McpError::internal_error(format!("failed to serialize output: {e}"), None)
            })?;
            return Ok(CallToolResult::success(vec![Content::text(json)]));
        };

        let mut roles: Vec<InfoRole> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for (role_id, role_cfg) in &cfg.roles {
            let prof = cfg.brains.get(&role_cfg.brain);
            let (backend, model, reasoning_effort) = match prof {
                Some(p) => (
                    Some(match p.backend {
                        Backend::Codex => "codex".to_string(),
                        Backend::Gemini => "gemini".to_string(),
                        Backend::Claude => "claude".to_string(),
                    }),
                    p.model.clone(),
                    p.reasoning_effort
                        .map(|e| e.as_codex_config_value().to_string()),
                ),
                None => {
                    errors.push(format!(
                        "role '{role_id}' references missing brain '{}'",
                        role_cfg.brain
                    ));
                    (None, None, None)
                }
            };

            // Precedence:
            // 1) roles.<role>.prompt
            // 2) roles.<role>.persona -> personas.<id>.prompt
            let (persona_source, prompt_raw) = if let Some(p) = role_cfg
                .prompt
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                ("role.prompt".to_string(), Some(p.to_string()))
            } else if let Some(pid) = role_cfg.persona.as_deref() {
                let p = cfg
                    .personas
                    .get(pid)
                    .and_then(|x| x.prompt.as_deref())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                (format!("personas.{pid}"), p)
            } else {
                ("none".to_string(), None)
            };

            let (prompt_present, prompt_len, prompt_preview) = match prompt_raw.as_deref() {
                Some(p) => {
                    let len = p.len();
                    let preview_len = 120usize;
                    let preview = if len <= preview_len {
                        p.to_string()
                    } else {
                        format!("{}...", &p[..preview_len])
                    };
                    (true, Some(len), Some(preview))
                }
                None => (false, None, None),
            };

            let (codex_sandbox, codex_ask_for_approval, codex_bypass, codex_skip_git_repo_check) =
                match backend.as_deref() {
                    Some("codex") => (
                        Some(codex_sandbox_str(role_cfg.policy.codex.sandbox).to_string()),
                        role_cfg
                            .policy
                            .codex
                            .ask_for_approval
                            .map(|p| codex_approval_str(p).to_string()),
                        Some(role_cfg.policy.codex.dangerously_bypass_approvals_and_sandbox),
                        Some(
                            role_cfg
                                .policy
                                .codex
                                .skip_git_repo_check
                                .unwrap_or(matches!(
                                    role_cfg.policy.codex.sandbox,
                                    CodexSandboxPolicy::ReadOnly
                                )),
                        ),
                    ),
                    _ => (None, None, None, None),
                };

            let description = role_cfg
                .description
                .clone()
                .or_else(|| role_cfg.persona.as_deref().and_then(|pid| {
                    cfg.personas
                        .get(pid)
                        .and_then(|p| p.description.clone())
                }));

            roles.push(InfoRole {
                role: role_id.to_string(),
                description,
                brain: role_cfg.brain.clone(),
                backend,
                model,
                reasoning_effort,
                codex_sandbox,
                codex_ask_for_approval,
                codex_dangerously_bypass_approvals_and_sandbox: codex_bypass,
                codex_skip_git_repo_check,
                timeout_secs: role_cfg.timeout_secs,
                persona_source,
                prompt_present,
                prompt_len,
                prompt_preview,
            });
        }

        let out = InfoOutput {
            success: errors.is_empty(),
            cd: repo_root.to_string_lossy().to_string(),
            config_sources: sources,
            roles,
            error: if errors.is_empty() {
                None
            } else {
                Some(errors.join("; "))
            },
        };

        let json = serde_json::to_string(&out)
            .map_err(|e| McpError::internal_error(format!("failed to serialize output: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

impl VibeServer {
    async fn run_vibe_internal(
        &self,
        peer: Option<Peer<RoleServer>>,
        args: VibeArgs,
    ) -> Result<VibeOutput, McpError> {
        if args.prompt.trim().is_empty() {
            return Err(McpError::invalid_params(
                "PROMPT is required and must be a non-empty string",
                None,
            ));
        }
        if args.cd.trim().is_empty() {
            return Err(McpError::invalid_params(
                "cd is required and must be a non-empty string",
                None,
            ));
        }

        let cd = PathBuf::from(args.cd.as_str());
        let repo_root = cd.canonicalize().map_err(|e| {
            McpError::invalid_params(
                format!("working directory does not exist or is not accessible: {} ({})", cd.display(), e),
                None,
            )
        })?;
        if !repo_root.is_dir() {
            return Err(McpError::invalid_params(
                format!("working directory is not a directory: {}", repo_root.display()),
                None,
            ));
        }

        let mut prompt_text = args.prompt.clone();

        // Resolve profile either from config (brain/role) or from explicit backend/model/effort.
        let mut resolved_backend: Backend;
        let mut resolved_model: Option<String>;
        let mut resolved_effort: Option<ReasoningEffort>;
        let resolved_brain_id: String;
        let role = args.role.clone().unwrap_or_else(|| "default".to_string());

        let cfg_for_repo = self
            .config_loader
            .load_for_repo(&repo_root)
            .map_err(|e| McpError::internal_error(format!("failed to load config: {e}"), None))?;

        // Role-level policy and defaults.
        // Default: skip git repo trust check only in read-only mode.
        let mut codex_sandbox = CodexSandboxPolicy::ReadOnly;
        let mut codex_ask_for_approval = Some(crate::config::CodexApprovalPolicy::Never);
        let mut codex_bypass = false;
        let mut codex_skip_git = true;
        let mut role_timeout_secs: Option<u64> = None;

        if let Some(cfg) = cfg_for_repo.as_ref() {
            if let Some(rc) = cfg.roles.get(role.as_str()) {
                codex_sandbox = rc.policy.codex.sandbox;
                codex_ask_for_approval = rc.policy.codex.ask_for_approval;
                codex_bypass = rc.policy.codex.dangerously_bypass_approvals_and_sandbox;
                codex_skip_git = rc.policy.codex.skip_git_repo_check.unwrap_or(matches!(
                    codex_sandbox,
                    CodexSandboxPolicy::ReadOnly
                ));
                role_timeout_secs = rc.timeout_secs;
            }
        }

        if let Some(cfg) = cfg_for_repo.as_ref() {
            if args.brain.is_some() || args.role.is_some() {
                let rp = cfg
                    .resolve_profile(args.role.as_deref(), args.brain.as_deref())
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                resolved_backend = rp.profile.backend;
                resolved_model = rp.profile.model;
                resolved_effort = rp.profile.reasoning_effort;
                resolved_brain_id = rp.brain_id;
            } else {
                // Config present but not used.
                (resolved_backend, resolved_model, resolved_effort, resolved_brain_id) =
                    resolve_explicit_profile(&args).map_err(|e| McpError::invalid_params(e, None))?;
            }
        } else {
            (resolved_backend, resolved_model, resolved_effort, resolved_brain_id) =
                resolve_explicit_profile(&args).map_err(|e| McpError::invalid_params(e, None))?;
        }

        // Optional persona injection (shared config is the source of truth).
        // Precedence:
        // 1) roles.<role>.prompt
        // 2) roles.<role>.persona -> personas.<id>.prompt
        if !prompt_text.contains("[THREE_PERSONA") {
            if let Some(cfg) = cfg_for_repo.as_ref() {
                if let Some(role_cfg) = cfg.roles.get(role.as_str()) {
                    // Prefer inline role prompt.
                    let inline = role_cfg
                        .prompt
                        .as_deref()
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(|s| ("role".to_string(), s.to_string()));

                    let library = role_cfg
                        .persona
                        .as_deref()
                        .and_then(|id| {
                            cfg.personas
                                .get(id)
                                .and_then(|p| p.prompt.as_deref())
                                .map(|t| (id.to_string(), t.to_string()))
                        });

                    if let Some((pid, ptext)) = inline.or(library) {
                        let ptext = ptext.trim();
                        if !ptext.is_empty() {
                            let role_id = role.as_str();
                            prompt_text = format!(
                                "[THREE_PERSONA id={pid} role={role_id}]\n{ptext}\n[/THREE_PERSONA]\n\n{prompt_text}"
                            );
                        }
                    }
                }
            }
        }

        // Explicit args override config, when provided.
        if let Some(model) = args.model.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            resolved_model = Some(model.to_string());
        }
        if let Some(eff) = args.reasoning_effort.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            resolved_effort = Some(parse_effort(eff).map_err(|e| McpError::invalid_params(e, None))?);
        }
        if let Some(backend) = args.backend.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            resolved_backend = parse_backend(backend).map_err(|e| McpError::invalid_params(e, None))?;
        }

        let session_key = args
            .session_key
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| SessionStore::compute_key(&repo_root, &role, &resolved_brain_id));
        let _key_lock = self
            .store
            .acquire_key_lock(&session_key)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let timeout_secs = args.timeout_secs.or(role_timeout_secs).unwrap_or(600);

        let prev_rec = self.store.get(&session_key).ok().flatten();
        let mut resumed = false;
        let mut session_id_to_use: Option<String> = None;
        let mut sampling_history_to_use: Vec<crate::session_store::SamplingHistoryMessage> = Vec::new();

        match resolved_backend {
            Backend::Codex | Backend::Gemini => {
                session_id_to_use = args.session_id.clone().filter(|s| !s.trim().is_empty());
                if session_id_to_use.is_none() && !args.force_new_session {
                    if let Some(rec) = prev_rec {
                        if rec.backend == resolved_backend {
                            session_id_to_use = Some(rec.backend_session_id);
                            resumed = true;
                        }
                    }
                }
            }
            Backend::Claude => {
                if !args.force_new_session {
                    if let Some(rec) = prev_rec {
                        if rec.backend == Backend::Claude && !rec.sampling_history.is_empty() {
                            sampling_history_to_use = rec.sampling_history;
                            resumed = true;
                        }
                    }
                }
            }
        }

        let mut model_used: Option<String> = resolved_model.clone();
        let mut sampling_history_to_save: Vec<crate::session_store::SamplingHistoryMessage> =
            Vec::new();

        let (backend_session_id, agent_messages, warnings) = match resolved_backend {
            Backend::Codex => {
                let r = backends::codex::run(backends::codex::CodexOptions {
                    prompt: prompt_text.clone(),
                    workdir: repo_root.clone(),
                    session_id: session_id_to_use,
                    model: resolved_model.clone(),
                    reasoning_effort: resolved_effort,
                    sandbox: codex_sandbox,
                    ask_for_approval: codex_ask_for_approval,
                    dangerously_bypass_approvals_and_sandbox: codex_bypass,
                    skip_git_repo_check: codex_skip_git,
                    timeout_secs,
                })
                .await
                .map_err(|e| McpError::internal_error(format!("codex failed: {e}"), None))?;
                (r.session_id, r.agent_messages, r.warnings)
            }
            Backend::Gemini => {
                let r = backends::gemini::run(backends::gemini::GeminiOptions {
                    prompt: prompt_text.clone(),
                    workdir: repo_root.clone(),
                    session_id: session_id_to_use,
                    model: resolved_model.clone(),
                    timeout_secs,
                })
                .await
                .map_err(|e| McpError::internal_error(format!("gemini failed: {e}"), None))?;
                (r.session_id, r.agent_messages, r.warnings)
            }
            Backend::Claude => {
                let peer = peer.ok_or_else(|| {
                    McpError::internal_error(
                        "backend=claude requires a host client that supports sampling/createMessage"
                            .to_string(),
                        None,
                    )
                })?;

                // Build conversation history from persisted sampling_history.
                let mut messages: Vec<SamplingMessage> = Vec::new();
                for m in &sampling_history_to_use {
                    let role = match m.role.as_str() {
                        "assistant" => Role::Assistant,
                        _ => Role::User,
                    };
                    messages.push(SamplingMessage {
                        role,
                        content: Content::text(&m.content),
                    });
                }
                messages.push(SamplingMessage {
                    role: Role::User,
                    content: Content::text(prompt_text.clone()),
                });

                let prefs = model_used.as_ref().map(|h| ModelPreferences {
                    hints: Some(vec![ModelHint {
                        name: Some(h.clone()),
                    }]),
                    cost_priority: None,
                    speed_priority: None,
                    intelligence_priority: None,
                });

                let fut = peer.create_message(CreateMessageRequestParams {
                    meta: None,
                    task: None,
                    messages: messages.clone(),
                    model_preferences: prefs,
                    system_prompt: None,
                    include_context: Some(ContextInclusion::None),
                    temperature: None,
                    max_tokens: 1024,
                    stop_sequences: None,
                    metadata: None,
                });

                let resp = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), fut)
                    .await
                    .map_err(|_| {
                        McpError::internal_error("claude sampling timed out".to_string(), None)
                    })
                    .and_then(|r| {
                        r.map_err(|e| {
                            McpError::internal_error(format!("claude sampling failed: {e}"), None)
                        })
                    })?;

                let text = resp
                    .message
                    .content
                    .as_text()
                    .map(|t| t.text.clone())
                    .unwrap_or_default();

                model_used = Some(resp.model);

                // Persist minimal sampling history: append user + assistant, cap size.
                let mut hist = sampling_history_to_use;
                hist.push(crate::session_store::SamplingHistoryMessage {
                    role: "user".to_string(),
                    content: prompt_text.clone(),
                });
                hist.push(crate::session_store::SamplingHistoryMessage {
                    role: "assistant".to_string(),
                    content: text.clone(),
                });
                if hist.len() > 20 {
                    hist = hist.split_off(hist.len() - 20);
                }
                sampling_history_to_save = hist;

                ("sampling".to_string(), text, None)
            }
        };

        // Persist session id
        self.store
            .put(
                &session_key,
                SessionRecord {
                    repo_root: repo_root.to_string_lossy().to_string(),
                    role: role.clone(),
                    brain_id: resolved_brain_id.clone(),
                    backend: resolved_backend,
                    backend_session_id: backend_session_id.clone(),
                    sampling_history: sampling_history_to_save,
                    updated_at_unix_secs: now_unix_secs(),
                },
            )
            .map_err(|e| McpError::internal_error(format!("failed to persist session: {e}"), None))?;

        let mut contract_errors: Vec<String> = Vec::new();
        let mut patch_format: Option<String> = None;
        let mut patch_apply_check_ok: Option<bool> = None;
        let mut patch_apply_check_output: Option<String> = None;
        let mut error: Option<String> = None;

        if let Some(OutputContract::PatchWithCitations) = args.contract {
            let check = contract::check_patch_with_citations(&agent_messages);
            contract_errors = check.errors.clone();
            patch_format = Some(format!("{:?}", check.patch_format).to_ascii_lowercase());

            if args.validate_patch {
                match (check.patch_format, check.extracted_patch.as_deref()) {
                    (contract::PatchFormat::UnifiedDiff, Some(patch)) => {
                        match contract::validate_git_apply_check(&repo_root, patch) {
                            Ok(apply) => {
                                patch_apply_check_ok = Some(apply.ok);
                                patch_apply_check_output = Some(apply.output);
                            }
                            Err(e) => {
                                patch_apply_check_ok = Some(false);
                                patch_apply_check_output = Some(e.to_string());
                            }
                        }
                    }
                    (contract::PatchFormat::UnifiedDiff, None) => {
                        patch_apply_check_ok = Some(false);
                        patch_apply_check_output = Some(
                            "validate_patch=true but failed to extract unified diff patch".to_string(),
                        );
                    }
                    _ => {
                        patch_apply_check_ok = Some(false);
                        patch_apply_check_output = Some(
                            "validate_patch=true but patch is not a unified diff".to_string(),
                        );
                    }
                }
            }

            // Contract gate: missing patch/citations always fails.
            if !contract_errors.is_empty() {
                error = Some(format!(
                    "output contract violation: {}",
                    contract_errors.join(", ")
                ));
            }
            // If patch validation requested, require it to pass.
            if args.validate_patch {
                if patch_apply_check_ok != Some(true) {
                    let msg = patch_apply_check_output
                        .clone()
                        .unwrap_or_else(|| "git apply --check failed".to_string());
                    error = Some(format!("patch validation failed: {msg}"));
                }
            }
        }

        let out = VibeOutput {
            success: error.is_none(),
            backend: match resolved_backend {
                Backend::Codex => "codex".to_string(),
                Backend::Gemini => "gemini".to_string(),
                Backend::Claude => "claude".to_string(),
            },
            role,
            brain_id: resolved_brain_id,
            model: model_used,
            reasoning_effort: resolved_effort.map(|e| e.as_codex_config_value().to_string()),
            codex_sandbox: match resolved_backend {
                Backend::Codex => Some(codex_sandbox_str(codex_sandbox).to_string()),
                Backend::Gemini | Backend::Claude => None,
            },
            codex_ask_for_approval: match resolved_backend {
                Backend::Codex => codex_ask_for_approval.map(|p| codex_approval_str(p).to_string()),
                Backend::Gemini | Backend::Claude => None,
            },
            codex_dangerously_bypass_approvals_and_sandbox: match resolved_backend {
                Backend::Codex => Some(codex_bypass),
                Backend::Gemini | Backend::Claude => None,
            },
            codex_skip_git_repo_check: match resolved_backend {
                Backend::Codex => Some(codex_skip_git),
                Backend::Gemini | Backend::Claude => None,
            },
            session_key,
            resumed,
            backend_session_id,
            agent_messages,
            warnings,
            contract: args.contract.map(|c| match c {
                OutputContract::PatchWithCitations => "patch_with_citations".to_string(),
            }),
            contract_errors,
            patch_format,
            patch_apply_check_ok,
            patch_apply_check_output,
            error,
        };

        Ok(out)
    }
}

#[tool_handler]
impl ServerHandler for VibeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "This server provides a 'three' tool that routes prompts to Codex/Gemini CLIs with session reuse."
                    .to_string(),
            ),
        }
    }
}

fn resolve_explicit_profile(
    args: &VibeArgs,
) -> std::result::Result<(Backend, Option<String>, Option<ReasoningEffort>, String), String> {
    let backend_str = args
        .backend
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "backend is required when no config role/brain is provided".to_string())?;
    let backend = parse_backend(&backend_str)?;
    let model = args.model.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let effort = args
        .reasoning_effort
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| parse_effort(&s))
        .transpose()?;

    let brain_id = match backend {
        Backend::Codex => {
            let m = model.as_deref().unwrap_or("default");
            let e = effort.map(|x| x.as_codex_config_value()).unwrap_or("default");
            format!("codex:{m}:{e}")
        }
        Backend::Gemini => {
            let m = model.as_deref().unwrap_or("default");
            format!("gemini:{m}")
        }
        Backend::Claude => {
            let m = model.as_deref().unwrap_or("default");
            format!("claude:{m}")
        }
    };

    Ok((backend, model, effort, brain_id))
}

fn parse_backend(s: &str) -> std::result::Result<Backend, String> {
    match s.to_ascii_lowercase().as_str() {
        "codex" => Ok(Backend::Codex),
        "gemini" => Ok(Backend::Gemini),
        "claude" => Ok(Backend::Claude),
        other => Err(format!("unknown backend: {other} (expected codex|gemini|claude)")),
    }
}

fn parse_effort(s: &str) -> std::result::Result<ReasoningEffort, String> {
    match s.to_ascii_lowercase().as_str() {
        "low" => Ok(ReasoningEffort::Low),
        "medium" => Ok(ReasoningEffort::Medium),
        "high" => Ok(ReasoningEffort::High),
        "xhigh" => Ok(ReasoningEffort::Xhigh),
        other => Err(format!("unknown reasoning_effort: {other} (expected low|medium|high|xhigh)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;
    use std::process::Command;

    // Note: older tests extracted tool output text; current tests call `run_vibe_internal` directly.

    fn write_fake_codex(bin: &Path, log: &Path, session_id: &str, agent_text: &str) {
        let agent_text_json = agent_text
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        let script = format!(
            "#!/bin/sh\nset -e\n( printf 'ARGS:'; printf ' %s' \"$@\"; printf '\\n' ) > \"{}\"\n\nprintf '%s\\n' '{{\"type\":\"thread.started\",\"thread_id\":\"{}\"}}'\nprintf '%s\\n' '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"{}\"}}}}'\n",
            log.display(),
            session_id,
            agent_text_json
        );
        {
            let mut f = std::fs::File::create(bin).unwrap();
            f.write_all(script.as_bytes()).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(bin, perms).unwrap();
        }
    }

    fn write_cfg(path: &Path, json: &str) {
        std::fs::write(path, json).unwrap();
    }

    fn read_log(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    #[tokio::test]
    async fn session_reuse_uses_stored_backend_session_id() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let server = VibeServer::new(ConfigLoader::new(None), store.clone());

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        let script = format!(
            "#!/bin/sh\nset -e\n\n# append args each invocation\necho \"ARGS: $@\" >> \"{}\"\n\nif echo \"$@\" | grep -q 'resume sess-1'; then\n  sid='sess-2'\nelse\n  sid='sess-1'\nfi\n\necho '{{\"type\":\"thread.started\",\"thread_id\":\"'\"$sid\"'\"}}'\necho '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"ok\"}}}}'\n",
            log.display()
        );
        {
            let mut f = std::fs::File::create(&fake).unwrap();
            f.write_all(script.as_bytes()).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fake).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fake, perms).unwrap();
        }
        let _env = crate::test_support::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let args1 = VibeArgs {
            prompt: "first".to_string(),
            cd: repo.to_string_lossy().to_string(),
            role: Some("implementer".to_string()),
            brain: None,
            backend: Some("codex".to_string()),
            model: Some("gpt-5.2-codex".to_string()),
            reasoning_effort: Some("xhigh".to_string()),
            session_id: None,
            force_new_session: false,
            session_key: None,
            timeout_secs: Some(5),
            contract: None,
            validate_patch: false,
        };
        let out1 = server.run_vibe_internal(None, args1).await.unwrap();
        assert_eq!(out1.success, true);
        assert_eq!(out1.resumed, false);
        assert_eq!(out1.backend_session_id, "sess-1");

        let args2 = VibeArgs {
            prompt: "second".to_string(),
            cd: repo.to_string_lossy().to_string(),
            role: Some("implementer".to_string()),
            brain: None,
            backend: Some("codex".to_string()),
            model: Some("gpt-5.2-codex".to_string()),
            reasoning_effort: Some("xhigh".to_string()),
            session_id: None,
            force_new_session: false,
            session_key: None,
            timeout_secs: Some(5),
            contract: None,
            validate_patch: false,
        };
        let out2 = server.run_vibe_internal(None, args2).await.unwrap();
        assert_eq!(out2.success, true);
        assert_eq!(out2.resumed, true);
        assert_eq!(out2.backend_session_id, "sess-2");

        // Ensure resume was passed on second call
        let log_txt = std::fs::read_to_string(&log).unwrap();
        assert!(log_txt.lines().any(|l| l.contains("resume sess-1")));

        // Ensure store updated to latest session
        let brain_id = "codex:gpt-5.2-codex:xhigh";
        let key = SessionStore::compute_key(&repo.canonicalize().unwrap(), "implementer", brain_id);
        let rec = store.get(&key).unwrap().unwrap();
        assert_eq!(rec.backend_session_id, "sess-2");
    }

    #[tokio::test]
    async fn contract_patch_with_citations_fails_when_missing() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let server = VibeServer::new(ConfigLoader::new(None), store);

        let fake = td.path().join("fake-codex.sh");
        let script = "#!/bin/sh\nset -e\necho '{\"type\":\"thread.started\",\"thread_id\":\"sess-x\"}'\necho '{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"no patch here\"}}'\n";
        {
            let mut f = std::fs::File::create(&fake).unwrap();
            f.write_all(script.as_bytes()).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fake).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fake, perms).unwrap();
        }
        let _env = crate::test_support::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(None, VibeArgs {
                prompt: "do".to_string(),
                cd: repo.to_string_lossy().to_string(),
                role: Some("reviewer".to_string()),
                brain: None,
                backend: Some("codex".to_string()),
                model: None,
                reasoning_effort: None,
                session_id: None,
                force_new_session: true,
                session_key: None,
                timeout_secs: Some(5),
                contract: Some(OutputContract::PatchWithCitations),
                validate_patch: false,
            })
            .await
            .unwrap();

        assert_eq!(out.success, false);
        assert!(out.error.as_deref().unwrap_or("").contains("output contract violation"));
    }

    #[tokio::test]
    async fn contract_patch_validation_runs_git_apply_check() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {:?} failed: {}{}",
                args,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            out
        };

        run(&["init"]);
        std::fs::write(repo.join("hello.txt"), "hi\n").unwrap();
        run(&["add", "hello.txt"]);
        run(&[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "init",
        ]);

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let server = VibeServer::new(ConfigLoader::new(None), store);

        let fake = td.path().join("fake-codex.sh");

        // Patch applies to baseline hello.txt: hi -> hello
        let agent_text = "CITATIONS:\n- hello.txt:1\n\nPATCH:\n```diff\ndiff --git a/hello.txt b/hello.txt\n--- a/hello.txt\n+++ b/hello.txt\n@@ -1 +1 @@\n-hi\n+hello\n```\n";
        let agent_text_json = agent_text
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");

        let script = format!(
            "#!/bin/sh\nset -e\nprintf '%s\\n' '{{\"type\":\"thread.started\",\"thread_id\":\"sess-p\"}}'\nprintf '%s\\n' '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"{}\"}}}}'\n",
            agent_text_json
        );
        {
            let mut f = std::fs::File::create(&fake).unwrap();
            f.write_all(script.as_bytes()).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fake).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fake, perms).unwrap();
        }
        let _env = crate::test_support::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(None, VibeArgs {
                prompt: "do".to_string(),
                cd: repo.to_string_lossy().to_string(),
                role: Some("implementer".to_string()),
                brain: None,
                backend: Some("codex".to_string()),
                model: None,
                reasoning_effort: None,
                session_id: None,
                force_new_session: true,
                session_key: None,
                timeout_secs: Some(5),
                contract: Some(OutputContract::PatchWithCitations),
                validate_patch: true,
            })
            .await
            .unwrap();

        assert_eq!(out.success, true, "error={:?}", out.error);
        assert_eq!(out.patch_apply_check_ok, Some(true));
    }

    #[tokio::test]
    async fn role_policy_overrides_codex_sandbox_and_approval() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let user_cfg = td.path().join("user-config.json");
        std::fs::write(
            &user_cfg,
            r#"{
  "provider": {
    "codex": {
      "type": "codex-cli",
      "models": {
        "m": {"id":"gpt-5.2", "options": {"reasoningEffort":"high"}}
      }
    }
  },
  "roles": {
    "implementer": {
      "model": "codex.m",
      "policy": {"codex": {"sandbox": "workspace-write", "ask_for_approval": "never"}}
    }
  }
}"#,
        )
        .unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let loader = ConfigLoader::new(Some(user_cfg));
        let server = VibeServer::new(loader, store);

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        let script = format!(
            "#!/bin/sh\nset -e\n\n# log args for inspection\necho \"ARGS: $@\" >> \"{}\"\n\n# Emit minimal codex --json stream\necho '{{\"type\":\"thread.started\",\"thread_id\":\"sess-abc\"}}'\necho '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"ok\"}}}}'\n",
            log.display()
        );
        {
            let mut f = std::fs::File::create(&fake).unwrap();
            f.write_all(script.as_bytes()).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fake).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fake, perms).unwrap();
        }
        let _env = crate::test_support::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let _ = server
            .run_vibe_internal(None, VibeArgs {
                prompt: "hello".to_string(),
                cd: repo.to_string_lossy().to_string(),
                role: Some("implementer".to_string()),
                brain: None,
                backend: None,
                model: None,
                reasoning_effort: None,
                session_id: None,
                force_new_session: true,
                session_key: None,
                timeout_secs: Some(5),
                contract: None,
                validate_patch: false,
            })
            .await
            .unwrap();

        let log_txt = std::fs::read_to_string(&log).unwrap();
        assert!(log_txt.contains("-s workspace-write"));
        assert!(log_txt.contains("-a never"));
    }

    #[test]
    fn test_claude_backend_logic_compiles() {
        // This test ensures the Backend::Claude branch in run_vibe_internal
        // compiles correctly with the peer argument.
        // Real logic verification requires a full rmcp Client/Server setup.
    }

    #[tokio::test]
    async fn cfgtest_codex_role_config_returns_response() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("three-config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "codex": {
      "type": "codex-cli",
      "models": {
        "gpt-5.2-codex-xhigh": {"id":"gpt-5.2-codex", "options": {"reasoningEffort":"xhigh"}},
        "gpt-5.2-high": {"id":"gpt-5.2", "options": {"reasoningEffort":"high"}}
      }
    }
  },
  "roles": {
    "oracle": {
      "model": {"backend":"codex","model":"gpt-5.2-codex-xhigh"},
      "policy": {"codex": {"sandbox":"workspace-write", "ask_for_approval":"never", "skip_git_repo_check": true}}
    }
  }
}"#,
        );

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let server = VibeServer::new(ConfigLoader::new(Some(cfg_path)), store);

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        write_fake_codex(&fake, &log, "sess-cfg-1", "pong");
        let _env = crate::test_support::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(None, VibeArgs {
                prompt: "ping".to_string(),
                cd: repo.to_string_lossy().to_string(),
                role: Some("oracle".to_string()),
                brain: None,
                backend: None,
                model: None,
                reasoning_effort: None,
                session_id: None,
                force_new_session: true,
                session_key: None,
                timeout_secs: Some(5),
                contract: None,
                validate_patch: false,
            })
            .await
            .unwrap();

        assert!(out.success, "error={:?}", out.error);
        assert_eq!(out.backend, "codex");
        assert!(out.agent_messages.contains("pong"));
        assert_eq!(out.model.as_deref(), Some("gpt-5.2-codex"));
        assert_eq!(out.reasoning_effort.as_deref(), Some("xhigh"));

        let log_txt = read_log(&log);
        assert!(log_txt.contains("-s workspace-write"));
        assert!(log_txt.contains("-a never"));
        assert!(log_txt.contains("--skip-git-repo-check"));
        assert!(log_txt.contains("--model gpt-5.2-codex"));
        assert!(log_txt.contains("model_reasoning_effort=\"xhigh\""));
    }

    #[tokio::test]
    async fn cfgtest_codex_role_overrides_model_and_effort() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("three-config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "codex": {
      "type": "codex-cli",
      "models": {
        "gpt-5.2-high": {"id":"gpt-5.2", "options": {"reasoningEffort":"high"}},
        "gpt-5.2-codex-xhigh": {"id":"gpt-5.2-codex", "options": {"reasoningEffort":"xhigh"}}
      }
    }
  },
  "roles": {
    "sisyphus": {
      "model": "codex.gpt-5.2-high",
      "policy": {"codex": {"sandbox":"read-only", "ask_for_approval":"never"}}
    }
  }
}"#,
        );

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let server = VibeServer::new(ConfigLoader::new(Some(cfg_path)), store);

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        write_fake_codex(&fake, &log, "sess-cfg-2", "ok");
        let _env = crate::test_support::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(None, VibeArgs {
                prompt: "ping".to_string(),
                cd: repo.to_string_lossy().to_string(),
                role: Some("sisyphus".to_string()),
                brain: None,
                backend: None,
                model: Some("gpt-5.2-codex".to_string()),
                reasoning_effort: Some("xhigh".to_string()),
                session_id: None,
                force_new_session: true,
                session_key: None,
                timeout_secs: Some(5),
                contract: None,
                validate_patch: false,
            })
            .await
            .unwrap();

        assert!(out.success, "error={:?}", out.error);
        assert_eq!(out.backend, "codex");
        assert!(out.agent_messages.contains("ok"));
        assert_eq!(out.model.as_deref(), Some("gpt-5.2-codex"));
        assert_eq!(out.reasoning_effort.as_deref(), Some("xhigh"));

        let log_txt = read_log(&log);
        assert!(log_txt.contains("-s read-only"));
        assert!(log_txt.contains("-a never"));
        assert!(log_txt.contains("--skip-git-repo-check"));
        assert!(log_txt.contains("--model gpt-5.2-codex"));
        assert!(log_txt.contains("model_reasoning_effort=\"xhigh\""));
    }

    #[tokio::test]
    async fn cfgtest_codex_explicit_backend_without_config() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let server = VibeServer::new(ConfigLoader::new(None), store);

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        write_fake_codex(&fake, &log, "sess-cfg-3", "hi");
        let _env = crate::test_support::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(None, VibeArgs {
                prompt: "ping".to_string(),
                cd: repo.to_string_lossy().to_string(),
                role: Some("default".to_string()),
                brain: None,
                backend: Some("codex".to_string()),
                model: Some("gpt-5.2".to_string()),
                reasoning_effort: Some("high".to_string()),
                session_id: None,
                force_new_session: true,
                session_key: None,
                timeout_secs: Some(5),
                contract: None,
                validate_patch: false,
            })
            .await
            .unwrap();

        assert!(out.success, "error={:?}", out.error);
        assert_eq!(out.backend, "codex");
        assert!(out.agent_messages.contains("hi"));
        assert_eq!(out.model.as_deref(), Some("gpt-5.2"));
        assert_eq!(out.reasoning_effort.as_deref(), Some("high"));

        let log_txt = read_log(&log);
        assert!(log_txt.contains("-s read-only"));
        assert!(log_txt.contains("-a never"));
        assert!(log_txt.contains("--skip-git-repo-check"));
        assert!(log_txt.contains("--model gpt-5.2"));
        assert!(log_txt.contains("model_reasoning_effort=\"high\""));
    }
}
