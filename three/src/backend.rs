use crate::config::{AdapterConfig, Capabilities, OptionValue, OutputParserConfig, OutputPick};
use anyhow::{anyhow, Context, Result};
use minijinja::{context, Environment};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone)]
pub struct GenericOptions {
    pub backend_id: String,
    pub adapter: AdapterConfig,
    pub prompt: String,
    pub workdir: PathBuf,
    pub session_id: Option<String>,
    pub model: String,
    pub options: BTreeMap<String, OptionValue>,
    pub capabilities: Capabilities,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct GenericResult {
    pub session_id: String,
    pub agent_messages: String,
    pub warnings: Option<String>,
}

pub async fn run(opts: GenericOptions) -> Result<GenericResult> {
    let timeout_duration = Duration::from_secs(opts.timeout_secs);
    timeout(timeout_duration, run_internal(opts))
        .await
        .context("backend command timed out")?
}

pub fn render_args(opts: &GenericOptions) -> Result<Vec<String>> {
    let env = Environment::new();
    let options_val = serde_json::to_value(&opts.options).context("serialize options")?;
    let capabilities_val = serde_json::to_value(&opts.capabilities).context("serialize capabilities")?;
    let include_directories = detect_include_directories(&opts.prompt, &opts.workdir);
    let ctx = context! {
        prompt => opts.prompt,
        model => opts.model,
        session_id => opts.session_id,
        workdir => opts.workdir.to_string_lossy().to_string(),
        options => options_val,
        capabilities => capabilities_val,
        include_directories => include_directories,
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
    let args = render_args(&opts)?;

    let mut cmd = Command::new(command);
    cmd.args(&args)
        .current_dir(&opts.workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = cmd.output().await.context("failed to spawn backend")?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        return Err(anyhow!("backend exited with status {code}. stderr: {stderr}"));
    }

    let (session_id, agent_messages) = parse_output(&opts.adapter.output_parser, &stdout)?;

    Ok(GenericResult {
        session_id,
        agent_messages,
        warnings: if stderr.trim().is_empty() { None } else { Some(stderr) },
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
    let workdir_norm = workdir.canonicalize().unwrap_or_else(|_| workdir.to_path_buf());

    for raw in prompt.split_whitespace() {
        let token = trim_path_token(raw);
        if token.is_empty() || !token.starts_with('/') {
            continue;
        }
        let path = PathBuf::from(token);
        if !path.is_absolute() {
            continue;
        }
        if path.starts_with(&workdir_norm) {
            continue;
        }

        let include_dir = if path.extension().is_some() {
            path.parent().map(|p| p.to_path_buf())
        } else {
            Some(path.clone())
        };
        if let Some(dir) = include_dir {
            if !dir.as_os_str().is_empty() {
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

fn parse_output(parser: &OutputParserConfig, stdout: &str) -> Result<(String, String)> {
    match parser {
        OutputParserConfig::JsonStream {
            session_id_path,
            message_path,
            pick,
        } => parse_json_stream(stdout, session_id_path, message_path, pick.unwrap_or(OutputPick::Last)),
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

fn parse_json_stream(
    stdout: &str,
    session_id_path: &str,
    message_path: &str,
    pick: OutputPick,
) -> Result<(String, String)> {
    let mut session_id: Option<String> = None;
    let mut message: Option<String> = None;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(trimmed)
            .with_context(|| format!("failed to parse json line: {trimmed}"))?;

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
    let message = message.unwrap_or_default();
    Ok((session_id, message))
}

fn parse_json_object(stdout: &str, session_id_path: Option<&str>, message_path: &str) -> Result<(String, String)> {
    let trimmed = stdout.trim();
    let v: Value =
        serde_json::from_str(trimmed).with_context(|| format!("failed to parse json output: {trimmed}"))?;

    let message = json_path_get(&v, message_path)
        .and_then(|val| val.as_str())
        .unwrap_or_default()
        .to_string();

    let session_id = session_id_path
        .and_then(|path| {
            if path.trim().is_empty() {
                None
            } else {
                json_path_get(&v, path).and_then(|val| val.as_str()).map(|s| s.to_string())
            }
        })
        .unwrap_or_else(|| "stateless".to_string());

    Ok((session_id, message))
}

fn parse_regex(stdout: &str, pattern: &str, message_capture_group: usize) -> Result<(String, String)> {
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
    use crate::config::{
        AdapterCatalog, ConfigLoader, FilesystemCapability, NetworkCapability, OutputParserConfig,
        ShellCapability,
    };
    use std::path::Path;
    use std::collections::BTreeMap;

    fn render_args_for_brain_with_prompt(
        cfg_path: &Path,
        adapter_path: &Path,
        repo: &Path,
        brain: &str,
        prompt: &str,
    ) -> Vec<String> {
        let loader =
            ConfigLoader::new(Some(cfg_path.to_path_buf())).with_adapter_path(Some(adapter_path.to_path_buf()));
        let cfg = loader.load_for_repo(repo).unwrap().unwrap();
        let rp = cfg.resolve_profile(Some(brain), None).unwrap();
        render_args(&GenericOptions {
            backend_id: rp.profile.backend_id.clone(),
            adapter: rp.profile.adapter.clone(),
            prompt: prompt.to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            model: rp.profile.model.clone(),
            options: rp.profile.options.clone(),
            capabilities: rp.profile.capabilities.clone(),
            timeout_secs: 5,
        })
        .unwrap()
    }

    fn render_args_for_brain(
        cfg_path: &Path,
        adapter_path: &Path,
        repo: &Path,
        brain: &str,
    ) -> Vec<String> {
        render_args_for_brain_with_prompt(cfg_path, adapter_path, repo, brain, "ping")
    }

    fn assert_gemini_render(args: &[String], model: &str, expect_sandbox: bool, expect_plan: bool) {
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"json".to_string()));
        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&model.to_string()));
        assert!(args.contains(&"--prompt".to_string()));
        let has_plan =
            args.iter().any(|token| token == "--approval-mode") && args.iter().any(|token| token == "plan");
        assert_eq!(has_plan, expect_plan);
        let has_sandbox = args.iter().any(|token| token == "--sandbox");
        assert_eq!(has_sandbox, expect_sandbox);
    }

    fn load_codex_adapter() -> AdapterConfig {
        let (_, adapter_path) = crate::test_utils::example_config_paths();
        let catalog = AdapterCatalog::load(&adapter_path).expect("load adapter catalog");
        catalog.adapters.get("codex").expect("codex adapter").clone()
    }

    fn load_opencode_adapter() -> AdapterConfig {
        let (_, adapter_path) = crate::test_utils::example_config_paths();
        let catalog = AdapterCatalog::load(&adapter_path).expect("load adapter catalog");
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
            model: model.to_string(),
            options,
            capabilities: base_capabilities(filesystem),
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
            model: model.to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
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
    fn cfgtest_render_kimi_readonly_appends_guardrail() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let (_, adapter_path) = crate::test_utils::example_config_paths();
        let catalog = AdapterCatalog::load(&adapter_path).expect("load adapter catalog");
        let adapter = catalog.adapters.get("kimi").expect("kimi adapter").clone();

        let args = render_args(&GenericOptions {
            backend_id: "kimi".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            model: "kimi-for-coding".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
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

        let prompt_idx = args.iter().position(|v| v == "--prompt").expect("prompt flag");
        let prompt_val = args.get(prompt_idx + 1).expect("prompt value");
        assert!(prompt_val.contains("ping"));
        assert!(prompt_val.contains("不允许写文件"));
    }

    #[test]
    fn cfgtest_render_kimi_readwrite_no_guardrail_and_session() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let (_, adapter_path) = crate::test_utils::example_config_paths();
        let catalog = AdapterCatalog::load(&adapter_path).expect("load adapter catalog");
        let adapter = catalog.adapters.get("kimi").expect("kimi adapter").clone();

        let args = render_args(&GenericOptions {
            backend_id: "kimi".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: Some("sess-1".to_string()),
            model: "kimi-for-coding".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadWrite),
            timeout_secs: 5,
        })
        .unwrap();

        assert!(args.contains(&"--session".to_string()));
        assert!(args.contains(&"sess-1".to_string()));
        let prompt_idx = args.iter().position(|v| v == "--prompt").expect("prompt flag");
        let prompt_val = args.get(prompt_idx + 1).expect("prompt value");
        assert!(!prompt_val.contains("不允许写文件"));
    }

    #[test]
    fn cfgtest_render_claude_default_model_skips_model_flag() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let (_, adapter_path) = crate::test_utils::example_config_paths();
        let catalog = AdapterCatalog::load(&adapter_path).expect("load adapter catalog");
        let adapter = catalog.adapters.get("claude").expect("claude adapter").clone();

        let args = render_args(&GenericOptions {
            backend_id: "claude".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            model: "default".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
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

        let (_, adapter_path) = crate::test_utils::example_config_paths();
        let catalog = AdapterCatalog::load(&adapter_path).expect("load adapter catalog");
        let adapter = catalog.adapters.get("gemini").expect("gemini adapter").clone();

        let args = render_args(&GenericOptions {
            backend_id: "gemini".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            model: "default".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
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

        let (_, adapter_path) = crate::test_utils::example_config_paths();
        let catalog = AdapterCatalog::load(&adapter_path).expect("load adapter catalog");
        let adapter = catalog.adapters.get("kimi").expect("kimi adapter").clone();

        let args = render_args(&GenericOptions {
            backend_id: "kimi".to_string(),
            adapter,
            prompt: "ping".to_string(),
            workdir: repo.to_path_buf(),
            session_id: None,
            model: "default".to_string(),
            options: BTreeMap::new(),
            capabilities: base_capabilities(FilesystemCapability::ReadOnly),
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
        let (cfg_path, adapter_path) = crate::test_utils::example_config_paths();
        let args = render_args_for_brain(&cfg_path, &adapter_path, &repo, "gemini_reader");
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

        let (cfg_path, adapter_path) = crate::test_utils::example_config_paths();
        let prompt = format!("Read {}", outside_file.display());
        let args =
            render_args_for_brain_with_prompt(&cfg_path, &adapter_path, &repo, "gemini_reader", &prompt);
        assert!(args.contains(&"--include-directories".to_string()));
        assert!(args.contains(&outside.to_string_lossy().to_string()));
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
