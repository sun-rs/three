use crate::config::{
    AdapterConfig, Capabilities, FilesystemCapability, JsonStreamFallback, OptionValue,
    OutputParserConfig, OutputPick, PromptTransport,
};
use anyhow::{anyhow, Context, Result};
use minijinja::{context, Environment};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone)]
pub struct GenericOptions {
    pub backend_id: String,
    pub adapter: AdapterConfig,
    pub prompt: String,
    pub workdir: PathBuf,
    pub session_id: Option<String>,
    pub resume: bool,
    pub model: String,
    pub options: BTreeMap<String, OptionValue>,
    pub capabilities: Capabilities,
    pub fallback_error_patterns: Vec<String>,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct GenericResult {
    pub session_id: String,
    pub agent_messages: String,
    pub warnings: Option<String>,
}

const DEFAULT_PROMPT_MAX_CHARS: usize = 32 * 1024;
const KIMI_READONLY_GUARDRAIL: &str = "不允许写文件";

pub async fn run(opts: GenericOptions) -> Result<GenericResult> {
    let timeout_duration = Duration::from_secs(opts.timeout_secs);
    timeout(timeout_duration, run_internal(opts))
        .await
        .context("backend command timed out")?
}

pub fn render_args(opts: &GenericOptions) -> Result<Vec<String>> {
    let prompt = apply_prompt_guardrails(&opts.backend_id, &opts.capabilities, &opts.prompt);
    let transport = resolve_prompt_transport(&opts.adapter, &prompt);
    let env = Environment::new();
    let options_val = serde_json::to_value(&opts.options).context("serialize options")?;
    let capabilities_val =
        serde_json::to_value(&opts.capabilities).context("serialize capabilities")?;
    let include_directories = detect_include_directories(&opts.prompt, &opts.workdir);
    let prompt_for_args = match transport {
        ResolvedPromptTransport::Arg => prompt.as_str(),
        ResolvedPromptTransport::Stdin => "",
    };
    let ctx = context! {
        prompt => prompt_for_args,
        model => opts.model,
        session_id => opts.session_id,
        resume => opts.resume,
        workdir => opts.workdir.to_string_lossy().to_string(),
        options => options_val,
        capabilities => capabilities_val,
        include_directories => include_directories,
        prompt_transport => transport.as_str(),
    };

    let mut args: Vec<String> = Vec::new();
    for token in &opts.adapter.args_template {
        let rendered = env
            .render_str(token, &ctx)
            .with_context(|| format!("failed to render template token: {token}"))?;
        let trimmed = rendered.trim();
        if !trimmed.is_empty() {
            args.push(trimmed.to_string());
        }
    }

    Ok(args)
}

async fn run_internal(opts: GenericOptions) -> Result<GenericResult> {
    let command = resolve_command(&opts.backend_id);
    let prompt = apply_prompt_guardrails(&opts.backend_id, &opts.capabilities, &opts.prompt);
    let transport = resolve_prompt_transport(&opts.adapter, &prompt);
    let args = render_args(&opts)?;

    let mut cmd = Command::new(command);
    cmd.args(&args)
        .current_dir(&opts.workdir)
        .stdin(match transport {
            ResolvedPromptTransport::Arg => Stdio::null(),
            ResolvedPromptTransport::Stdin => Stdio::piped(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().context("failed to spawn backend")?;
    if let ResolvedPromptTransport::Stdin = transport {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .context("failed to write prompt to stdin")?;
        }
    }

    let output = child
        .wait_with_output()
        .await
        .context("failed to spawn backend")?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if let Some(model_err) = detect_model_error(
        &stdout,
        &stderr,
        &opts.fallback_error_patterns,
        output.status.success(),
    ) {
        return Err(anyhow!("model_not_found: {model_err}"));
    }

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        return Err(anyhow!(
            "backend exited with status {code}. stderr: {stderr}"
        ));
    }

    let (session_id, agent_messages) = parse_output(&opts.adapter.output_parser, &stdout)?;

    Ok(GenericResult {
        session_id,
        agent_messages,
        warnings: if stderr.trim().is_empty() {
            None
        } else {
            Some(stderr)
        },
    })
}

fn resolve_command(backend_id: &str) -> String {
    match backend_id {
        "codex" => std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string()),
        "gemini" => std::env::var("GEMINI_BIN").unwrap_or_else(|_| "gemini".to_string()),
        _ => backend_id.to_string(),
    }
}

fn detect_include_directories(prompt: &str, workdir: &Path) -> String {
    let mut dirs = BTreeSet::new();
    let workdir_norm = workdir
        .canonicalize()
        .unwrap_or_else(|_| workdir.to_path_buf());

    for raw in prompt.split_whitespace() {
        let token = trim_path_token(raw);
        if token.is_empty() {
            continue;
        }
        let path = PathBuf::from(token);
        if !path.is_absolute() {
            continue;
        }
        if path.starts_with(&workdir_norm) {
            continue;
        }

        let include_dir = if path.exists() {
            if path.is_file() {
                path.parent().map(|p| p.to_path_buf())
            } else if path.is_dir() {
                Some(path.clone())
            } else {
                None
            }
        } else if path.extension().is_some() {
            path.parent().map(|p| p.to_path_buf())
        } else {
            Some(path.clone())
        };
        if let Some(dir) = include_dir {
            if !dir.as_os_str().is_empty() && dir.is_dir() {
                dirs.insert(dir.to_string_lossy().to_string());
            }
        }
    }

    dirs.into_iter().collect::<Vec<_>>().join(",")
}

fn trim_path_token(raw: &str) -> String {
    let trimmed = raw.trim_matches(|c: char| {
        matches!(
            c,
            '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
        )
    });
    let trimmed = trimmed.trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':'));
    trimmed.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedPromptTransport {
    Arg,
    Stdin,
}

