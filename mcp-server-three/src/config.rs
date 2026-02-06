use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::adapter_catalog::embedded_adapter_catalog;

#[derive(Debug, Clone, Deserialize)]
pub struct VibeConfig {
    pub backend: BTreeMap<String, BackendConfig>,
    pub roles: BTreeMap<String, RoleConfig>,
}

#[derive(Debug, Clone)]
pub struct ConfigLoader {
    user_config_path: Option<PathBuf>,
}

pub struct ConfigLoadResult {
    pub config: Option<VibeConfig>,
    pub sources: Vec<PathBuf>,
}

impl ConfigLoader {
    pub fn new(user_config_path: Option<PathBuf>) -> Self {
        Self { user_config_path }
    }

    pub fn user_config_path(&self) -> Option<&Path> {
        self.user_config_path.as_deref()
    }

    pub fn project_config_paths(repo_root: &Path) -> [PathBuf; 2] {
        // Prefer a dedicated config directory.
        let a = repo_root.join(".three").join("config.json");
        // Back-compat / convenience for small repos.
        let b = repo_root.join(".three.json");
        [a, b]
    }

    pub fn project_config_paths_for_client(repo_root: &Path, client: Option<&str>) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Some(client) = client {
            if !client.is_empty() {
                paths.push(
                    repo_root
                        .join(".three")
                        .join(format!("config-{client}.json")),
                );
            }
        }
        paths.extend(Self::project_config_paths(repo_root));
        paths
    }

    pub fn user_config_paths_for_client(&self, client: Option<&str>) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let Some(path) = self.user_config_path.as_ref() else {
            return paths;
        };
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name == "config.json" {
            if let Some(client) = client {
                if !client.is_empty() {
                    if let Some(dir) = path.parent() {
                        paths.push(dir.join(format!("config-{client}.json")));
                    }
                }
            }
            paths.push(path.clone());
        } else {
            paths.push(path.clone());
        }
        paths
    }

    /// Load config for a repo, merging user-level config with a project override.
    ///
    /// Precedence: project overrides user. If neither exists, returns None.
    pub fn load_for_repo(&self, repo_root: &Path) -> Result<Option<VibeConfig>> {
        Ok(self.load_for_repo_with_client(repo_root, None)?.config)
    }

    pub fn load_for_repo_with_client(
        &self,
        repo_root: &Path,
        client: Option<&str>,
    ) -> Result<ConfigLoadResult> {
        let mut sources: Vec<PathBuf> = Vec::new();

        let mut user_cfg: Option<VibeConfig> = None;
        for p in self.user_config_paths_for_client(client) {
            if p.exists() {
                user_cfg = Some(VibeConfig::load(&p)?);
                sources.push(p);
                break;
            }
        }

        let mut project_cfg: Option<VibeConfig> = None;
        for p in Self::project_config_paths_for_client(repo_root, client) {
            if p.exists() {
                project_cfg =
                    Some(VibeConfig::load(&p).with_context(|| {
                        format!("failed to load project config: {}", p.display())
                    })?);
                sources.push(p);
                break;
            }
        }

        let mut cfg = match (user_cfg, project_cfg) {
            (None, None) => None,
            (Some(u), None) => Some(u),
            (None, Some(p)) => Some(p),
            (Some(u), Some(p)) => Some(merge_config(u, p)),
        };

        if let Some(ref mut cfg_val) = cfg {
            let catalog = embedded_adapter_catalog();
            apply_adapter_catalog(cfg_val, &catalog);
        }

        Ok(ConfigLoadResult {
            config: cfg,
            sources,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackendConfig {
    #[serde(default)]
    pub adapter: Option<AdapterConfig>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelConfig>,
    #[serde(default)]
    pub fallback: Option<BackendFallback>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackendFallback {
    pub model: String,
    #[serde(default)]
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterConfig {
    pub args_template: Vec<String>,
    pub output_parser: OutputParserConfig,
    #[serde(default)]
    pub filesystem_capabilities: Option<Vec<FilesystemCapability>>,
    /// How to pass prompt text to the backend (default: arg).
    #[serde(default)]
    pub prompt_transport: Option<PromptTransport>,
    /// Maximum prompt length (chars) before auto switches to stdin (default: 32768).
    #[serde(default)]
    pub prompt_max_chars: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputParserConfig {
    JsonStream {
        session_id_path: String,
        message_path: String,
        #[serde(default)]
        pick: Option<OutputPick>,
        #[serde(default)]
        fallback: Option<JsonStreamFallback>,
    },
    JsonObject {
        message_path: String,
        #[serde(default)]
        session_id_path: Option<String>,
    },
    Regex {
        session_id_pattern: String,
        message_capture_group: usize,
    },
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputPick {
    First,
    Last,
}

impl Default for OutputPick {
    fn default() -> Self {
        Self::Last
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonStreamFallback {
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptTransport {
    Arg,
    Stdin,
    Auto,
}

impl OutputParserConfig {
    pub fn supports_session(&self) -> bool {
        match self {
            OutputParserConfig::JsonStream { .. } => true,
            OutputParserConfig::Regex { .. } => true,
            OutputParserConfig::JsonObject {
                session_id_path, ..
            } => session_id_path
                .as_ref()
                .map(|p| !p.trim().is_empty())
                .unwrap_or(false),
            OutputParserConfig::Text => false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    #[serde(default)]
    pub options: BTreeMap<String, OptionValue>,
    #[serde(default)]
    pub variants: BTreeMap<String, BTreeMap<String, OptionValue>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum OptionValue {
    Bool(bool),
    Number(serde_json::Number),
    String(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoleConfig {
    pub model: String,
    #[serde(default)]
    pub personas: Option<PersonaConfig>,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PersonaConfig {
    pub description: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Capabilities {
    #[serde(default = "default_filesystem_capability")]
    pub filesystem: FilesystemCapability,
    #[serde(default = "default_shell_capability")]
    pub shell: ShellCapability,
    #[serde(default = "default_network_capability")]
    pub network: NetworkCapability,
    #[serde(default = "default_tools")]
    pub tools: Vec<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            filesystem: default_filesystem_capability(),
            shell: default_shell_capability(),
            network: default_network_capability(),
            tools: default_tools(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilesystemCapability {
    ReadOnly,
    ReadWrite,
}

fn default_shell_capability() -> ShellCapability {
    ShellCapability::Allow
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellCapability {
    Allow,
    Deny,
}

fn default_network_capability() -> NetworkCapability {
    NetworkCapability::Allow
}

fn default_filesystem_capability() -> FilesystemCapability {
    FilesystemCapability::ReadWrite
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkCapability {
    Allow,
    Deny,
}

fn default_tools() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Claude,
    Codex,
    Opencode,
    Kimi,
    Gemini,
}

impl Backend {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "opencode" => Some(Self::Opencode),
            "kimi" => Some(Self::Kimi),
            "gemini" => Some(Self::Gemini),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Kimi => "kimi",
            Self::Gemini => "gemini",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoleProfile {
    pub backend: Backend,
    pub backend_id: String,
    pub model: String,
    pub options: BTreeMap<String, OptionValue>,
    pub capabilities: Capabilities,
    pub adapter: AdapterConfig,
    pub timeout_secs: Option<u64>,
}

impl VibeConfig {
    pub fn default_path() -> Option<PathBuf> {
        if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(base).join("three").join("config.json"));
        }
        if cfg!(windows) {
            if let Some(dir) = dirs::config_dir() {
                return Some(dir.join("three").join("config.json"));
            }
        }
        let home = dirs::home_dir()?;
        Some(home.join(".config").join("three").join("config.json"))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;

        let v: serde_json::Value = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse config JSON: {}", path.display()))?;
        let obj = v
            .as_object()
            .ok_or_else(|| anyhow!("invalid config: expected a JSON object"))?;

        for key in obj.keys() {
            if key != "backend" && key != "roles" {
                return Err(anyhow!("invalid config: unexpected top-level key: {key}"));
            }
        }
        if !obj.contains_key("backend") {
            return Err(anyhow!("invalid config: missing 'backend' object"));
        }
        if !obj.contains_key("roles") {
            return Err(anyhow!("invalid config: missing 'roles' object"));
        }
        if let Some(roles) = obj.get("roles").and_then(|v| v.as_object()) {
            for (role_id, role_val) in roles {
                if let Some(role_obj) = role_val.as_object() {
                    if role_obj.contains_key("fallback_models") {
                        return Err(anyhow!(
                            "invalid config: roles.{role_id}.fallback_models is not supported; use backend.<id>.fallback instead"
                        ));
                    }
                }
            }
        }

        let mut cfg: VibeConfig = serde_json::from_value(v)
            .with_context(|| format!("failed to parse config JSON: {}", path.display()))?;
        let catalog = embedded_adapter_catalog();
        apply_adapter_catalog(&mut cfg, &catalog);
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn resolve_profile(&self, role: Option<&str>) -> Result<ResolvedProfile> {
        let role_id = role.ok_or_else(|| anyhow!("'role' must be provided when using config"))?;
        let role_cfg = self
            .roles
            .get(role_id)
            .ok_or_else(|| anyhow!("unknown role profile: {role_id}"))?;
        if !role_cfg.enabled {
            return Err(anyhow!("role '{role_id}' is disabled"));
        }

        let (backend_id, model_id, variant) = parse_role_model_ref(&role_cfg.model)?;
        let backend = parse_backend_key(&backend_id)?;
        let backend_cfg = self
            .backend
            .get(&backend_id)
            .ok_or_else(|| anyhow!("missing backend config: {backend_id}"))?;
        let adapter = backend_cfg
            .adapter
            .clone()
            .ok_or_else(|| anyhow!("missing adapter config for backend: {backend_id}"))?;
        if let Some(allowed) = adapter.filesystem_capabilities.as_ref() {
            if !allowed.contains(&role_cfg.capabilities.filesystem) {
                return Err(anyhow!(
                    "unsupported filesystem capability {:?} for backend '{}' (role '{}')",
                    role_cfg.capabilities.filesystem,
                    backend_id,
                    role_id
                ));
            }
        }
        let options = if model_id == "default" {
            if variant.is_some() {
                return Err(anyhow!("model 'default' does not support variants"));
            }
            if let Some(model_cfg) = backend_cfg.models.get("default") {
                resolve_model_options(model_cfg, None)?
            } else {
                BTreeMap::new()
            }
        } else {
            let model_cfg = backend_cfg
                .models
                .get(&model_id)
                .ok_or_else(|| anyhow!("unknown model '{model_id}' for backend '{backend_id}'"))?;
            resolve_model_options(model_cfg, variant.as_deref())?
        };

        Ok(ResolvedProfile {
            role_id: role_id.to_string(),
            profile: RoleProfile {
                backend,
                backend_id: backend_id.clone(),
                model: model_id,
                options,
                capabilities: role_cfg.capabilities.clone(),
                adapter,
                timeout_secs: role_cfg.timeout_secs.or(backend_cfg.timeout_secs),
            },
        })
    }

    fn validate(&self) -> Result<()> {
        for backend_id in self.backend.keys() {
            parse_backend_key(backend_id)?;
        }
        for (backend_id, backend_cfg) in &self.backend {
            if let Some(fallback) = backend_cfg.fallback.as_ref() {
                let has_patterns = fallback.patterns.iter().any(|p| !p.trim().is_empty());
                if !has_patterns {
                    return Err(anyhow!(
                        "backend {backend_id} defines fallback but no fallback.patterns"
                    ));
                }
                let (fallback_backend_id, model_id, variant) =
                    parse_role_model_ref(&fallback.model).with_context(|| {
                        format!("invalid backend fallback model reference: {backend_id}")
                    })?;
                if !self.backend.contains_key(&fallback_backend_id) {
                    return Err(anyhow!(
                        "backend {backend_id} fallback model references missing backend: {fallback_backend_id}"
                    ));
                }
                let target_backend = self
                    .backend
                    .get(&fallback_backend_id)
                    .ok_or_else(|| anyhow!("missing backend config: {fallback_backend_id}"))?;
                if model_id == "default" {
                    if variant.is_some() {
                        return Err(anyhow!(
                            "fallback model 'default' does not support variants"
                        ));
                    }
                } else if !target_backend.models.contains_key(&model_id) {
                    return Err(anyhow!(
                        "backend {backend_id} fallback model references unknown model '{model_id}' for backend '{fallback_backend_id}'"
                    ));
                }
            }
        }
        for (role_id, role) in &self.roles {
            let (backend_id, _model_id, variant) = parse_role_model_ref(&role.model)
                .with_context(|| format!("invalid role model reference: {role_id}"))?;
            if let Some(v) = variant.as_deref() {
                if v.trim().is_empty() {
                    return Err(anyhow!("invalid role model reference: {role_id}"));
                }
            }
            if !self.backend.contains_key(&backend_id) {
                return Err(anyhow!(
                    "role {role_id} references missing backend: {backend_id}"
                ));
            }
        }
        Ok(())
    }
}

fn parse_backend_key(provider_id: &str) -> Result<Backend> {
    Backend::parse(provider_id).ok_or_else(|| {
        anyhow!(
            "unsupported backend key: {provider_id} (expected claude|codex|opencode|kimi|gemini)"
        )
    })
}

pub(crate) fn parse_role_model_ref(s: &str) -> Result<(String, String, Option<String>)> {
    let (backend, rest) = s
        .split_once('/')
        .ok_or_else(|| anyhow!("role model reference must be 'backend/model@variant'"))?;
    let backend = backend.trim();
    let rest = rest.trim();
    if backend.is_empty() || rest.is_empty() {
        return Err(anyhow!(
            "role model reference must be 'backend/model@variant'"
        ));
    }

    let (model, variant) = match rest.split_once('@') {
        Some((m, v)) => (m.trim(), Some(v.trim().to_string())),
        None => (rest, None),
    };
    if model.is_empty() {
        return Err(anyhow!(
            "role model reference must be 'backend/model@variant'"
        ));
    }
    Ok((backend.to_string(), model.to_string(), variant))
}

pub(crate) fn resolve_model_options(
    model_cfg: &ModelConfig,
    variant: Option<&str>,
) -> Result<BTreeMap<String, OptionValue>> {
    let mut out = model_cfg.options.clone();
    if let Some(v) = variant {
        let v = v.trim();
        if v.is_empty() {
            return Err(anyhow!("variant name cannot be empty"));
        }
        let overrides = model_cfg
            .variants
            .get(v)
            .ok_or_else(|| anyhow!("unknown variant: {v}"))?;
        for (k, val) in overrides {
            out.insert(k.to_string(), val.clone());
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub role_id: String,
    pub profile: RoleProfile,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterCatalog {
    pub adapters: BTreeMap<String, AdapterConfig>,
}

fn merge_config(mut base: VibeConfig, overlay: VibeConfig) -> VibeConfig {
    // Maps are merged by key; project overrides user on conflicts.
    for (backend_id, overlay_backend) in overlay.backend {
        match base.backend.get_mut(&backend_id) {
            Some(base_backend) => {
                base_backend.models.extend(overlay_backend.models);
                if overlay_backend.adapter.is_some() {
                    base_backend.adapter = overlay_backend.adapter;
                }
                if overlay_backend.timeout_secs.is_some() {
                    base_backend.timeout_secs = overlay_backend.timeout_secs;
                }
                if overlay_backend.fallback.is_some() {
                    base_backend.fallback = overlay_backend.fallback;
                }
            }
            None => {
                base.backend.insert(backend_id, overlay_backend);
            }
        }
    }
    base.roles.extend(overlay.roles);
    base
}

fn apply_adapter_catalog(cfg: &mut VibeConfig, catalog: &AdapterCatalog) {
    for (backend_id, backend_cfg) in cfg.backend.iter_mut() {
        if backend_cfg.adapter.is_none() {
            if let Some(adapter) = catalog.adapters.get(backend_id) {
                backend_cfg.adapter = Some(adapter.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_catalog::embedded_adapter_catalog;
    use std::path::Path;

    #[test]
    fn rejects_unknown_backend_key() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "unknown": {
      "adapter": {"args_template":["run"], "output_parser":{"type":"regex","session_id_pattern":"x","message_capture_group":1}},
      "models": {
        "m": {}
      }
    }
  },
  "roles": {
    "oracle": {
      "model": "unknown/m",
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {"filesystem":"read-only","shell":"deny","network":"deny","tools":["read"]}
    }
  }
}"#,
        )
        .unwrap();

        let err = VibeConfig::load(&path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unsupported backend key") || msg.contains("invalid backend key"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn rejects_missing_roles_key() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "codex": { "models": { "gpt-5.2": {} } }
  }
}"#,
        )
        .unwrap();

        let err = VibeConfig::load(&path).unwrap_err();
        assert!(err.to_string().contains("missing 'roles'"));
    }

    #[test]
    fn rejects_backend_fallback_without_patterns() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "codex": {
      "fallback": {
        "model": "codex/gpt-5.2",
        "patterns": []
      },
      "models": { "gpt-5.2": {} }
    }
  },
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2",
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {"filesystem":"read-only","shell":"deny","network":"deny","tools":["read"]}
    }
  }
}"#,
        )
        .unwrap();

        let err = VibeConfig::load(&path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("fallback") && msg.contains("patterns"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn rejects_role_fallback_models_key() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "codex": { "models": { "gpt-5.2": {} } }
  },
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2",
      "fallback_models": ["codex/gpt-5.2"],
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {"filesystem":"read-only","shell":"deny","network":"deny","tools":["read"]}
    }
  }
}"#,
        )
        .unwrap();

        let err = VibeConfig::load(&path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("fallback_models") && msg.contains("roles"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn resolves_role_timeout_over_backend_timeout() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "codex": {
      "timeout_secs": 120,
      "models": {
        "gpt-5.2": {}
      }
    }
  },
  "roles": {
    "role_default": {
      "model": "codex/gpt-5.2",
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {"filesystem":"read-only"}
    },
    "role_override": {
      "model": "codex/gpt-5.2",
      "timeout_secs": 45,
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {"filesystem":"read-only"}
    }
  }
}"#,
        )
        .unwrap();

        let cfg = VibeConfig::load(&path).unwrap();
        let rp_default = cfg.resolve_profile(Some("role_default")).unwrap();
        assert_eq!(rp_default.profile.timeout_secs, Some(120));

        let rp_override = cfg.resolve_profile(Some("role_override")).unwrap();
        assert_eq!(rp_override.profile.timeout_secs, Some(45));
    }

    #[test]
    fn rejects_disabled_role_on_resolve() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "codex": {
      "models": {
        "gpt-5.2": {}
      }
    }
  },
  "roles": {
    "disabled": {
      "model": "codex/gpt-5.2",
      "enabled": false,
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {"filesystem":"read-only"}
    }
  }
}"#,
        )
        .unwrap();

        let cfg = VibeConfig::load(&path).unwrap();
        let err = cfg.resolve_profile(Some("disabled")).unwrap_err();
        assert!(err.to_string().contains("disabled"), "err={err}");
    }

    #[test]
    fn rejects_filesystem_deny_capability() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "gemini": {
      "models": {
        "gemini-3-pro-preview": {}
      }
    }
  },
  "roles": {
    "reader": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {"filesystem":"deny","shell":"deny","network":"deny","tools":["read"]}
    }
  }
}"#,
        )
        .unwrap();

        let err = VibeConfig::load(&path).unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("deny"), "unexpected error: {msg}");
    }

    #[test]
    fn defaults_missing_capability_fields() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "codex": {
      "models": {
        "gpt-5.2": {}
      }
    }
  },
  "roles": {
    "reader": {
      "model": "codex/gpt-5.2",
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {}
    }
  }
}"#,
        )
        .unwrap();

        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let loader = ConfigLoader::new(Some(path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();
        let resolved = cfg.resolve_profile(Some("reader")).unwrap();
        assert_eq!(
            resolved.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(resolved.profile.capabilities.shell, ShellCapability::Allow);
        assert_eq!(
            resolved.profile.capabilities.network,
            NetworkCapability::Allow
        );
        assert_eq!(resolved.profile.capabilities.tools, vec!["*".to_string()]);
    }

    #[test]
    fn loads_role_from_roles_map() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "opencode": {
      "adapter": {"args_template": ["run"], "output_parser": {"type":"regex","session_id_pattern":"x","message_capture_group":1}},
      "models": { "opencode-gpt-5": {} }
    }
  },
  "roles": {
    "oracle": {
      "model": "opencode/opencode-gpt-5",
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {"filesystem":"read-write","shell":"deny","network":"deny","tools":["read"]}
    }
  }
}"#,
        )
        .unwrap();

        let cfg = VibeConfig::load(&path).unwrap();
        let resolved = cfg.resolve_profile(Some("oracle")).unwrap();
        assert_eq!(resolved.role_id, "oracle");
        assert_eq!(resolved.profile.backend_id, "opencode");
    }

    #[test]
    fn loads_embedded_adapter_catalog() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let config_home = td.path().join("xdg");
        std::fs::create_dir_all(&config_home).unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &config_home);
        }

        let cfg_path = crate::test_utils::example_config_path();
        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();
        let codex = cfg.backend.get("codex").unwrap();
        assert!(codex.adapter.is_some());

        match prev {
            Some(v) => unsafe {
                std::env::set_var("XDG_CONFIG_HOME", v);
            },
            None => unsafe {
                std::env::remove_var("XDG_CONFIG_HOME");
            },
        }
    }

    #[test]
    fn rejects_role_model_without_slash_separator() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("cfg.json");
        std::fs::write(
            &path,
            r#"{
  "backend": {
    "codex": {
      "adapter": {"args_template": ["run"], "output_parser": {"type":"regex","session_id_pattern":"x","message_capture_group":1}},
      "models": { "gpt-5.2": {} }
    }
  },
  "roles": {
    "oracle": {
      "model": "codex.gpt-5.2",
      "personas": {"description":"d","prompt":"p"},
      "capabilities": {"filesystem":"read-only","shell":"deny","network":"deny","tools":["read"]}
    }
  }
}"#,
        )
        .unwrap();

        let err = VibeConfig::load(&path).unwrap_err();
        assert!(
            err.to_string().contains("role model reference"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn allows_default_model_without_backend_definition() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "kimi": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": {}
    }
  },
  "roles": {
    "reader": {
      "model": "kimi/default",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();
        let reader = cfg.resolve_profile(Some("reader")).unwrap();
        assert_eq!(reader.profile.model, "default");
        assert!(reader.profile.options.is_empty());
    }

    #[test]
    fn example_gemini_adapter_uses_sandbox_and_prompt() {
        let catalog = embedded_adapter_catalog();
        let gemini = catalog.adapters.get("gemini").expect("gemini adapter");
        let args = &gemini.args_template;

        assert!(
            args.iter().any(|token| token.contains("--sandbox")),
            "expected --sandbox in gemini adapter args"
        );
        assert!(
            args.iter().any(|token| token.contains("--prompt")),
            "expected --prompt in gemini adapter args"
        );
        assert!(
            args.contains(&"--output-format".to_string()),
            "expected --output-format in gemini adapter args"
        );
        assert!(
            args.contains(&"json".to_string()),
            "expected json in gemini adapter args"
        );
    }

    #[test]
    fn example_opencode_adapter_uses_sessionid_part_text() {
        let catalog = embedded_adapter_catalog();
        let opencode = catalog.adapters.get("opencode").expect("opencode adapter");

        match &opencode.output_parser {
            OutputParserConfig::JsonStream {
                session_id_path,
                message_path,
                pick,
                ..
            } => {
                assert_eq!(session_id_path, "part.sessionID");
                assert_eq!(message_path, "part.text");
                assert_eq!(pick.unwrap_or_default(), OutputPick::Last);
            }
            other => panic!("expected json_stream output parser, got {other:?}"),
        }
    }

    #[test]
    fn example_claude_adapter_uses_json_object() {
        let catalog = embedded_adapter_catalog();
        let claude = catalog.adapters.get("claude").expect("claude adapter");
        assert_eq!(
            claude.filesystem_capabilities.as_deref(),
            Some(
                &[
                    FilesystemCapability::ReadOnly,
                    FilesystemCapability::ReadWrite
                ][..]
            )
        );
        match &claude.output_parser {
            OutputParserConfig::JsonObject {
                session_id_path,
                message_path,
            } => {
                assert_eq!(session_id_path.as_deref(), Some("session_id"));
                assert_eq!(message_path, "result");
            }
            other => panic!("expected json_object output parser, got {other:?}"),
        }
    }

    #[test]
    fn example_codex_adapter_uses_json_stream() {
        let catalog = embedded_adapter_catalog();
        let codex = catalog.adapters.get("codex").expect("codex adapter");
        assert_eq!(
            codex.filesystem_capabilities.as_deref(),
            Some(
                &[
                    FilesystemCapability::ReadOnly,
                    FilesystemCapability::ReadWrite
                ][..]
            )
        );
        match &codex.output_parser {
            OutputParserConfig::JsonStream {
                session_id_path,
                message_path,
                pick,
                ..
            } => {
                assert_eq!(session_id_path, "thread_id");
                assert_eq!(message_path, "item.text");
                assert_eq!(pick.unwrap_or_default(), OutputPick::Last);
            }
            other => panic!("expected json_stream output parser, got {other:?}"),
        }
    }

    #[test]
    fn example_kimi_adapter_uses_text_output() {
        let catalog = embedded_adapter_catalog();
        let kimi = catalog.adapters.get("kimi").expect("kimi adapter");
        assert_eq!(
            kimi.filesystem_capabilities.as_deref(),
            Some(&[FilesystemCapability::ReadWrite][..])
        );
        match &kimi.output_parser {
            OutputParserConfig::Text => {}
            other => panic!("expected text output parser, got {other:?}"),
        }
    }

    #[test]
    fn example_config_loads_with_defaults() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = crate::test_utils::example_config_path();
        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();

        let oracle = cfg.resolve_profile(Some("oracle")).unwrap();
        assert_eq!(oracle.role_id, "oracle");
        assert_eq!(
            oracle.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(oracle.profile.capabilities.shell, ShellCapability::Allow);
        assert_eq!(
            oracle.profile.capabilities.network,
            NetworkCapability::Allow
        );
        let backend_cfg = cfg.backend.get("codex").unwrap();
        let fallback = backend_cfg.fallback.as_ref().expect("fallback");
        assert_eq!(fallback.model, "codex/gpt-5.2@high");
    }

    #[test]
    fn rejects_readonly_for_opencode_on_resolve_only() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let path = td.path().join("cfg.json");
        write_cfg(
            &path,
            r#"{
  "backend": {
    "opencode": {
      "models": { "opencode-gpt-5": {} }
    }
  },
  "roles": {
    "reader": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "writer": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();
        let writer = cfg.resolve_profile(Some("writer")).unwrap();
        assert_eq!(writer.profile.backend_id, "opencode");

        let err = cfg.resolve_profile(Some("reader")).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("filesystem capability") && msg.contains("opencode"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn rejects_readonly_for_kimi_on_resolve_only() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let path = td.path().join("cfg.json");
        write_cfg(
            &path,
            r#"{
  "backend": {
    "kimi": {
      "models": { "kimi-k2": {} }
    }
  },
  "roles": {
    "reader": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "writer": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();
        let writer = cfg.resolve_profile(Some("writer")).unwrap();
        assert_eq!(writer.profile.backend_id, "kimi");

        let err = cfg.resolve_profile(Some("reader")).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("filesystem capability") && msg.contains("kimi"),
            "unexpected error: {msg}"
        );
    }

    fn write_cfg(path: &Path, json: &str) {
        std::fs::write(path, json).unwrap();
    }

    #[test]
    fn resolves_codex_variant_overrides_options() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "codex": {
      "models": {
        "gpt-5.2-codex": {
          "options": { "model_reasoning_effort": "high" },
          "variants": { "fast": { "model_reasoning_effort": "low" } }
        }
      }
    }
  },
  "roles": {
    "oracle": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "oracle-fast": {
      "model": "codex/gpt-5.2-codex@fast",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();

        let base = cfg.resolve_profile(Some("oracle")).unwrap();
        let base_effort =
            base.profile
                .options
                .get("model_reasoning_effort")
                .and_then(|v| match v {
                    OptionValue::String(s) => Some(s.as_str()),
                    _ => None,
                });
        assert_eq!(base_effort, Some("high"));

        let fast = cfg.resolve_profile(Some("oracle-fast")).unwrap();
        let fast_effort =
            fast.profile
                .options
                .get("model_reasoning_effort")
                .and_then(|v| match v {
                    OptionValue::String(s) => Some(s.as_str()),
                    _ => None,
                });
        assert_eq!(fast_effort, Some("low"));
    }

    #[test]
    fn parses_role_capabilities_read_write() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "codex": {
      "adapter": {
        "args_template": ["run"],
        "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 }
      },
      "models": { "gpt-5.2-codex": {} }
    }
  },
  "roles": {
    "reader": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "writer": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();

        let reader = cfg.resolve_profile(Some("reader")).unwrap();
        assert_eq!(
            reader.profile.capabilities.filesystem,
            FilesystemCapability::ReadOnly
        );
        assert_eq!(reader.profile.capabilities.shell, ShellCapability::Deny);
        assert_eq!(reader.profile.capabilities.network, NetworkCapability::Deny);

        let writer = cfg.resolve_profile(Some("writer")).unwrap();
        assert_eq!(
            writer.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(writer.profile.capabilities.shell, ShellCapability::Allow);
        assert_eq!(
            writer.profile.capabilities.network,
            NetworkCapability::Allow
        );
    }

    #[test]
    fn parses_capabilities_for_claude() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "claude": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "claude-opus-4-5-20251101": {} }
    },
    "codex": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gpt-5.2-codex": {} }
    },
    "gemini": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gemini-3-pro-preview": {} }
    },
    "opencode": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "opencode-gpt-5": {} }
    },
    "kimi": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "kimi-k2": {} }
    }
  },
  "roles": {
    "claude_reader": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "codex_reader": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "gemini_reader": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "opencode_reader": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "kimi_reader": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "claude_writer": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "codex_writer": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "gemini_writer": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "opencode_writer": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "kimi_writer": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();

        let reader = cfg.resolve_profile(Some("claude_reader")).unwrap();
        assert_eq!(reader.profile.backend_id, "claude");
        assert_eq!(
            reader.profile.capabilities.filesystem,
            FilesystemCapability::ReadOnly
        );
        assert_eq!(reader.profile.capabilities.shell, ShellCapability::Deny);
        assert_eq!(reader.profile.capabilities.network, NetworkCapability::Deny);

        let writer = cfg.resolve_profile(Some("claude_writer")).unwrap();
        assert_eq!(writer.profile.backend_id, "claude");
        assert_eq!(
            writer.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(writer.profile.capabilities.shell, ShellCapability::Allow);
        assert_eq!(
            writer.profile.capabilities.network,
            NetworkCapability::Allow
        );
    }

    #[test]
    fn parses_capabilities_for_codex() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "claude": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "claude-opus-4-5-20251101": {} }
    },
    "codex": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gpt-5.2-codex": {} }
    },
    "gemini": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gemini-3-pro-preview": {} }
    },
    "opencode": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "opencode-gpt-5": {} }
    },
    "kimi": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "kimi-k2": {} }
    }
  },
  "roles": {
    "claude_reader": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "codex_reader": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "gemini_reader": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "opencode_reader": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "kimi_reader": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "claude_writer": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "codex_writer": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "gemini_writer": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "opencode_writer": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "kimi_writer": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();

        let reader = cfg.resolve_profile(Some("codex_reader")).unwrap();
        assert_eq!(reader.profile.backend_id, "codex");
        assert_eq!(
            reader.profile.capabilities.filesystem,
            FilesystemCapability::ReadOnly
        );
        assert_eq!(reader.profile.capabilities.shell, ShellCapability::Deny);
        assert_eq!(reader.profile.capabilities.network, NetworkCapability::Deny);

        let writer = cfg.resolve_profile(Some("codex_writer")).unwrap();
        assert_eq!(writer.profile.backend_id, "codex");
        assert_eq!(
            writer.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(writer.profile.capabilities.shell, ShellCapability::Allow);
        assert_eq!(
            writer.profile.capabilities.network,
            NetworkCapability::Allow
        );
    }

    #[test]
    fn parses_capabilities_for_gemini() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "claude": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "claude-opus-4-5-20251101": {} }
    },
    "codex": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gpt-5.2-codex": {} }
    },
    "gemini": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gemini-3-pro-preview": {} }
    },
    "opencode": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "opencode-gpt-5": {} }
    },
    "kimi": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "kimi-k2": {} }
    }
  },
  "roles": {
    "claude_reader": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "codex_reader": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "gemini_reader": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "opencode_reader": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "kimi_reader": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "claude_writer": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "codex_writer": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "gemini_writer": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "opencode_writer": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "kimi_writer": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();

        let reader = cfg.resolve_profile(Some("gemini_reader")).unwrap();
        assert_eq!(reader.profile.backend_id, "gemini");
        assert_eq!(
            reader.profile.capabilities.filesystem,
            FilesystemCapability::ReadOnly
        );
        assert_eq!(reader.profile.capabilities.shell, ShellCapability::Deny);
        assert_eq!(reader.profile.capabilities.network, NetworkCapability::Deny);

        let writer = cfg.resolve_profile(Some("gemini_writer")).unwrap();
        assert_eq!(writer.profile.backend_id, "gemini");
        assert_eq!(
            writer.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(writer.profile.capabilities.shell, ShellCapability::Allow);
        assert_eq!(
            writer.profile.capabilities.network,
            NetworkCapability::Allow
        );
    }

    #[test]
    fn parses_capabilities_for_opencode() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "claude": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "claude-opus-4-5-20251101": {} }
    },
    "codex": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gpt-5.2-codex": {} }
    },
    "gemini": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gemini-3-pro-preview": {} }
    },
    "opencode": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "opencode-gpt-5": {} }
    },
    "kimi": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "kimi-k2": {} }
    }
  },
  "roles": {
    "claude_reader": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "codex_reader": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "gemini_reader": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "opencode_reader": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "kimi_reader": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "claude_writer": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "codex_writer": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "gemini_writer": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "opencode_writer": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "kimi_writer": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();

        let reader = cfg.resolve_profile(Some("opencode_reader")).unwrap();
        assert_eq!(reader.profile.backend_id, "opencode");
        assert_eq!(
            reader.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(reader.profile.capabilities.shell, ShellCapability::Deny);
        assert_eq!(reader.profile.capabilities.network, NetworkCapability::Deny);

        let writer = cfg.resolve_profile(Some("opencode_writer")).unwrap();
        assert_eq!(writer.profile.backend_id, "opencode");
        assert_eq!(
            writer.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(writer.profile.capabilities.shell, ShellCapability::Allow);
        assert_eq!(
            writer.profile.capabilities.network,
            NetworkCapability::Allow
        );
    }

    #[test]
    fn parses_capabilities_for_kimi() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = td.path().join("config.json");
        write_cfg(
            &cfg_path,
            r#"{
  "backend": {
    "claude": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "claude-opus-4-5-20251101": {} }
    },
    "codex": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gpt-5.2-codex": {} }
    },
    "gemini": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "gemini-3-pro-preview": {} }
    },
    "opencode": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "opencode-gpt-5": {} }
    },
    "kimi": {
      "adapter": { "args_template": ["run"], "output_parser": { "type": "regex", "session_id_pattern": "x", "message_capture_group": 1 } },
      "models": { "kimi-k2": {} }
    }
  },
  "roles": {
    "claude_reader": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "codex_reader": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "gemini_reader": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-only", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "opencode_reader": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "kimi_reader": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "claude_writer": {
      "model": "claude/claude-opus-4-5-20251101",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "codex_writer": {
      "model": "codex/gpt-5.2-codex",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "gemini_writer": {
      "model": "gemini/gemini-3-pro-preview",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "opencode_writer": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    },
    "kimi_writer": {
      "model": "kimi/kimi-k2",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    }
  }
}"#,
        );

        let loader = ConfigLoader::new(Some(cfg_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();

        let reader = cfg.resolve_profile(Some("kimi_reader")).unwrap();
        assert_eq!(reader.profile.backend_id, "kimi");
        assert_eq!(
            reader.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(reader.profile.capabilities.shell, ShellCapability::Deny);
        assert_eq!(reader.profile.capabilities.network, NetworkCapability::Deny);

        let writer = cfg.resolve_profile(Some("kimi_writer")).unwrap();
        assert_eq!(writer.profile.backend_id, "kimi");
        assert_eq!(
            writer.profile.capabilities.filesystem,
            FilesystemCapability::ReadWrite
        );
        assert_eq!(writer.profile.capabilities.shell, ShellCapability::Allow);
        assert_eq!(
            writer.profile.capabilities.network,
            NetworkCapability::Allow
        );
    }
}
