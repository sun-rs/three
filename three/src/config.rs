use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct VibeConfig {
    #[serde(default)]
    pub brains: BTreeMap<String, BrainProfile>,
    #[serde(default)]
    pub roles: BTreeMap<String, RoleConfig>,
    /// Optional persona library shared with Claude Code plugins.
    #[serde(default)]
    pub personas: BTreeMap<String, PersonaConfig>,
}

// =====================
// Config file formats
// =====================

#[derive(Debug, Clone, Deserialize)]
struct ThreeConfigV2 {
    #[serde(rename = "backend", alias = "provider", default)]
    providers: BTreeMap<String, ProviderV2>,
    #[serde(default)]
    roles: BTreeMap<String, RoleV2>,
    #[serde(default)]
    personas: BTreeMap<String, PersonaConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderV2 {
    /// Provider type determines which runner is used.
    ///
    /// Supported:
    /// - `codex-cli`
    /// - `gemini-cli`
    #[serde(rename = "type", default)]
    kind: Option<ProviderKind>,

    #[serde(default)]
    models: BTreeMap<String, ProviderModelV2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ProviderKind {
    CodexCli,
    GeminiCli,
    McpSampling,
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderModelV2 {
    /// Real model id used by the underlying CLI. If omitted, the model key is used.
    #[serde(default)]
    #[serde(rename = "model", alias = "id")]
    id: Option<String>,

    /// Optional per-model options.
    #[serde(default)]
    options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct RoleV2 {
    /// Role selects a model from a provider.
    ///
    /// Examples:
    /// - {"provider":"codex","model":"gpt-5.2-codex-xhigh"}
    /// - "codex.gpt-5.2-codex-xhigh"
    ///
    /// You may also use the key name `brain` instead of `model`.
    #[serde(alias = "brain", alias = "model")]
    model: RoleModelRefV2,

    #[serde(default)]
    policy: RolePolicy,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,

    /// Optional persona reference (top-level `personas.{id}`)
    #[serde(default)]
    persona: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RoleModelRefV2 {
    Qualified(String),
    Object {
        #[serde(alias = "backend")]
        provider: String,
        model: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoleConfig {
    pub brain: String,
    /// Role-level policy (e.g., read vs write) independent of model choice.
    #[serde(default)]
    pub policy: RolePolicy,
    /// Optional role description (plugin/UI).
    #[serde(default)]
    pub description: Option<String>,
    /// Optional role prompt (persona) to inject ahead of tool prompts.
    ///
    /// If set, this takes precedence over `persona`.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Optional per-role timeout (seconds) used when the tool call does not specify `timeout_secs`.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Optional persona id (references `personas.{id}`) to provide role instructions.
    #[serde(default)]
    pub persona: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PersonaConfig {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RolePolicy {
    #[serde(default)]
    pub codex: CodexRolePolicy,
}

impl Default for RolePolicy {
    fn default() -> Self {
        Self {
            codex: CodexRolePolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodexRolePolicy {
    #[serde(default)]
    pub sandbox: CodexSandboxPolicy,
    #[serde(default)]
    pub ask_for_approval: Option<CodexApprovalPolicy>,
    /// If true, pass `--dangerously-bypass-approvals-and-sandbox`.
    ///
    /// WARNING: this removes sandbox boundaries and approvals.
    #[serde(default)]
    pub dangerously_bypass_approvals_and_sandbox: bool,
    /// If true, pass `--skip-git-repo-check` to codex.
    ///
    /// Default behavior (when unset): true for read-only sandbox, false otherwise.
    #[serde(default)]
    pub skip_git_repo_check: Option<bool>,
}

impl Default for CodexRolePolicy {
    fn default() -> Self {
        Self {
            sandbox: CodexSandboxPolicy::ReadOnly,
            // Default to non-interactive automation. Users can override.
            ask_for_approval: Some(CodexApprovalPolicy::Never),
            dangerously_bypass_approvals_and_sandbox: false,
            skip_git_repo_check: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodexSandboxPolicy {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

impl Default for CodexSandboxPolicy {
    fn default() -> Self {
        Self::ReadOnly
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodexApprovalPolicy {
    Untrusted,
    OnFailure,
    OnRequest,
    Never,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrainProfile {
    pub backend: Backend,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Codex,
    Gemini,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
    Xhigh,
}

impl ReasoningEffort {
    pub fn as_codex_config_value(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }
}

impl VibeConfig {
    pub fn default_path() -> Option<PathBuf> {
        // Prefer XDG-style config layout for consistency with other CLIs.
        // - $XDG_CONFIG_HOME/three/config.json
        // - ~/.config/three/config.json
        if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(base).join("three").join("config.json"));
        }
        let home = dirs::home_dir()?;
        Some(home.join(".config").join("three").join("config.json"))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;

        let v: serde_json::Value = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse config JSON: {}", path.display()))?;

        let is_v2 = v.get("backend").is_some()
            || v.get("provider").is_some()
            || v.get("providers").is_some();
        if !is_v2 {
            return Err(anyhow!(
                "invalid config: missing 'backend' object (v2 schema required)"
            ));
        }

        // Support both `backend` (preferred) and legacy `provider`/`providers` as aliases.
        let mut v2_value = v;
        if v2_value.get("backend").is_none() {
            // provider -> backend
            if let Some(p) = v2_value.get("provider").cloned() {
                if let Some(obj) = v2_value.as_object_mut() {
                    obj.insert("backend".to_string(), p);
                    obj.remove("provider");
                }
            }
        }
        if v2_value.get("backend").is_none() {
            // providers -> backend
            if let Some(p) = v2_value.get("providers").cloned() {
                if let Some(obj) = v2_value.as_object_mut() {
                    obj.insert("backend".to_string(), p);
                    obj.remove("providers");
                }
            }
        }

        let v2: ThreeConfigV2 = serde_json::from_value(v2_value)
            .with_context(|| format!("failed to parse v2 config JSON: {}", path.display()))?;
        v2.into_effective()
    }

    pub fn resolve_profile(
        &self,
        role: Option<&str>,
        brain: Option<&str>,
    ) -> Result<ResolvedProfile> {
        if let Some(brain_id) = brain {
            let prof = self
                .brains
                .get(brain_id)
                .ok_or_else(|| anyhow!("unknown brain profile: {brain_id}"))?
                .clone();
            return Ok(ResolvedProfile {
                brain_id: brain_id.to_string(),
                profile: prof,
            });
        }

        if let Some(role_id) = role {
            let role_cfg = self
                .roles
                .get(role_id)
                .ok_or_else(|| anyhow!("unknown role: {role_id}"))?;
            let brain_id = role_cfg.brain.as_str();
            let prof = self
                .brains
                .get(brain_id)
                .ok_or_else(|| anyhow!("role {role_id} references missing brain: {brain_id}"))?
                .clone();
            return Ok(ResolvedProfile {
                brain_id: brain_id.to_string(),
                profile: prof,
            });
        }

        Err(anyhow!(
            "either 'brain' or 'role' must be provided when using config"
        ))
    }
}

impl ThreeConfigV2 {
    fn into_effective(self) -> Result<VibeConfig> {
        let mut brains: BTreeMap<String, BrainProfile> = BTreeMap::new();

        for (provider_id, provider) in self.providers {
            let kind = provider.kind.or_else(|| infer_provider_kind(&provider_id));
            let Some(kind) = kind else {
                // Skip unknown providers (future extension).
                continue;
            };

            for (model_key, model_cfg) in provider.models {
                let brain_id = format!("{provider_id}.{model_key}");
                let model_id = model_cfg.id.clone().or_else(|| Some(model_key.clone()));

                let (backend, reasoning_effort) = match kind {
                    ProviderKind::CodexCli => (
                        Backend::Codex,
                        extract_reasoning_effort(model_cfg.options.as_ref()),
                    ),
                    ProviderKind::GeminiCli => (Backend::Gemini, None),
                    ProviderKind::McpSampling => (Backend::Claude, None),
                };

                brains.insert(
                    brain_id,
                    BrainProfile {
                        backend,
                        model: model_id,
                        reasoning_effort,
                    },
                );
            }
        }

        let mut roles: BTreeMap<String, RoleConfig> = BTreeMap::new();
        for (role_id, role_cfg) in self.roles {
            let (provider_id, model_key) = match role_cfg.model {
                RoleModelRefV2::Object { provider, model } => (provider, model),
                RoleModelRefV2::Qualified(s) => parse_qualified_ref(&s)
                    .ok_or_else(|| anyhow!("invalid role model reference: {s}"))?,
            };
            let brain = format!("{provider_id}.{model_key}");
            roles.insert(
                role_id,
                RoleConfig {
                    brain,
                    policy: role_cfg.policy,
                    description: role_cfg.description,
                    prompt: role_cfg.prompt,
                    timeout_secs: role_cfg.timeout_secs,
                    persona: role_cfg.persona,
                },
            );
        }

        Ok(VibeConfig {
            brains,
            roles,
            personas: self.personas,
        })
    }
}

fn infer_provider_kind(provider_id: &str) -> Option<ProviderKind> {
    match provider_id.to_ascii_lowercase().as_str() {
        "codex" => Some(ProviderKind::CodexCli),
        "gemini" => Some(ProviderKind::GeminiCli),
        "claude" => Some(ProviderKind::McpSampling),
        _ => None,
    }
}

fn parse_qualified_ref(s: &str) -> Option<(String, String)> {
    // Accept "provider.model", "provider:model", or "provider/model".
    for sep in ['/', '.', ':'] {
        if let Some((a, b)) = s.split_once(sep) {
            let a = a.trim();
            let b = b.trim();
            if !a.is_empty() && !b.is_empty() {
                return Some((a.to_string(), b.to_string()));
            }
        }
    }
    None
}

fn extract_reasoning_effort(options: Option<&serde_json::Value>) -> Option<ReasoningEffort> {
    let v = options?;
    let obj = v.as_object()?;

    // Support both naming styles.
    let raw = obj
        .get("reasoningEffort")
        .and_then(|x| x.as_str())
        .or_else(|| obj.get("reasoning_effort").and_then(|x| x.as_str()))?;

    match raw.to_ascii_lowercase().as_str() {
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" => Some(ReasoningEffort::Xhigh),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub brain_id: String,
    pub profile: BrainProfile,
}
