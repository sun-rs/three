use crate::{
    backend,
    config::{
        parse_role_model_ref, resolve_model_options, AdapterConfig, Backend, ConfigLoader,
        OptionValue,
    },
    contract,
    personas::resolve_persona,
    session_store::{now_unix_secs, SessionRecord, SessionStore},
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, Peer, RoleServer,
    ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Input parameters for a single routed task (used by batch/roundtable internals).
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

    /// Optional client identifier (e.g., claude|codex|opencode)
    #[serde(default)]
    pub client: Option<String>,

    /// Optional main conversation/session identifier from the host CLI.
    #[serde(default)]
    pub conversation_id: Option<String>,
}

/// Input parameters for the roundtable tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoundtableArgs {
    /// Topic/question for the roundtable
    #[serde(rename = "TOPIC")]
    pub topic: String,

    /// Working directory (repo root recommended)
    pub cd: String,

    /// Participant list
    pub participants: Vec<RoundtableParticipant>,

    /// Default timeout in seconds for each participant (default: 600)
    #[serde(default)]
    pub timeout_secs: Option<u64>,

    /// Optional client identifier (e.g., claude|codex|opencode)
    #[serde(default)]
    pub client: Option<String>,

    /// Optional main conversation/session identifier from the host CLI.
    #[serde(default)]
    pub conversation_id: Option<String>,
}