impl ResolvedPromptTransport {
    fn as_str(&self) -> &'static str {
        match self {
            ResolvedPromptTransport::Arg => "arg",
            ResolvedPromptTransport::Stdin => "stdin",
        }
    }
}

fn resolve_prompt_transport(adapter: &AdapterConfig, prompt: &str) -> ResolvedPromptTransport {
    let configured = adapter.prompt_transport.unwrap_or(PromptTransport::Arg);
    match configured {
        PromptTransport::Arg => ResolvedPromptTransport::Arg,
        PromptTransport::Stdin => ResolvedPromptTransport::Stdin,
        PromptTransport::Auto => {
            let max_chars = adapter.prompt_max_chars.unwrap_or(DEFAULT_PROMPT_MAX_CHARS);
            if prompt.len() > max_chars {
                ResolvedPromptTransport::Stdin
            } else {
                ResolvedPromptTransport::Arg
            }
        }
    }
}

fn apply_prompt_guardrails(backend_id: &str, capabilities: &Capabilities, prompt: &str) -> String {
    if backend_id == "kimi" && capabilities.filesystem == FilesystemCapability::ReadOnly {
        if prompt.contains(KIMI_READONLY_GUARDRAIL) {
            return prompt.to_string();
        }
        if prompt.ends_with('\n') {
            format!("{prompt}{KIMI_READONLY_GUARDRAIL}")
        } else {
            format!("{prompt}\n{KIMI_READONLY_GUARDRAIL}")
        }
    } else {
        prompt.to_string()
    }
}

fn parse_output(parser: &OutputParserConfig, stdout: &str) -> Result<(String, String)> {
    match parser {
        OutputParserConfig::JsonStream {
            session_id_path,
            message_path,
            pick,
            fallback,
        } => parse_json_stream(
            stdout,
            session_id_path,
            message_path,
            pick.unwrap_or(OutputPick::Last),
            *fallback,
        ),
        OutputParserConfig::JsonObject {
            message_path,
            session_id_path,
        } => parse_json_object(stdout, session_id_path.as_deref(), message_path),
        OutputParserConfig::Regex {
            session_id_pattern,
            message_capture_group,
        } => parse_regex(stdout, session_id_pattern, *message_capture_group),
        OutputParserConfig::Text => parse_text(stdout),
    }
}