/// Input parameters for the batch tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BatchArgs {
    /// Working directory (repo root recommended)
    pub cd: String,

    /// Task list (fan-out)
    pub tasks: Vec<BatchTask>,

    /// Default timeout in seconds for each task (default: 600)
    #[serde(default)]
    pub timeout_secs: Option<u64>,

    /// Optional client identifier (e.g., claude|codex|opencode)
    #[serde(default)]
    pub client: Option<String>,

    /// Optional main conversation/session identifier from the host CLI.
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct BatchTask {
    /// Task instruction
    #[serde(rename = "PROMPT")]
    pub prompt: String,

    /// Optional task label
    #[serde(default)]
    pub name: Option<String>,

    /// Role name (used in session key + optional config mapping)
    #[serde(default)]
    pub role: Option<String>,

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

/// Input parameters for the info tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InfoArgs {
    /// Working directory (repo root recommended)
    pub cd: String,

    /// Optional client identifier (e.g., claude|codex|opencode)
    #[serde(default)]
    pub client: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct RoundtableParticipant {
    pub name: String,

    #[serde(default)]
    pub role: Option<String>,

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
pub struct VibeOutput {
    pub success: bool,
    pub backend: String,
    pub role: String,
    pub role_id: String,
    pub model: Option<String>,
    pub session_key: String,
    pub resumed: bool,
    pub backend_session_id: String,
    pub agent_messages: String,
    pub warnings: Option<String>,
    pub contract: Option<String>,
    pub contract_errors: Vec<String>,
    pub patch_format: Option<String>,
    pub patch_apply_check_ok: Option<bool>,
    pub patch_apply_check_output: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BatchOutput {
    success: bool,
    cd: String,
    results: Vec<BatchResult>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BatchResult {
    name: Option<String>,
    role: String,
    backend: String,
    output: Option<VibeOutput>,
    error: Option<String>,
}

#[derive(Debug)]
struct FanoutTaskSpec {
    name: Option<String>,
    role: String,
    args: VibeArgs,
}

#[derive(Debug)]
struct FanoutResult {
    name: Option<String>,
    role: String,
    result: std::result::Result<VibeOutput, McpError>,
}

#[derive(Debug, Clone)]
struct ModelCandidate {
    backend_id: String,
    backend: Backend,
    adapter: AdapterConfig,
    model: String,
    options: BTreeMap<String, OptionValue>,
    ref_str: String,
    fallback_error_patterns: Vec<String>,
}

fn format_model_ref(backend_id: &str, model_id: &str, variant: Option<&str>) -> String {
    match variant {
        Some(v) if !v.trim().is_empty() => format!("{backend_id}/{model_id}@{v}"),
        _ => format!("{backend_id}/{model_id}"),
    }
}

fn compute_default_session_key(
    repo_root: &PathBuf,
    role: &str,
    role_id: &str,
    client_hint: Option<&str>,
    conversation_id: Option<&str>,
) -> String {
    SessionStore::compute_key_with_scope(repo_root, role, role_id, client_hint, conversation_id)
}

fn resolve_client_hint(explicit: Option<&str>) -> Result<Option<String>, McpError> {
    let raw = explicit
        .map(|s| s.to_string())
        .or_else(|| std::env::var("THREE_CLIENT").ok());
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(McpError::invalid_params(
            "client must be alphanumeric/underscore/dash".to_string(),
            None,
        ));
    }
    Ok(Some(trimmed.to_ascii_lowercase()))
}

fn resolve_conversation_hint(explicit: Option<&str>) -> Result<Option<String>, McpError> {
    let raw = explicit
        .map(|s| s.to_string())
        .or_else(|| std::env::var("THREE_CONVERSATION_ID").ok());
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > 256 {
        return Err(McpError::invalid_params(
            "conversation_id must be 256 chars or fewer".to_string(),
            None,
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':' || c == '.')
    {
        return Err(McpError::invalid_params(
            "conversation_id must use [A-Za-z0-9._:-]".to_string(),
            None,
        ));
    }
    Ok(Some(trimmed.to_string()))
}

async fn notify_fanout_progress(
    peer: Option<&Peer<RoleServer>>,
    logger: &str,
    level: LoggingLevel,
    message: String,
) {
    let Some(peer) = peer else {
        return;
    };

    let _ = peer
        .notify_logging_message(LoggingMessageNotificationParam {
            level,
            logger: Some(logger.to_string()),
            data: serde_json::Value::String(message),
        })
        .await;
}

#[derive(Debug, Serialize)]
struct RoundtableOutput {
    success: bool,
    topic: String,
    cd: String,
    contributions: Vec<RoundtableContribution>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct InfoOutput {
    success: bool,
    cd: String,
    config_sources: Vec<String>,
    roles: Vec<InfoRole>,
    warnings: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct InfoRole {
    role: String,
    backend: String,
    model: String,
    description: String,
    enabled: bool,
    prompt_present: bool,
    prompt_len: Option<usize>,
    prompt_preview: Option<String>,
}

#[derive(Debug, Serialize)]
struct RoundtableContribution {
    name: String,
    role: String,
    backend: String,
    role_id: String,
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
    /// Run multiple tasks in parallel and return partial results.
    #[tool(
        name = "batch",
        description = "Run multiple tasks in parallel with session reuse"
    )]
    async fn batch(
        &self,
        peer: Peer<RoleServer>,
        Parameters(args): Parameters<BatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let out = self.run_batch_internal(Some(peer), args).await?;
        let json = serde_json::to_string(&out).map_err(|e| {
            McpError::internal_error(format!("failed to serialize output: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Alias of `batch` with Roundtable-first naming.
    #[tool(
        name = "roundtable-batch",
        description = "Roundtable-first alias of batch: run multiple tasks in parallel with session reuse"
    )]
    async fn roundtable_batch(
        &self,
        peer: Peer<RoleServer>,
        Parameters(args): Parameters<BatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let out = self.run_batch_internal(Some(peer), args).await?;
        let json = serde_json::to_string(&out).map_err(|e| {
            McpError::internal_error(format!("failed to serialize output: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Alias of `batch` for hosts that prefer underscore tool naming.
    #[tool(
        name = "roundtable_batch",
        description = "Alias of roundtable-batch"
    )]
    async fn roundtable_batch_underscore(
        &self,
        peer: Peer<RoleServer>,
        Parameters(args): Parameters<BatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.roundtable_batch(peer, Parameters(args)).await
    }

    /// Run a multi-role discussion on a topic and return participant contributions.
    #[tool(
        name = "roundtable",
        description = "Fan-out a topic to multiple roles (conductor synthesizes externally)"
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
                format!(
                    "working directory is not a directory: {}",
                    repo_root.display()
                ),
                None,
            ));
        }

        let client_hint = resolve_client_hint(args.client.as_deref())?;
        let conversation_hint = resolve_conversation_hint(args.conversation_id.as_deref())?;
        let RoundtableArgs {
            topic,
            participants,
            timeout_secs,
            client: _client,
            conversation_id: _conversation_id,
            cd: _,
        } = args;

        let topic_trimmed = topic.trim().to_string();
        let repo_cd = repo_root.to_string_lossy().to_string();
        let timeout_override = timeout_secs;

        let mut tasks: Vec<FanoutTaskSpec> = Vec::new();
        for p in participants {
            if p.name.trim().is_empty() {
                return Err(McpError::invalid_params(
                    "participant.name must be non-empty",
                    None,
                ));
            }

            let name = p.name.trim().to_string();
            let role = p
                .role
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| name.clone());

            let prompt = format!(
                "TOPIC:
{}

You are a roundtable participant named '{}' (role: {}).

Reply with:
1) Position (1-2 sentences)
2) Arguments (bullets)
3) Risks/edge cases (bullets)
4) Recommendation (actionable)

Constraints:
- Do not claim to have run commands unless you actually did.
- Prefer referencing repo paths when relevant.
",
                topic_trimmed, name, role
            );

            let args = VibeArgs {
                prompt,
                cd: repo_cd.clone(),
                role: Some(role.clone()),
                backend: p.backend,
                model: p.model,
                reasoning_effort: p.reasoning_effort,
                session_id: None,
                force_new_session: p.force_new_session,
                session_key: None,
                timeout_secs: timeout_override,
                contract: None,
                validate_patch: false,
                client: client_hint.clone(),
                conversation_id: conversation_hint.clone(),
            };
            tasks.push(FanoutTaskSpec {
                name: Some(name),
                role,
                args,
            });
        }

        let results = self
            .run_fanout_internal(
                Some(peer.clone()),
                &repo_root,
                tasks,
                client_hint.clone(),
                conversation_hint.clone(),
                "roundtable",
            )
            .await?;

        let mut contributions = Vec::new();
        let mut any_error = false;
        for res in results {
            match res.result {
                Ok(out) => {
                    if out.error.is_some() {
                        any_error = true;
                    }
                    contributions.push(RoundtableContribution {
                        name: res.name.unwrap_or_default(),
                        role: out.role.clone(),
                        backend: out.backend.clone(),
                        role_id: out.role_id.clone(),
                        resumed: out.resumed,
                        backend_session_id: out.backend_session_id.clone(),
                        agent_messages: out.agent_messages.clone(),
                        error: out.error.clone(),
                    });
                }
                Err(e) => {
                    any_error = true;
                    contributions.push(RoundtableContribution {
                        name: res.name.unwrap_or_default(),
                        role: res.role,
                        backend: "error".to_string(),
                        role_id: "".to_string(),
                        resumed: false,
                        backend_session_id: "".to_string(),
                        agent_messages: "".to_string(),
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        let out = RoundtableOutput {
            success: !any_error,
            topic,
            cd: repo_root.to_string_lossy().to_string(),
            contributions,
            error: if any_error {
                Some("one or more participants returned an error".to_string())
            } else {
                None
            },
        };

        let json = serde_json::to_string(&out).map_err(|e| {
            McpError::internal_error(format!("failed to serialize output: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Show effective config (roles -> models) without calling any LLM.
    #[tool(
        name = "info",
        description = "Show effective three role/model mapping for this directory"
    )]
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
                format!(
                    "working directory is not a directory: {}",
                    repo_root.display()
                ),
                None,
            ));
        }

        let client_hint = resolve_client_hint(args.client.as_deref())?;
        let cfg_result = self
            .config_loader
            .load_for_repo_with_client(&repo_root, client_hint.as_deref())
            .map_err(|e| McpError::internal_error(format!("failed to load config: {e}"), None))?;
        let sources: Vec<String> = cfg_result
            .sources
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let cfg = cfg_result.config;

        let Some(cfg) = cfg else {
            let out = InfoOutput {
                success: false,
                cd: repo_root.to_string_lossy().to_string(),
                config_sources: sources,
                roles: Vec::new(),
                warnings: Vec::new(),
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
            let persona = resolve_persona(role_id, role_cfg.personas.as_ref());
            let description = persona
                .as_ref()
                .map(|p| p.description.clone())
                .unwrap_or_default();
            let prompt_raw = persona.as_ref().map(|p| p.prompt.trim()).unwrap_or("");
            let (prompt_present, prompt_len, prompt_preview) = if prompt_raw.is_empty() {
                (false, None, None)
            } else {
                let len = prompt_raw.len();
                let preview_len = 120usize;
                let preview = if len <= preview_len {
                    prompt_raw.to_string()
                } else {
                    format!("{}...", &prompt_raw[..preview_len])
                };
                (true, Some(len), Some(preview))
            };

            let enabled = role_cfg.enabled;
            if !enabled {
                let (backend_id, model_id, _variant) = match parse_role_model_ref(&role_cfg.model) {
                    Ok(parts) => parts,
                    Err(e) => {
                        errors.push(format!("role '{role_id}' invalid: {e}"));
                        continue;
                    }
                };
                roles.push(InfoRole {
                    role: role_id.to_string(),
                    backend: backend_id,
                    model: model_id,
                    description,
                    enabled,
                    prompt_present,
                    prompt_len,
                    prompt_preview,
                });
                continue;
            }

            let resolved = match cfg.resolve_profile(Some(role_id)) {
                Ok(r) => r,
                Err(e) => {
                    errors.push(format!("role '{role_id}' invalid: {e}"));
                    continue;
                }
            };

            roles.push(InfoRole {
                role: role_id.to_string(),
                backend: resolved.profile.backend_id.clone(),
                model: resolved.profile.model.clone(),
                description,
                enabled,
                prompt_present,
                prompt_len,
                prompt_preview,
            });
        }

        let warnings = {
            let kimi_roles: Vec<String> = roles
                .iter()
                .filter(|r| r.enabled && r.backend == "kimi")
                .map(|r| r.role.clone())
                .collect();
            if kimi_roles.len() > 1 {
                vec![format!(
                    "multiple enabled roles use backend 'kimi' ({}). Parallel resume (batch/roundtable with force_new_session=false) will be rejected.",
                    kimi_roles.join(", ")
                )]
            } else {
                Vec::new()
            }
        };

        let out = InfoOutput {
            success: errors.is_empty(),
            cd: repo_root.to_string_lossy().to_string(),
            config_sources: sources,
            roles,
            warnings,
            error: if errors.is_empty() {
                None
            } else {
                Some(errors.join("; "))
            },
        };

        let json = serde_json::to_string(&out).map_err(|e| {
            McpError::internal_error(format!("failed to serialize output: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

impl VibeServer {
    pub async fn run_vibe_internal(
        &self,
        _peer: Option<Peer<RoleServer>>,
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
                format!(
                    "working directory is not a directory: {}",
                    repo_root.display()
                ),
                None,
            ));
        }

        let role = args.role.clone().unwrap_or_else(|| "default".to_string());
        let client_hint = resolve_client_hint(args.client.as_deref())?;
        let conversation_hint = resolve_conversation_hint(args.conversation_id.as_deref())?;

        let cfg_for_repo = self
            .config_loader
            .load_for_repo_with_client(&repo_root, client_hint.as_deref())
            .map_err(|e| McpError::internal_error(format!("failed to load config: {e}"), None))?;
        let cfg = cfg_for_repo.config.ok_or_else(|| {
            McpError::invalid_params("no config found (create ~/.config/three/config.json)", None)
        })?;

        let rp = cfg
            .resolve_profile(args.role.as_deref())
            .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
        let mut prompt_text = args.prompt.clone();

        let session_key = args
            .session_key
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                compute_default_session_key(
                    &repo_root,
                    &role,
                    &rp.role_id,
                    client_hint.as_deref(),
                    conversation_hint.as_deref(),
                )
            });
        let _key_lock = self
            .store
            .acquire_key_lock(&session_key)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let timeout_secs = args.timeout_secs.or(rp.profile.timeout_secs).unwrap_or(600);

        let explicit_session_id = args
            .session_id
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let mut warning_messages: Vec<String> = Vec::new();

        let prev_rec = self.store.get(&session_key).ok().flatten();
        let supports_session = rp.profile.adapter.output_parser.supports_session();
        let mut resumed = false;
        let mut session_id_to_use = if args.force_new_session {
            if let Some(sid) = explicit_session_id.as_ref() {
                warning_messages.push(format!(
                    "force_new_session=true ignored provided session_id '{sid}'"
                ));
            }
            None
        } else {
            explicit_session_id.clone()
        };
        let mut resume_without_session = false;
        if !args.force_new_session
            && args.session_key.is_none()
            && explicit_session_id.is_none()
            && conversation_hint.is_none()
        {
            warning_messages.push(
                "conversation_id not provided; auto-resume may cross top-level chats in the same repo/role".to_string(),
            );
        }

        if session_id_to_use.is_none() && !args.force_new_session {
            if supports_session {
                if let Some(rec) = prev_rec.as_ref() {
                    if rec.backend == rp.profile.backend {
                        let prev_id = rec.backend_session_id.trim();
                        if !prev_id.is_empty() && prev_id != "stateless" {
                            session_id_to_use = Some(rec.backend_session_id.clone());
                            resumed = true;
                        }
                    }
                }
            } else if rp.profile.backend_id == "kimi" {
                if let Some(rec) = prev_rec.as_ref() {
                    if rec.backend == rp.profile.backend {
                        resume_without_session = true;
                        resumed = true;
                    }
                }
            }
        }

        let role_cfg = cfg.roles.get(&role).ok_or_else(|| {
            McpError::internal_error(format!("role '{role}' missing after resolve"), None)
        })?;
        let persona = resolve_persona(&role, role_cfg.personas.as_ref());

        let is_resuming = !args.force_new_session && (explicit_session_id.is_some() || resumed);
        if !is_resuming && !prompt_text.contains("[THREE_PERSONA") {
            let ptext = persona.as_ref().map(|p| p.prompt.trim()).unwrap_or("");
            if !ptext.is_empty() {
                let bid = rp.role_id.as_str();
                prompt_text = format!(
                    "[THREE_PERSONA id={bid}]
{ptext}
[/THREE_PERSONA]

{prompt_text}"
                );
            }
        }

        let backend_cfg = cfg.backend.get(&rp.profile.backend_id).ok_or_else(|| {
            McpError::internal_error(
                format!("missing backend config: {}", rp.profile.backend_id),
                None,
            )
        })?;
        let mut candidates: Vec<ModelCandidate> = Vec::new();
        let fallback_patterns = backend_cfg
            .fallback
            .as_ref()
            .map(|f| f.patterns.clone())
            .unwrap_or_default();
        candidates.push(ModelCandidate {
            backend_id: rp.profile.backend_id.clone(),
            backend: rp.profile.backend,
            adapter: rp.profile.adapter.clone(),
            model: rp.profile.model.clone(),
            options: rp.profile.options.clone(),
            ref_str: format_model_ref(&rp.profile.backend_id, &rp.profile.model, None),
            fallback_error_patterns: fallback_patterns,
        });
        if let Some(fallback) = backend_cfg.fallback.as_ref() {
            let (backend_id, model_id, variant) = parse_role_model_ref(&fallback.model)
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
            let fallback_backend = Backend::parse(&backend_id).ok_or_else(|| {
                McpError::invalid_params(format!("unsupported backend key: {backend_id}"), None)
            })?;
            let fallback_cfg = cfg.backend.get(&backend_id).ok_or_else(|| {
                McpError::invalid_params(format!("missing backend config: {backend_id}"), None)
            })?;
            let adapter = fallback_cfg.adapter.clone().ok_or_else(|| {
                McpError::invalid_params(
                    format!("missing adapter config for backend: {backend_id}"),
                    None,
                )
            })?;
            let compatible = adapter
                .filesystem_capabilities
                .as_ref()
                .map(|allowed| allowed.contains(&rp.profile.capabilities.filesystem))
                .unwrap_or(true);
            if compatible {
                let options = if model_id == "default" {
                    if variant.is_some() {
                        return Err(McpError::invalid_params(
                            "fallback model 'default' does not support variants".to_string(),
                            None,
                        ));
                    }
                    if let Some(model_cfg) = fallback_cfg.models.get("default") {
                        resolve_model_options(model_cfg, None)
                            .map_err(|e| McpError::invalid_params(e.to_string(), None))?
                    } else {
                        BTreeMap::new()
                    }
                } else {
                    let model_cfg = fallback_cfg.models.get(&model_id).ok_or_else(|| {
                        McpError::invalid_params(
                            format!(
                                "unknown fallback model '{model_id}' for backend '{backend_id}'"
                            ),
                            None,
                        )
                    })?;
                    resolve_model_options(model_cfg, variant.as_deref())
                        .map_err(|e| McpError::invalid_params(e.to_string(), None))?
                };
                let ref_str = format_model_ref(&backend_id, &model_id, variant.as_deref());
                let candidate_patterns = fallback_cfg
                    .fallback
                    .as_ref()
                    .map(|f| f.patterns.clone())
                    .unwrap_or_default();
                candidates.push(ModelCandidate {
                    backend_id,
                    backend: fallback_backend,
                    adapter,
                    model: model_id,
                    options,
                    ref_str,
                    fallback_error_patterns: candidate_patterns,
                });
            }
        }

        let mut used_fallback: Option<String> = None;
        let mut last_err: Option<String> = None;
        let mut result: Option<backend::GenericResult> = None;
        let mut used_backend_id = rp.profile.backend_id.clone();
        let mut used_backend = rp.profile.backend;
        let mut used_model = rp.profile.model.clone();
        let total_candidates = candidates.len();
        for (idx, candidate) in candidates.into_iter().enumerate() {
            let same_backend = candidate.backend_id == rp.profile.backend_id;
            let out = backend::run(backend::GenericOptions {
                backend_id: candidate.backend_id.clone(),
                adapter: candidate.adapter.clone(),
                prompt: prompt_text.clone(),
                workdir: repo_root.clone(),
                session_id: if same_backend {
                    session_id_to_use.clone()
                } else {
                    None
                },
                resume: if same_backend {
                    resume_without_session
                } else {
                    false
                },
                model: candidate.model.clone(),
                options: candidate.options,
                capabilities: rp.profile.capabilities.clone(),
                fallback_error_patterns: candidate.fallback_error_patterns.clone(),
                timeout_secs,
            })
            .await;
            match out {
                Ok(r) => {
                    if idx > 0 {
                        used_fallback = Some(candidate.ref_str.clone());
                    }
                    used_backend_id = candidate.backend_id;
                    used_backend = candidate.backend;
                    used_model = candidate.model;
                    result = Some(r);
                    break;
                }
                Err(e) => {
                    let msg = e.to_string();
                    last_err = Some(msg.clone());
                    if is_model_error_message(&msg) && idx + 1 < total_candidates {
                        continue;
                    }
                    return Err(McpError::internal_error(
                        format!("backend failed: {msg}"),
                        None,
                    ));
                }
            }
        }

        let r = result.ok_or_else(|| {
            McpError::internal_error(
                format!(
                    "backend failed: {}",
                    last_err.unwrap_or_else(|| "unknown error".to_string())
                ),
                None,
            )
        })?;

        let backend_session_id = r.session_id;
        let agent_messages = r.agent_messages;
        let fallback_warning = used_fallback
            .as_ref()
            .map(|m| format!("model fallback used: {m}"));
        let warning_extra = if warning_messages.is_empty() {
            None
        } else {
            Some(warning_messages.join("\n"))
        };
        let warnings = match (r.warnings, warning_extra, fallback_warning) {
            (Some(base), Some(extra), Some(fallback)) => {
                Some(format!("{base}\n{extra}\n{fallback}"))
            }
            (Some(base), Some(extra), None) => Some(format!("{base}\n{extra}")),
            (Some(base), None, Some(fallback)) => Some(format!("{base}\n{fallback}")),
            (Some(base), None, None) => Some(base),
            (None, Some(extra), Some(fallback)) => Some(format!("{extra}\n{fallback}")),
            (None, Some(extra), None) => Some(extra),
            (None, None, Some(fallback)) => Some(fallback),
            (None, None, None) => None,
        };

        self.store
            .put(
                &session_key,
                SessionRecord {
                    repo_root: repo_root.to_string_lossy().to_string(),
                    role: role.clone(),
                    role_id: rp.role_id.clone(),
                    backend: used_backend,
                    backend_session_id: backend_session_id.clone(),
                    sampling_history: Vec::new(),
                    updated_at_unix_secs: now_unix_secs(),
                },
            )
            .map_err(|e| {
                McpError::internal_error(format!("failed to persist session: {e}"), None)
            })?;

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
                            "validate_patch=true but failed to extract unified diff patch"
                                .to_string(),
                        );
                    }
                    _ => {
                        patch_apply_check_ok = Some(false);
                        patch_apply_check_output =
                            Some("validate_patch=true but patch is not a unified diff".to_string());
                    }
                }
            }

            if !contract_errors.is_empty() {
                error = Some(format!(
                    "output contract violation: {}",
                    contract_errors.join(", ")
                ));
            }
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
            backend: used_backend_id,
            role,
            role_id: rp.role_id,
            model: Some(used_model),
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

    async fn run_batch_internal(
        &self,
        peer: Option<Peer<RoleServer>>,
        args: BatchArgs,
    ) -> Result<BatchOutput, McpError> {
        if args.cd.trim().is_empty() {
            return Err(McpError::invalid_params(
                "cd is required and must be a non-empty string",
                None,
            ));
        }
        if args.tasks.is_empty() {
            return Err(McpError::invalid_params(
                "tasks must be a non-empty array",
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
                format!(
                    "working directory is not a directory: {}",
                    repo_root.display()
                ),
                None,
            ));
        }

        let repo_cd = repo_root.to_string_lossy().to_string();
        let client_hint = resolve_client_hint(args.client.as_deref())?;
        let conversation_hint = resolve_conversation_hint(args.conversation_id.as_deref())?;
        let mut tasks: Vec<FanoutTaskSpec> = Vec::with_capacity(args.tasks.len());
        for task in args.tasks {
            let role_opt = task
                .role
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let role_label = role_opt
                .clone()
                .unwrap_or_else(|| "<missing role>".to_string());
            let timeout_secs = task.timeout_secs.or(args.timeout_secs);
            let args = VibeArgs {
                prompt: task.prompt,
                cd: repo_cd.clone(),
                role: role_opt,
                backend: task.backend,
                model: task.model,
                reasoning_effort: task.reasoning_effort,
                session_id: task.session_id,
                force_new_session: task.force_new_session,
                session_key: task.session_key,
                timeout_secs,
                contract: task.contract,
                validate_patch: task.validate_patch,
                client: client_hint.clone(),
                conversation_id: conversation_hint.clone(),
            };
            tasks.push(FanoutTaskSpec {
                name: task.name,
                role: role_label,
                args,
            });
        }

        let results = self
            .run_fanout_internal(
                peer,
                &repo_root,
                tasks,
                client_hint.clone(),
                conversation_hint.clone(),
                "batch",
            )
            .await?;

        let mut any_error = false;
        let mut outputs: Vec<BatchResult> = Vec::new();
        for res in results {
            match res.result {
                Ok(out) => {
                    if out.error.is_some() {
                        any_error = true;
                    }
                    let err = out.error.clone();
                    let backend = out.backend.clone();
                    outputs.push(BatchResult {
                        name: res.name,
                        role: res.role,
                        backend,
                        output: Some(out),
                        error: err,
                    });
                }
                Err(e) => {
                    any_error = true;
                    outputs.push(BatchResult {
                        name: res.name,
                        role: res.role,
                        backend: "error".to_string(),
                        output: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        Ok(BatchOutput {
            success: !any_error,
            cd: repo_root.to_string_lossy().to_string(),
            results: outputs,
            error: if any_error {
                Some("one or more tasks returned an error".to_string())
            } else {
                None
            },
        })
    }

    async fn run_fanout_internal(
        &self,
        peer: Option<Peer<RoleServer>>,
        repo_root: &PathBuf,
        tasks: Vec<FanoutTaskSpec>,
        client: Option<String>,
        conversation_id: Option<String>,
        operation: &'static str,
    ) -> Result<Vec<FanoutResult>, McpError> {
        let cfg_for_repo = self
            .config_loader
            .load_for_repo_with_client(repo_root, client.as_deref())
            .map_err(|e| McpError::internal_error(format!("failed to load config: {e}"), None))?;
        let cfg = cfg_for_repo.config.ok_or_else(|| {
            McpError::invalid_params("no config found (create ~/.config/three/config.json)", None)
        })?;

        let mut kimi_resume_roles: Vec<String> = Vec::new();
        for task in &tasks {
            if task.args.force_new_session {
                continue;
            }
            let Some(role_id) = task.args.role.as_deref() else {
                continue;
            };
            let resolved = match cfg.resolve_profile(Some(role_id)) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if resolved.profile.backend_id != "kimi" {
                continue;
            }

            let explicit_session_id = task
                .args
                .session_id
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if explicit_session_id.is_some() {
                kimi_resume_roles.push(role_id.to_string());
                continue;
            }

            let effective_client = task.args.client.as_deref().or(client.as_deref());
            let effective_conversation = task
                .args
                .conversation_id
                .as_deref()
                .or(conversation_id.as_deref());
            let session_key = task
                .args
                .session_key
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    compute_default_session_key(
                        repo_root,
                        role_id,
                        &resolved.role_id,
                        effective_client,
                        effective_conversation,
                    )
                });
            let prev_rec = self.store.get(&session_key).ok().flatten();
            if let Some(rec) = prev_rec {
                if rec.backend == resolved.profile.backend {
                    kimi_resume_roles.push(role_id.to_string());
                }
            }
        }

        if kimi_resume_roles.len() > 1 {
            return Err(McpError::invalid_params(
                format!(
                    "multiple kimi roles requested with force_new_session=false: {}. Kimi cannot resume multiple sessions in the same working directory.",
                    kimi_resume_roles.join(", ")
                ),
                None,
            ));
        }

        let logger = format!("three.{operation}");
        let total_tasks = tasks.len();
        let mut joinset: tokio::task::JoinSet<FanoutResult> = tokio::task::JoinSet::new();
        for (idx, task) in tasks.into_iter().enumerate() {
            let task_label = task.name.clone().unwrap_or_else(|| task.role.clone());
            notify_fanout_progress(
                peer.as_ref(),
                &logger,
                LoggingLevel::Info,
                format!(
                    "[{operation}] started {task_label} ({}/{})",
                    idx + 1,
                    total_tasks
                ),
            )
            .await;

            let server = VibeServer::new(self.config_loader.clone(), self.store.clone());
            let peer = peer.clone();
            let FanoutTaskSpec { name, role, args } = task;
            joinset.spawn(async move {
                let out = server.run_vibe_internal(peer, args).await;
                FanoutResult {
                    name,
                    role,
                    result: out,
                }
            });
        }

        let mut results: Vec<FanoutResult> = Vec::with_capacity(total_tasks);
        let mut completed = 0usize;
        while let Some(joined) = joinset.join_next().await {
            completed += 1;
            match joined {
                Ok(res) => {
                    let task_label = res
                        .name
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| {
                            if res.role.trim().is_empty() {
                                "<unknown>".to_string()
                            } else {
                                res.role.clone()
                            }
                        });
                    let status = match &res.result {
                        Ok(out) if out.error.is_none() => "ok",
                        _ => "error",
                    };
                    notify_fanout_progress(
                        peer.as_ref(),
                        &logger,
                        if status == "ok" {
                            LoggingLevel::Info
                        } else {
                            LoggingLevel::Warning
                        },
                        format!(
                            "[{operation}] completed {task_label} ({}/{}) status={status}",
                            completed, total_tasks
                        ),
                    )
                    .await;
                    results.push(res);
                }
                Err(e) => {
                    notify_fanout_progress(
                        peer.as_ref(),
                        &logger,
                        LoggingLevel::Error,
                        format!(
                            "[{operation}] join error ({}/{}) {}",
                            completed, total_tasks, e
                        ),
                    )
                    .await;
                    results.push(FanoutResult {
                        name: None,
                        role: "".to_string(),
                        result: Err(McpError::internal_error(format!("join error: {e}"), None)),
                    });
                }
            }
        }

        Ok(results)
    }
}

fn is_model_error_message(msg: &str) -> bool {
    msg.trim_start()
        .to_ascii_lowercase()
        .starts_with("model_not_found:")
}

#[tool_handler]
impl ServerHandler for VibeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "This server provides 'roundtable' (core), 'batch'/'roundtable-batch' (fan-out), and 'info' tools for multi-role orchestration."
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Backend;
    use std::io::Write;
    use std::path::Path;
    use std::process::Command;

    // Note: tests call `run_vibe_internal` directly.

    fn write_fake_cli(bin: &Path, log: &Path, session_id: &str, agent_text: &str) {
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

    fn read_log(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    fn read_log_args(path: &Path) -> Vec<String> {
        let raw = std::fs::read(path).unwrap_or_default();
        raw.split(|b| *b == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| String::from_utf8_lossy(chunk).to_string())
            .collect()
    }

    fn write_codex_test_config(path: &Path) {
        let cfg = r#"{
  "backend": {
    "codex": {
      "models": {
        "gpt-5.2-codex": {
          "options": { "model_reasoning_effort": "high" },
          "variants": { "xhigh": { "model_reasoning_effort": "xhigh" } }
        }
      }
    }
  },
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2-codex@xhigh",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    }
  }
}"#;
        std::fs::write(path, cfg).unwrap();
    }

    fn write_codex_test_config_with_model(path: &Path, model_id: &str) {
        let cfg = format!(
            r#"{{
  "backend": {{
    "codex": {{
      "models": {{
        "{model_id}": {{ "options": {{}} }}
      }}
    }}
  }},
  "roles": {{
    "oracle": {{
      "model": "codex/{model_id}",
      "personas": {{ "description": "d", "prompt": "p" }},
      "capabilities": {{ "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }}
    }}
  }}
}}"#,
            model_id = model_id
        );
        std::fs::write(path, cfg).unwrap();
    }

    fn write_batch_codex_config(path: &Path) {
        let cfg = r#"{
  "backend": {
    "codex": {
      "models": {
        "gpt-5.2-codex": { "options": {} }
      }
    }
  },
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "builder": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    }
  }
}"#;
        std::fs::write(path, cfg).unwrap();
    }

    fn write_batch_kimi_config(path: &Path) {
        let cfg = r#"{
  "backend": {
    "kimi": { "models": {} }
  },
  "roles": {
    "kimi_a": {
      "model": "kimi/default",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "kimi_b": {
      "model": "kimi/default",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    }
  }
}"#;
        std::fs::write(path, cfg).unwrap();
    }

    fn write_codex_prompt_transport_config(path: &Path, max_chars: usize) {
        let cfg = format!(
            r#"{{
  "backend": {{
    "codex": {{
      "adapter": {{
        "args_template": [
          "exec",
          "{{% if not session_id and model != 'default' %}}--model{{% endif %}}",
          "{{% if not session_id and model != 'default' %}}{{{{ model }}}}{{% endif %}}",
          "--skip-git-repo-check",
          "{{% if not session_id %}}-C{{% endif %}}",
          "{{% if not session_id %}}{{{{ workdir }}}}{{% endif %}}",
          "--json",
          "{{% if session_id %}}resume{{% endif %}}",
          "{{% if session_id %}}{{{{ session_id }}}}{{% endif %}}",
          "{{% if prompt %}}{{{{ prompt }}}}{{% endif %}}"
        ],
        "output_parser": {{
          "type": "json_stream",
          "session_id_path": "thread_id",
          "message_path": "item.text",
          "pick": "last"
        }},
        "prompt_transport": "auto",
        "prompt_max_chars": {max_chars}
      }},
      "models": {{
        "gpt-5.2-codex": {{}}
      }}
    }}
  }},
  "roles": {{
    "oracle": {{
      "model": "codex/gpt-5.2-codex",
      "personas": {{ "description": "d", "prompt": "p" }},
      "capabilities": {{ "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }}
    }}
  }}
}}"#,
            max_chars = max_chars
        );
        std::fs::write(path, cfg).unwrap();
    }

    fn write_codex_fallback_config(path: &Path) {
        let cfg = r#"{
  "backend": {
    "codex": {
      "fallback": {
        "model": "codex/gpt-5.2",
        "patterns": ["model_not_found"]
      },
      "models": {
        "gpt-5.2-codex": { "options": {} },
        "gpt-5.2": { "options": {} }
      }
    }
  },
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    }
  }
}"#;
        std::fs::write(path, cfg).unwrap();
    }

    fn write_codex_fallback_pattern_config(path: &Path) {
        let cfg = r#"{
  "backend": {
    "codex": {
      "fallback": {
        "model": "codex/gpt-5.2",
        "patterns": ["custom_not_found"]
      },
      "models": {
        "gpt-5.2-codex": { "options": {} },
        "gpt-5.2": { "options": {} }
      }
    }
  },
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    }
  }
}"#;
        std::fs::write(path, cfg).unwrap();
    }

    fn write_fake_cli_with_arg_log(bin: &Path, log: &Path, session_id: &str) {
        let script = format!(
            "#!/bin/sh\nset -e\n\nprintf '%s\\0' \"$@\" > \"{}\"\n\nprintf '%s\\n' '{{\"type\":\"thread.started\",\"thread_id\":\"{}\"}}'\nprintf '%s\\n' '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"ok\"}}}}'\n",
            log.display(),
            session_id
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

    fn write_fake_cli_with_arg_and_stdin_log(
        bin: &Path,
        args_log: &Path,
        stdin_log: &Path,
        session_id: &str,
    ) {
        let script = format!(
            "#!/bin/sh\nset -e\n\ncat - > \"{}\"\nprintf '%s\\0' \"$@\" > \"{}\"\n\nprintf '%s\\n' '{{\"type\":\"thread.started\",\"thread_id\":\"{}\"}}'\nprintf '%s\\n' '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"ok\"}}}}'\n",
            stdin_log.display(),
            args_log.display(),
            session_id
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

    fn write_fake_cli_with_model_error_once(bin: &Path, log: &Path, bad_model: &str) {
        let script = format!(
            "#!/bin/sh\nset -e\n\nprintf '%s ' \"$@\" >> \"{}\"\nprintf '\\n' >> \"{}\"\n\nif echo \"$@\" | grep -q '{}'; then\n  printf '%s\\n' '{{\"type\":\"error\",\"message\":\"model_not_found\"}}'\n  exit 0\nfi\n\nprintf '%s\\n' '{{\"type\":\"thread.started\",\"thread_id\":\"sess-1\"}}'\nprintf '%s\\n' '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"ok\"}}}}'\n",
            log.display(),
            log.display(),
            bad_model
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

    fn write_fake_cli_with_custom_model_error_once(
        bin: &Path,
        log: &Path,
        bad_model: &str,
        message: &str,
    ) {
        let msg = message
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        let script = format!(
            "#!/bin/sh\nset -e\n\nprintf '%s ' \"$@\" >> \"{}\"\nprintf '\\n' >> \"{}\"\n\nif echo \"$@\" | grep -q '{}'; then\n  printf '%s\\n' '{{\"type\":\"error\",\"message\":\"{}\"}}'\n  exit 0\nfi\n\nprintf '%s\\n' '{{\"type\":\"thread.started\",\"thread_id\":\"sess-1\"}}'\nprintf '%s\\n' '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"ok\"}}}}'\n",
            log.display(),
            log.display(),
            bad_model,
            msg
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

    fn codex_loader(cfg_path: &Path) -> ConfigLoader {
        ConfigLoader::new(Some(cfg_path.to_path_buf()))
    }

    #[tokio::test]
    async fn client_config_prefers_client_specific_file() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store = SessionStore::new(td.path().join("sessions.json"));

        let cfg_path = td.path().join("config.json");
        write_codex_test_config_with_model(&cfg_path, "gpt-5.2-codex");
        let cfg_client = td.path().join("config-claude.json");
        write_codex_test_config_with_model(&cfg_client, "gpt-5.3-codex");

        let server = VibeServer::new(codex_loader(&cfg_path), store);

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        write_fake_cli(&fake, &log, "sess-cfg-1", "pong");
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: "ping".to_string(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: None,
                    force_new_session: true,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: None,
                    validate_patch: false,
                    client: Some("claude".to_string()),

                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(out.model.as_deref(), Some("gpt-5.3-codex"));
    }

    #[tokio::test]
    async fn session_reuse_uses_stored_backend_session_id() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let cfg_path = td.path().join("config.json");
        write_codex_test_config(&cfg_path);
        let server = VibeServer::new(codex_loader(&cfg_path), store.clone());

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
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let args1 = VibeArgs {
            prompt: "first".to_string(),
            cd: repo.to_string_lossy().to_string(),
            role: Some("oracle".to_string()),
            backend: None,
            model: None,
            reasoning_effort: None,
            session_id: None,
            force_new_session: false,
            session_key: None,
            timeout_secs: Some(5),
            contract: None,
            validate_patch: false,
            client: None,

            conversation_id: None,
        };
        let out1 = server.run_vibe_internal(None, args1).await.unwrap();
        assert_eq!(out1.success, true);
        assert_eq!(out1.resumed, false);
        assert_eq!(out1.backend_session_id, "sess-1");

        let args2 = VibeArgs {
            prompt: "second".to_string(),
            cd: repo.to_string_lossy().to_string(),
            role: Some("oracle".to_string()),
            backend: None,
            model: None,
            reasoning_effort: None,
            session_id: None,
            force_new_session: false,
            session_key: None,
            timeout_secs: Some(5),
            contract: None,
            validate_patch: false,
            client: None,

            conversation_id: None,
        };
        let out2 = server.run_vibe_internal(None, args2).await.unwrap();
        assert_eq!(out2.success, true);
        assert_eq!(out2.resumed, true);
        assert_eq!(out2.backend_session_id, "sess-2");

        let log_txt = std::fs::read_to_string(&log).unwrap();
        assert!(log_txt.lines().any(|l| l.contains("resume sess-1")));

        let role_id = "oracle";
        let key = SessionStore::compute_key(&repo.canonicalize().unwrap(), role_id, role_id);
        let rec = store.get(&key).unwrap().unwrap();
        assert_eq!(rec.backend_session_id, "sess-2");
    }

    #[tokio::test]
    async fn session_resume_skips_persona_injection() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let cfg_path = td.path().join("config.json");
        write_codex_test_config(&cfg_path);
        let server = VibeServer::new(codex_loader(&cfg_path), store.clone());

        let fake_first = td.path().join("fake-codex-first.sh");
        let log_first = td.path().join("codex-first.log");
        write_fake_cli_with_arg_log(&fake_first, &log_first, "sess-1");
        {
            let _env = crate::test_utils::scoped_codex_bin(fake_first.to_string_lossy().as_ref());
            let out1 = server
                .run_vibe_internal(
                    None,
                    VibeArgs {
                        prompt: "first".to_string(),
                        cd: repo.to_string_lossy().to_string(),
                        role: Some("oracle".to_string()),
                        backend: None,
                        model: None,
                        reasoning_effort: None,
                        session_id: None,
                        force_new_session: false,
                        session_key: None,
                        timeout_secs: Some(5),
                        contract: None,
                        validate_patch: false,
                        client: None,

                        conversation_id: None,
                    },
                )
                .await
                .unwrap();
            assert!(!out1.resumed);
        }

        let fake_second = td.path().join("fake-codex-second.sh");
        let log_second = td.path().join("codex-second.log");
        write_fake_cli_with_arg_log(&fake_second, &log_second, "sess-2");
        {
            let _env = crate::test_utils::scoped_codex_bin(fake_second.to_string_lossy().as_ref());
            let out2 = server
                .run_vibe_internal(
                    None,
                    VibeArgs {
                        prompt: "second".to_string(),
                        cd: repo.to_string_lossy().to_string(),
                        role: Some("oracle".to_string()),
                        backend: None,
                        model: None,
                        reasoning_effort: None,
                        session_id: None,
                        force_new_session: false,
                        session_key: None,
                        timeout_secs: Some(5),
                        contract: None,
                        validate_patch: false,
                        client: None,

                        conversation_id: None,
                    },
                )
                .await
                .unwrap();
            assert!(out2.resumed);
        }

        let args = read_log_args(&log_second);
        let prompt_arg = args.last().cloned().unwrap_or_default();
        assert!(prompt_arg.contains("second"));
        assert!(
            !prompt_arg.contains("[THREE_PERSONA"),
            "prompt should not include persona on resume: {prompt_arg}"
        );
    }

    #[tokio::test]
    async fn force_new_session_ignores_session_id_and_warns() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let cfg_path = td.path().join("config.json");
        write_codex_test_config(&cfg_path);
        let server = VibeServer::new(codex_loader(&cfg_path), store);

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        write_fake_cli_with_arg_log(&fake, &log, "sess-new");
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: "fresh".to_string(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: Some("sess-123".to_string()),
                    force_new_session: true,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: None,
                    validate_patch: false,
                    client: None,

                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        assert!(!out.resumed);
        let warn = out.warnings.unwrap_or_default();
        assert!(warn.contains("force_new_session=true"));
        assert!(warn.contains("sess-123"));

        let args = read_log_args(&log);
        assert!(!args.iter().any(|v| v == "resume"), "args={args:?}");
    }

    #[tokio::test]
    async fn codex_auto_prompt_uses_stdin_for_long_prompt() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_codex_prompt_transport_config(&cfg_path, 4);
        let store = SessionStore::new(td.path().join("sessions.json"));
        let server = VibeServer::new(codex_loader(&cfg_path), store);

        let fake = td.path().join("fake-codex.sh");
        let args_log = td.path().join("codex-args.log");
        let stdin_log = td.path().join("codex-stdin.log");
        write_fake_cli_with_arg_and_stdin_log(&fake, &args_log, &stdin_log, "sess-1");
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let long_prompt = "12345".to_string();
        server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: long_prompt.clone(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: None,
                    force_new_session: true,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: None,
                    validate_patch: false,
                    client: None,

                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        let args = read_log_args(&args_log);
        assert!(
            !args.iter().any(|v| v.contains(&long_prompt)),
            "prompt should not be passed as argv when auto uses stdin: {args:?}"
        );
        let stdin_text = read_log(&stdin_log);
        assert!(
            stdin_text.contains(&long_prompt),
            "prompt should be passed via stdin when auto triggers"
        );
    }

    #[tokio::test]
    async fn model_fallback_uses_next_model_on_error() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_codex_fallback_config(&cfg_path);
        let store = SessionStore::new(td.path().join("sessions.json"));
        let server = VibeServer::new(codex_loader(&cfg_path), store);

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex-model.log");
        write_fake_cli_with_model_error_once(&fake, &log, "gpt-5.2-codex");
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: "ping".to_string(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: None,
                    force_new_session: true,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: None,
                    validate_patch: false,
                    client: None,

                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        let warn = out.warnings.unwrap_or_default();
        assert!(warn.contains("model fallback used"));
        assert!(warn.contains("gpt-5.2"));

        let log_text = read_log(&log);
        assert!(log_text.contains("gpt-5.2-codex"));
        assert!(log_text.contains("gpt-5.2"));
    }

    #[tokio::test]
    async fn model_fallback_uses_configured_error_patterns() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_codex_fallback_pattern_config(&cfg_path);
        let store = SessionStore::new(td.path().join("sessions.json"));
        let server = VibeServer::new(codex_loader(&cfg_path), store);

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex-model.log");
        write_fake_cli_with_custom_model_error_once(
            &fake,
            &log,
            "gpt-5.2-codex",
            "custom_not_found",
        );
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: "ping".to_string(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: None,
                    force_new_session: true,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: None,
                    validate_patch: false,
                    client: None,

                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        let warn = out.warnings.unwrap_or_default();
        assert!(warn.contains("model fallback used"));
        assert!(warn.contains("codex/gpt-5.2"));
    }

    #[tokio::test]
    async fn info_includes_enabled_flag() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        std::fs::write(
            &cfg_path,
            r#"{
  "backend": {
    "codex": {
      "models": { "gpt-5.2": {} }
    }
  },
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2",
      "enabled": false,
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only" }
    },
    "reader": {
      "model": "codex/gpt-5.2",
      "personas": { "description": "d2", "prompt": "p2" },
      "capabilities": { "filesystem": "read-only" }
    }
  }
}"#,
        )
        .unwrap();

        let store = SessionStore::new(td.path().join("sessions.json"));
        let server = VibeServer::new(ConfigLoader::new(Some(cfg_path)), store);

        let out = server
            .info(Parameters(InfoArgs {
                cd: repo.to_string_lossy().to_string(),
                client: None,
            }))
            .await
            .unwrap();

        let content = out
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        let roles = v["roles"].as_array().unwrap();
        let mut found = false;
        for role in roles {
            if role["role"] == "oracle" {
                found = true;
                assert_eq!(role["enabled"], false);
            }
        }
        assert!(found, "role list should include disabled roles");
    }

    #[tokio::test]
    async fn adapter_renders_options_and_capabilities() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let cfg_path = td.path().join("config.json");
        write_codex_test_config(&cfg_path);
        let server = VibeServer::new(codex_loader(&cfg_path), store);

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        write_fake_cli(&fake, &log, "sess-cfg-1", "pong");
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: "ping".to_string(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: None,
                    force_new_session: true,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: None,
                    validate_patch: false,
                    client: None,

                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        assert!(out.success, "error={:?}", out.error);
        assert_eq!(out.backend, "codex");
        assert!(out.agent_messages.contains("pong"));

        let log_txt = read_log(&log);
        assert!(log_txt.contains("--model gpt-5.2-codex"));
        assert!(log_txt.contains("model_reasoning_effort=xhigh"));
        assert!(!log_txt.contains("--dangerously-bypass-approvals-and-sandbox"));
    }

    #[tokio::test]
    async fn contract_patch_with_citations_fails_when_missing() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let cfg_path = td.path().join("config.json");
        write_codex_test_config(&cfg_path);
        let server = VibeServer::new(codex_loader(&cfg_path), store);

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
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: "do".to_string(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: None,
                    force_new_session: true,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: Some(OutputContract::PatchWithCitations),
                    validate_patch: false,
                    client: None,

                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(out.success, false);
        assert!(out
            .error
            .as_deref()
            .unwrap_or("")
            .contains("output contract violation"));
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
        let cfg_path = td.path().join("config.json");
        write_codex_test_config(&cfg_path);
        let server = VibeServer::new(codex_loader(&cfg_path), store);

        let fake = td.path().join("fake-codex.sh");

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
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: "do".to_string(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: None,
                    force_new_session: true,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: Some(OutputContract::PatchWithCitations),
                    validate_patch: true,
                    client: None,

                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(out.success, true, "error={:?}", out.error);
        assert_eq!(out.patch_apply_check_ok, Some(true));
    }

    #[tokio::test]
    async fn batch_returns_partial_results() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);

        let cfg_path = td.path().join("config.json");
        write_batch_codex_config(&cfg_path);
        let server = VibeServer::new(codex_loader(&cfg_path), store);

        let fake = td.path().join("fake-codex.sh");
        let script = r#"#!/bin/sh
set -e

if echo "$@" | grep -q 'FAIL'; then
  echo "boom" 1>&2
  exit 1
fi

echo '{"type":"thread.started","thread_id":"sess-1"}'
echo '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
"#;
        std::fs::write(&fake, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fake).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fake, perms).unwrap();
        }
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let out = server
            .run_batch_internal(
                None,
                BatchArgs {
                    cd: repo.to_string_lossy().to_string(),
                    timeout_secs: Some(5),
                    client: None,
                    tasks: vec![
                        BatchTask {
                            prompt: "ok".to_string(),
                            role: Some("oracle".to_string()),
                            backend: None,
                            model: None,
                            reasoning_effort: None,
                            session_id: None,
                            force_new_session: false,
                            session_key: None,
                            timeout_secs: None,
                            contract: None,
                            validate_patch: false,
                            name: Some("one".to_string()),
                        },
                        BatchTask {
                            prompt: "FAIL".to_string(),
                            role: Some("builder".to_string()),
                            backend: None,
                            model: None,
                            reasoning_effort: None,
                            session_id: None,
                            force_new_session: false,
                            session_key: None,
                            timeout_secs: None,
                            contract: None,
                            validate_patch: false,
                            name: Some("two".to_string()),
                        },
                    ],

                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        assert!(!out.success);
        assert_eq!(out.results.len(), 2);
        assert!(out
            .results
            .iter()
            .any(|r| r.output.as_ref().map(|o| o.success).unwrap_or(false)));
        assert!(out.results.iter().any(|r| {
            r.output.as_ref().map(|o| !o.success).unwrap_or(false) || r.error.is_some()
        }));
    }

    #[tokio::test]
    async fn batch_rejects_multiple_kimi_resume_tasks() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);

        let cfg_path = td.path().join("config.json");
        write_batch_kimi_config(&cfg_path);
        let server = VibeServer::new(ConfigLoader::new(Some(cfg_path)), store.clone());

        let key_a = SessionStore::compute_key(&repo.canonicalize().unwrap(), "kimi_a", "kimi_a");
        let key_b = SessionStore::compute_key(&repo.canonicalize().unwrap(), "kimi_b", "kimi_b");
        store
            .put(
                &key_a,
                SessionRecord {
                    repo_root: repo.to_string_lossy().to_string(),
                    role: "kimi_a".to_string(),
                    role_id: "kimi_a".to_string(),
                    backend: Backend::Kimi,
                    backend_session_id: "stateless".to_string(),
                    sampling_history: Vec::new(),
                    updated_at_unix_secs: now_unix_secs(),
                },
            )
            .unwrap();
        store
            .put(
                &key_b,
                SessionRecord {
                    repo_root: repo.to_string_lossy().to_string(),
                    role: "kimi_b".to_string(),
                    role_id: "kimi_b".to_string(),
                    backend: Backend::Kimi,
                    backend_session_id: "stateless".to_string(),
                    sampling_history: Vec::new(),
                    updated_at_unix_secs: now_unix_secs(),
                },
            )
            .unwrap();

        let err = server
            .run_batch_internal(
                None,
                BatchArgs {
                    cd: repo.to_string_lossy().to_string(),
                    timeout_secs: Some(5),
                    client: None,
                    tasks: vec![
                        BatchTask {
                            prompt: "a".to_string(),
                            role: Some("kimi_a".to_string()),
                            backend: None,
                            model: None,
                            reasoning_effort: None,
                            session_id: None,
                            force_new_session: false,
                            session_key: None,
                            timeout_secs: None,
                            contract: None,
                            validate_patch: false,
                            name: None,
                        },
                        BatchTask {
                            prompt: "b".to_string(),
                            role: Some("kimi_b".to_string()),
                            backend: None,
                            model: None,
                            reasoning_effort: None,
                            session_id: None,
                            force_new_session: false,
                            session_key: None,
                            timeout_secs: None,
                            contract: None,
                            validate_patch: false,
                            name: None,
                        },
                    ],

                    conversation_id: None,
                },
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("kimi"));
    }

    #[test]
    fn roundtable_args_rejects_moderator_field() {
        let raw = r#"{
  "TOPIC": "test",
  "cd": ".",
  "participants": [
    {"name": "oracle", "role": "oracle"}
  ],
  "moderator": {"role": "oracle"}
}"#;

        let err = serde_json::from_str::<RoundtableArgs>(raw).unwrap_err();
        assert!(
            err.to_string().contains("unknown field") && err.to_string().contains("moderator"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn session_reuse_isolated_by_client() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let cfg_path = td.path().join("config.json");
        write_codex_test_config(&cfg_path);
        let server = VibeServer::new(codex_loader(&cfg_path), store.clone());

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        write_fake_cli(&fake, &log, "sess-new", "pong");
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let key = SessionStore::compute_key_with_scope(
            &repo.canonicalize().unwrap(),
            "oracle",
            "oracle",
            Some("claude"),
            None,
        );
        store
            .put(
                &key,
                SessionRecord {
                    repo_root: repo.to_string_lossy().to_string(),
                    role: "oracle".to_string(),
                    role_id: "oracle".to_string(),
                    backend: Backend::Codex,
                    backend_session_id: "sess-prev".to_string(),
                    sampling_history: Vec::new(),
                    updated_at_unix_secs: now_unix_secs(),
                },
            )
            .unwrap();

        let out = server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: "ping".to_string(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: None,
                    force_new_session: false,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: None,
                    validate_patch: false,
                    client: Some("codex".to_string()),
                    conversation_id: None,
                },
            )
            .await
            .unwrap();

        assert!(
            !out.resumed,
            "session should not resume across different client hints"
        );
    }

    #[tokio::test]
    async fn session_reuse_isolated_by_conversation_id() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let cfg_path = td.path().join("config.json");
        write_codex_test_config(&cfg_path);
        let server = VibeServer::new(codex_loader(&cfg_path), store.clone());

        let fake = td.path().join("fake-codex.sh");
        let log = td.path().join("codex.log");
        write_fake_cli(&fake, &log, "sess-new", "pong");
        let _env = crate::test_utils::scoped_codex_bin(fake.to_string_lossy().as_ref());

        let key = SessionStore::compute_key_with_scope(
            &repo.canonicalize().unwrap(),
            "oracle",
            "oracle",
            Some("claude"),
            Some("conv-a"),
        );
        store
            .put(
                &key,
                SessionRecord {
                    repo_root: repo.to_string_lossy().to_string(),
                    role: "oracle".to_string(),
                    role_id: "oracle".to_string(),
                    backend: Backend::Codex,
                    backend_session_id: "sess-prev".to_string(),
                    sampling_history: Vec::new(),
                    updated_at_unix_secs: now_unix_secs(),
                },
            )
            .unwrap();

        let out = server
            .run_vibe_internal(
                None,
                VibeArgs {
                    prompt: "ping".to_string(),
                    cd: repo.to_string_lossy().to_string(),
                    role: Some("oracle".to_string()),
                    backend: None,
                    model: None,
                    reasoning_effort: None,
                    session_id: None,
                    force_new_session: false,
                    session_key: None,
                    timeout_secs: Some(5),
                    contract: None,
                    validate_patch: false,
                    client: Some("claude".to_string()),
                    conversation_id: Some("conv-b".to_string()),
                },
            )
            .await
            .unwrap();

        assert!(
            !out.resumed,
            "session should not resume across different conversation_id values"
        );
    }
}