fn detect_model_error(
    stdout: &str,
    stderr: &str,
    patterns: &[String],
    status_success: bool,
) -> Option<String> {
    let patterns: Vec<String> = patterns
        .iter()
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    if patterns.is_empty() {
        return None;
    }

    let matches = |text: &str| {
        let lower = text.to_ascii_lowercase();
        patterns.iter().any(|p| lower.contains(p))
    };

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if ty == "error" || ty == "turn.failed" {
                let msg = v
                    .get("message")
                    .and_then(|m| m.as_str())
                    .or_else(|| {
                        v.get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(|m| m.as_str())
                    })
                    .unwrap_or("");
                if matches(msg) {
                    return Some(msg.to_string());
                }
                if matches(trimmed) {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    if !status_success {
        for line in stderr.lines() {
            if matches(line) {
                return Some(line.to_string());
            }
        }
        for line in stdout.lines() {
            if matches(line) {
                return Some(line.to_string());
            }
        }
    }

    None
}

fn parse_json_stream(
    stdout: &str,
    session_id_path: &str,
    message_path: &str,
    pick: OutputPick,
    fallback: Option<JsonStreamFallback>,
) -> Result<(String, String)> {
    let mut session_id: Option<String> = None;
    let mut message: Option<String> = None;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(err) => {
                if fallback.is_some() {
                    continue;
                }
                return Err(anyhow::Error::from(err))
                    .with_context(|| format!("failed to parse json line: {trimmed}"));
            }
        };

        if let Some(val) = json_path_get(&v, session_id_path) {
            if let Some(s) = val.as_str() {
                if pick == OutputPick::First {
                    session_id.get_or_insert_with(|| s.to_string());
                } else {
                    session_id = Some(s.to_string());
                }
            }
        }

        if let Some(val) = json_path_get(&v, message_path) {
            if let Some(s) = val.as_str() {
                if pick == OutputPick::First {
                    message.get_or_insert_with(|| s.to_string());
                } else {
                    message = Some(s.to_string());
                }
            }
        }
    }

    let session_id = session_id.ok_or_else(|| anyhow!("failed to get session_id from output"))?;
    let mut message = message.unwrap_or_default();
    if message.trim().is_empty() {
        if let Some(JsonStreamFallback::Codex) = fallback {
            if let Some(fallback_message) = parse_codex_jsonl_message(stdout) {
                message = fallback_message;
            }
        }
    }
    Ok((session_id, message))
}

fn parse_codex_jsonl_message(stdout: &str) -> Option<String> {
    let mut messages: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if v.get("type") == Some(&Value::String("item.completed".to_string())) {
            if let Some(item) = v.get("item") {
                if item.get("type") == Some(&Value::String("agent_message".to_string())) {
                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                        messages.push(text.to_string());
                    }
                }
            }
        }

        if v.get("type") == Some(&Value::String("message".to_string())) {
            if let Some(content) = v.get("content") {
                if let Some(text) = content.as_str() {
                    messages.push(text.to_string());
                } else if let Some(arr) = content.as_array() {
                    for part in arr {
                        if part.get("type") == Some(&Value::String("text".to_string())) {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                messages.push(text.to_string());
                            }
                        }
                    }
                }
            }
        }

        if v.get("type") == Some(&Value::String("output_text".to_string())) {
            if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
                messages.push(text.to_string());
            }
        }
    }

    if messages.is_empty() {
        None
    } else {
        Some(messages.join("\n"))
    }
}

fn parse_json_object(
    stdout: &str,
    session_id_path: Option<&str>,
    message_path: &str,
) -> Result<(String, String)> {
    let trimmed = stdout.trim();
    let v: Value = serde_json::from_str(trimmed)
        .with_context(|| format!("failed to parse json output: {trimmed}"))?;

    let message = json_path_get(&v, message_path)
        .and_then(|val| val.as_str())
        .unwrap_or_default()
        .to_string();

    let session_id = session_id_path
        .and_then(|path| {
            if path.trim().is_empty() {
                None
            } else {
                json_path_get(&v, path)
                    .and_then(|val| val.as_str())
                    .map(|s| s.to_string())
            }
        })
        .unwrap_or_else(|| "stateless".to_string());

    Ok((session_id, message))
}

fn parse_regex(
    stdout: &str,
    pattern: &str,
    message_capture_group: usize,
) -> Result<(String, String)> {
    let re = Regex::new(pattern).with_context(|| format!("invalid regex: {pattern}"))?;
    let caps = re
        .captures(stdout)
        .ok_or_else(|| anyhow!("failed to match regex: {pattern}"))?;

    let session_id = caps
        .get(1)
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| anyhow!("regex did not capture session_id"))?;

    let message = caps
        .get(message_capture_group)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();

    Ok((session_id, message))
}

fn parse_text(stdout: &str) -> Result<(String, String)> {
    let message = stdout.trim().to_string();
    Ok(("stateless".to_string(), message))
}

fn json_path_get<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = value;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_catalog::embedded_adapter_catalog;
    use crate::config::{
        ConfigLoader, FilesystemCapability, NetworkCapability, OutputParserConfig, ShellCapability,
    };
    use std::collections::BTreeMap;
    use std::path::Path;

    fn render_args_for_role_with_prompt(
        cfg_path: &Path,
        repo: &Path,
        role: &str,
        prompt: &str,
    ) -> Vec<String> {
        let loader = ConfigLoader::new(Some(cfg_path.to_path_buf()));
        let cfg = loader.load_for_repo(repo).unwrap().unwrap();
        let rp = cfg.resolve_profile(Some(role)).unwrap();
        render_args(&GenericOptions {
            backend_id: rp.profile.backend_id.clone(),
            adapter: rp.profile.adapter.clone(),
            prompt: prompt.to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: false,
            model: rp.profile.model.clone(),
            options: rp.profile.options.clone(),
            capabilities: rp.profile.capabilities.clone(),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap()
    }

    fn render_args_for_role(cfg_path: &Path, repo: &Path, role: &str) -> Vec<String> {
        render_args_for_role_with_prompt(cfg_path, repo, role, "ping")
    }

    fn assert_gemini_render(args: &[String], model: &str, expect_sandbox: bool, expect_plan: bool) {
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"json".to_string()));
        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&model.to_string()));
        assert!(args.contains(&"--prompt".to_string()));
        let has_plan = args.iter().any(|token| token == "--approval-mode")
            && args.iter().any(|token| token == "plan");
        assert_eq!(has_plan, expect_plan);
        let has_sandbox = args.iter().any(|token| token == "--sandbox");
        assert_eq!(has_sandbox, expect_sandbox);
    }

    fn load_codex_adapter() -> AdapterConfig {
        let catalog = embedded_adapter_catalog();
        catalog
            .adapters
            .get("codex")
            .expect("codex adapter")
            .clone()
    }

    fn load_opencode_adapter() -> AdapterConfig {
        let catalog = embedded_adapter_catalog();
        catalog
            .adapters
            .get("opencode")
            .expect("opencode adapter")
            .clone()
    }

    fn base_capabilities(filesystem: FilesystemCapability) -> Capabilities {
        Capabilities {
            filesystem,
            shell: ShellCapability::Deny,
            network: NetworkCapability::Deny,
            tools: Vec::new(),
        }
    }

    fn render_codex_args(
        model: &str,
        session_id: Option<&str>,
        filesystem: FilesystemCapability,
        options: BTreeMap<String, OptionValue>,
        repo: &Path,
    ) -> Vec<String> {
        render_args(&GenericOptions {
            backend_id: "codex".to_string(),
            adapter: load_codex_adapter(),
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: session_id.map(|s| s.to_string()),
            resume: false,
            model: model.to_string(),
            options,
            capabilities: base_capabilities(filesystem),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap()
    }

    fn render_opencode_args(model: &str, session_id: Option<&str>, repo: &Path) -> Vec<String> {
        render_args(&GenericOptions {
            backend_id: "opencode".to_string(),
            adapter: load_opencode_adapter(),
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: session_id.map(|s| s.to_string()),
            resume: false,
            model: model.to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap()
    }

    #[test]
    fn cfgtest_parse_text_output_returns_message_and_stateless() {
        let (session_id, message) =
            parse_output(&OutputParserConfig::Text, "hello\n").expect("parse text");
        assert_eq!(session_id, "stateless");
        assert_eq!(message, "hello");
    }

    #[test]
    fn cfgtest_json_stream_fallback_codex_recovers_message() {
        let stdout = r#"{"type":"thread.started","thread_id":"sess-1"}
{"type":"item.completed","item":{"type":"agent_message","text":"hi"}}
"#;
        let (session_id, message) = parse_output(
            &OutputParserConfig::JsonStream {
                session_id_path: "thread_id".to_string(),
                message_path: "item.text".to_string(),
                pick: Some(OutputPick::Last),
                fallback: Some(JsonStreamFallback::Codex),
            },
            stdout,
        )
        .expect("parse json stream");
        assert_eq!(session_id, "sess-1");
        assert_eq!(message, "hi");
    }

    #[test]
    fn cfgtest_render_kimi_readonly_appends_guardrail() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog.adapters.get("kimi").expect("kimi adapter").clone();

        let args = render_args(&GenericOptions {
            backend_id: "kimi".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: false,
            model: "kimi-for-coding".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--thinking".to_string()));
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"text".to_string()));
        assert!(args.contains(&"--final-message-only".to_string()));
        assert!(args.contains(&"--work-dir".to_string()));
        assert!(args.contains(&repo.to_string_lossy().to_string()));
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"kimi-for-coding".to_string()));

        let prompt_idx = args
            .iter()
            .position(|v| v == "--prompt")
            .expect("prompt flag");
        let prompt_val = args.get(prompt_idx + 1).expect("prompt value");
        assert!(prompt_val.contains("ping"));
        assert!(prompt_val.contains("不允许写文件"));
    }

    #[test]
    fn cfgtest_kimi_readonly_guardrail_applies_to_prompt() {
        let prompt = "ping";
        let guarded = apply_prompt_guardrails(
            "kimi",
            &base_capabilities(FilesystemCapability::ReadOnly),
            prompt,
        );
        assert!(guarded.contains("ping"));
        assert!(guarded.contains("不允许写文件"));
    }

    #[test]
    fn cfgtest_render_kimi_readwrite_no_guardrail_and_session() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog.adapters.get("kimi").expect("kimi adapter").clone();

        let args = render_args(&GenericOptions {
            backend_id: "kimi".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: Some("sess-1".to_string()),
            resume: false,
            model: "kimi-for-coding".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadWrite),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(args.contains(&"--session".to_string()));
        assert!(args.contains(&"sess-1".to_string()));
        let prompt_idx = args
            .iter()
            .position(|v| v == "--prompt")
            .expect("prompt flag");
        let prompt_val = args.get(prompt_idx + 1).expect("prompt value");
        assert!(!prompt_val.contains("不允许写文件"));
    }

    #[test]
    fn cfgtest_render_kimi_resume_uses_continue_flag() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog.adapters.get("kimi").expect("kimi adapter").clone();

        let args = render_args(&GenericOptions {
            backend_id: "kimi".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: true,
            model: "kimi-for-coding".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadWrite),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(args.contains(&"--continue".to_string()));
        assert!(!args.contains(&"--session".to_string()));
    }

    #[test]
    fn cfgtest_render_claude_default_model_skips_model_flag() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog
            .adapters
            .get("claude")
            .expect("claude adapter")
            .clone();

        let args = render_args(&GenericOptions {
            backend_id: "claude".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: false,
            model: "default".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(!args.contains(&"--model".to_string()));
        assert!(!args.contains(&"default".to_string()));
    }

    #[test]
    fn cfgtest_render_gemini_default_model_skips_model_flag() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog
            .adapters
            .get("gemini")
            .expect("gemini adapter")
            .clone();

        let args = render_args(&GenericOptions {
            backend_id: "gemini".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: false,
            model: "default".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(!args.contains(&"-m".to_string()));
        assert!(!args.contains(&"default".to_string()));
    }

    #[test]
    fn cfgtest_render_codex_default_model_skips_model_flag() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let args = render_codex_args(
            "default",
            Some("sess-1"),
            FilesystemCapability::ReadOnly,
            BTreeMap::new(),
            &repo,
        );

        assert!(!args.contains(&"--model".to_string()));
        assert!(!args.iter().any(|t| t.starts_with("model=")));
        assert!(!args.contains(&"default".to_string()));
    }

    #[test]
    fn cfgtest_render_kimi_default_model_skips_model_flag() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog.adapters.get("kimi").expect("kimi adapter").clone();

        let args = render_args(&GenericOptions {
            backend_id: "kimi".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: false,
            model: "default".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(!args.contains(&"--model".to_string()));
        assert!(!args.contains(&"default".to_string()));
    }

    #[test]
    fn cfgtest_render_opencode_default_model_skips_model_flag() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let args = render_opencode_args("default", None, &repo);
        assert_eq!(args.first().map(String::as_str), Some("run"));
        assert!(args.contains(&"--format".to_string()));
        assert!(args.contains(&"json".to_string()));
        assert!(!args.contains(&"-m".to_string()));
        assert!(!args.contains(&"default".to_string()));
    }

    #[test]
    fn cfgtest_render_opencode_session_includes_format_and_session() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let args = render_opencode_args("opencode-gpt-5", Some("sess-1"), &repo);
        assert_eq!(args.first().map(String::as_str), Some("run"));
        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&"opencode-gpt-5".to_string()));
        assert!(args.contains(&"-s".to_string()));
        assert!(args.contains(&"sess-1".to_string()));
        assert!(args.contains(&"--format".to_string()));
        assert!(args.contains(&"json".to_string()));
    }

    #[test]
    fn cfgtest_render_gemini_reader_args_match_example() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let cfg_path = crate::test_utils::example_config_path();
        let args = render_args_for_role(&cfg_path, &repo, "researcher");
        assert_gemini_render(&args, "gemini-3-pro-preview", true, true);
    }

    #[test]
    fn cfgtest_render_gemini_include_directories_for_external_paths() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let outside = td.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let outside_file = outside.join("note.txt");
        std::fs::write(&outside_file, "data").unwrap();

        let cfg_path = crate::test_utils::example_config_path();
        let prompt = format!("Read {}", outside_file.display());
        let args = render_args_for_role_with_prompt(&cfg_path, &repo, "researcher", &prompt);
        assert!(args.contains(&"--include-directories".to_string()));
        assert!(args.contains(&outside.to_string_lossy().to_string()));
    }

    #[test]
    fn cfgtest_render_gemini_include_directories_ignores_persona_tags() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let cfg_path = crate::test_utils::example_config_path();
        let prompt = "[THREE_PERSONA id=researcher]\nfoo\n[/THREE_PERSONA]\nRead this";
        let args = render_args_for_role_with_prompt(&cfg_path, &repo, "researcher", prompt);
        assert!(!args.contains(&"--include-directories".to_string()));
        assert!(!args.iter().any(|t| t == "/THREE_PERSONA"));
    }

    fn long_prompt() -> String {
        "x".repeat(40_000)
    }

    #[test]
    fn cfgtest_prompt_transport_auto_omits_prompt_for_claude() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog
            .adapters
            .get("claude")
            .expect("claude adapter")
            .clone();
        let prompt = long_prompt();

        let args = render_args(&GenericOptions {
            backend_id: "claude".to_string(),
            adapter,
            prompt: prompt.clone(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: false,
            model: "claude-sonnet-4-5-20250929".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadWrite),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(!args.contains(&prompt));
    }

    #[test]
    fn cfgtest_prompt_transport_auto_omits_prompt_for_gemini() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog
            .adapters
            .get("gemini")
            .expect("gemini adapter")
            .clone();
        let prompt = long_prompt();

        let args = render_args(&GenericOptions {
            backend_id: "gemini".to_string(),
            adapter,
            prompt: prompt.clone(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: false,
            model: "gemini-3-pro-preview".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadWrite),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(!args.contains(&"--prompt".to_string()));
        assert!(!args.contains(&prompt));
    }

    #[test]
    fn cfgtest_prompt_transport_auto_omits_prompt_for_kimi() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog.adapters.get("kimi").expect("kimi adapter").clone();
        let prompt = long_prompt();

        let args = render_args(&GenericOptions {
            backend_id: "kimi".to_string(),
            adapter,
            prompt: prompt.clone(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: false,
            model: "kimi-for-coding".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadWrite),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(!args.contains(&"--prompt".to_string()));
        assert!(!args.contains(&prompt));
    }

    #[test]
    fn cfgtest_prompt_transport_auto_omits_prompt_for_opencode() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let catalog = embedded_adapter_catalog();
        let adapter = catalog
            .adapters
            .get("opencode")
            .expect("opencode adapter")
            .clone();
        let prompt = long_prompt();

        let args = render_args(&GenericOptions {
            backend_id: "opencode".to_string(),
            adapter,
            prompt: prompt.clone(),
            workdir: repo.to_path_buf(),
            session_id: None,
            resume: false,
            model: "opencode-gpt-5".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadWrite),
            fallback_error_patterns: Vec::new(),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(!args.contains(&prompt));
    }

    #[test]
    fn cfgtest_render_codex_readonly_no_session_uses_model_and_sandbox() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let mut options = BTreeMap::new();
        options.insert(
            "model_reasoning_effort".to_string(),
            OptionValue::String("high".to_string()),
        );

        let args = render_codex_args(
            "gpt-5.2-codex",
            None,
            FilesystemCapability::ReadOnly,
            options,
            &repo,
        );

        assert!(args.contains(&"exec".to_string()));
        assert!(args.contains(&"--sandbox".to_string()));
        assert!(args.contains(&"read-only".to_string()));
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"gpt-5.2-codex".to_string()));
        assert!(args.iter().any(|t| t == "model_reasoning_effort=high"));
        assert!(args.contains(&"--skip-git-repo-check".to_string()));
        assert!(args.contains(&"-C".to_string()));
        assert!(args.contains(&repo.to_string_lossy().to_string()));
        assert!(args.contains(&"--json".to_string()));
        assert!(!args.contains(&"resume".to_string()));
        assert!(!args.iter().any(|t| t.starts_with("model=")));
    }

    #[test]
    fn cfgtest_render_codex_readwrite_resume_uses_config_model() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let mut options = BTreeMap::new();
        options.insert(
            "model_reasoning_effort".to_string(),
            OptionValue::String("medium".to_string()),
        );

        let args = render_codex_args(
            "gpt-5.2",
            Some("sess-1"),
            FilesystemCapability::ReadWrite,
            options,
            &repo,
        );

        assert!(args.contains(&"exec".to_string()));
        assert!(args.contains(&"--sandbox".to_string()));
        assert!(args.contains(&"workspace-write".to_string()));
        assert!(!args.contains(&"--model".to_string()));
        assert!(args.iter().any(|t| t == "model=gpt-5.2"));
        assert!(args.iter().any(|t| t == "model_reasoning_effort=medium"));
        assert!(args.contains(&"--skip-git-repo-check".to_string()));
        assert!(!args.contains(&"-C".to_string()));
        assert!(args.contains(&"--json".to_string()));
        assert!(args.contains(&"resume".to_string()));
        assert!(args.contains(&"sess-1".to_string()));
    }
}
