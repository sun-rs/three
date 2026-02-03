use crate::config::{CodexApprovalPolicy, CodexSandboxPolicy, ReasoningEffort};
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone)]
pub struct CodexOptions {
    pub prompt: String,
    pub workdir: PathBuf,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub sandbox: CodexSandboxPolicy,
    pub ask_for_approval: Option<CodexApprovalPolicy>,
    pub dangerously_bypass_approvals_and_sandbox: bool,
    pub skip_git_repo_check: bool,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct CodexResult {
    pub session_id: String,
    pub agent_messages: String,
    pub warnings: Option<String>,
}

pub async fn run(opts: CodexOptions) -> Result<CodexResult> {
    let timeout_duration = Duration::from_secs(opts.timeout_secs);
    timeout(timeout_duration, run_internal(opts))
        .await
        .context("codex command timed out")?
}

async fn run_internal(opts: CodexOptions) -> Result<CodexResult> {
    let codex_bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string());

    #[cfg(windows)]
    let mut cmd = {
        let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
        let mut c = Command::new(comspec);
        c.args(["/D", "/S", "/C", &codex_bin]);
        c
    };

    #[cfg(not(windows))]
    let mut cmd = Command::new(codex_bin);

    // Global flags must appear before subcommand.
    if !opts.dangerously_bypass_approvals_and_sandbox {
        cmd.args(["-s", sandbox_str(opts.sandbox)]);
        if let Some(appr) = opts.ask_for_approval {
            cmd.args(["-a", approval_str(appr)]);
        }
    }

    cmd.arg("exec");
    if opts.dangerously_bypass_approvals_and_sandbox {
        cmd.arg("--dangerously-bypass-approvals-and-sandbox");
    }
    cmd.args(["--cd"]);
    cmd.arg(opts.workdir.as_os_str());
    cmd.arg("--json");

    if opts.skip_git_repo_check {
        cmd.arg("--skip-git-repo-check");
    }


    if let Some(model) = opts.model.as_deref() {
        if !model.trim().is_empty() {
            cmd.args(["--model", model]);
        }
    }

    if let Some(effort) = opts.reasoning_effort {
        // codex config expects TOML string values
        let val = format!("model_reasoning_effort=\"{}\"", effort.as_codex_config_value());
        cmd.args(["-c", &val]);
    }

    if let Some(session_id) = opts.session_id.as_deref() {
        if !session_id.trim().is_empty() {
            cmd.args(["resume", session_id]);
        }
    }

    cmd.args(["--", &opts.prompt]);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().context("failed to spawn codex")?;
    let stdout = child.stdout.take().context("missing codex stdout")?;
    let stderr = child.stderr.take().context("missing codex stderr")?;

    // Capture stderr in background (treated as warnings on success)
    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = String::new();
        let mut out = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf).await {
                Ok(0) => break,
                Ok(_) => {
                    // Avoid unbounded growth; codex stderr can be chatty.
                    if out.len() < 1024 * 1024 {
                        out.push_str(&buf);
                    }
                }
                Err(_) => break,
            }
        }
        out
    });

    let mut result = CodexResult {
        session_id: String::new(),
        agent_messages: String::new(),
        warnings: None,
    };

    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(trimmed)
            .with_context(|| format!("failed to parse codex json line: {}", preview(trimmed)))?;

        if let Some(thread_id) = v.get("thread_id").and_then(|x| x.as_str()) {
            if !thread_id.is_empty() {
                result.session_id = thread_id.to_string();
            }
        }

        if let Some(item) = v.get("item").and_then(|x| x.as_object()) {
            if item.get("type").and_then(|x| x.as_str()) == Some("agent_message") {
                if let Some(text) = item.get("text").and_then(|x| x.as_str()) {
                    if !result.agent_messages.is_empty() && !text.is_empty() {
                        result.agent_messages.push('\n');
                    }
                    result.agent_messages.push_str(text);
                }
            }
        }

        if let Some(line_type) = v.get("type").and_then(|x| x.as_str()) {
            let lower = line_type.to_ascii_lowercase();
            if lower.contains("fail") || lower.contains("error") {
                if let Some(msg) = v.get("message").and_then(|x| x.as_str()) {
                    anyhow::bail!("codex error: {msg}");
                }
                anyhow::bail!("codex error event: {line_type}");
            }
        }
    }

    let status = child.wait().await?;
    let stderr_out = stderr_handle.await.unwrap_or_default();

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("codex exited with status {code}. stderr: {stderr_out}");
    }
    if !stderr_out.trim().is_empty() {
        result.warnings = Some(stderr_out);
    }

    if result.session_id.is_empty() {
        anyhow::bail!("failed to get session_id from codex output");
    }

    Ok(result)
}

fn sandbox_str(p: CodexSandboxPolicy) -> &'static str {
    match p {
        CodexSandboxPolicy::ReadOnly => "read-only",
        CodexSandboxPolicy::WorkspaceWrite => "workspace-write",
        CodexSandboxPolicy::DangerFullAccess => "danger-full-access",
    }
}

fn approval_str(p: CodexApprovalPolicy) -> &'static str {
    match p {
        CodexApprovalPolicy::Untrusted => "untrusted",
        CodexApprovalPolicy::OnFailure => "on-failure",
        CodexApprovalPolicy::OnRequest => "on-request",
        CodexApprovalPolicy::Never => "never",
    }
}

fn preview(s: &str) -> String {
    const MAX: usize = 200;
    if s.len() <= MAX {
        return s.to_string();
    }
    format!("{}...", &s[..MAX])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn codex_passes_model_effort_cd_and_resume() {
        let td = tempfile::tempdir().unwrap();
        let bin = td.path().join("fake-codex.sh");
        let log = td.path().join("log.txt");

        let script = format!(
            "#!/bin/sh\nset -e\n( pwd; printf '\\nARGS:'; printf ' %s' \"$@\"; printf '\\n' ) > \"{}\"\n\n# Emit minimal codex --json stream\necho '{{\"type\":\"thread.started\",\"thread_id\":\"sess-123\"}}'\necho '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"hello\"}}}}'\n\nexit 0\n",
            log.display()
        );

        {
            let mut f = std::fs::File::create(&bin).unwrap();
            f.write_all(script.as_bytes()).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin, perms).unwrap();
        }

        let _env = crate::test_support::scoped_codex_bin(bin.to_string_lossy().as_ref());

        let workdir = td.path().join("repo");
        std::fs::create_dir_all(&workdir).unwrap();

        let res = run(CodexOptions {
            prompt: "do the thing".to_string(),
            workdir: workdir.clone(),
            session_id: Some("sess-prev".to_string()),
            model: Some("gpt-5.2-codex".to_string()),
            reasoning_effort: Some(ReasoningEffort::Xhigh),
            sandbox: CodexSandboxPolicy::WorkspaceWrite,
            ask_for_approval: Some(CodexApprovalPolicy::Never),
            dangerously_bypass_approvals_and_sandbox: false,
            skip_git_repo_check: true,
            timeout_secs: 5,
        })
        .await
        .unwrap();

        assert_eq!(res.session_id, "sess-123");
        assert!(res.agent_messages.contains("hello"));

        let log_txt = std::fs::read_to_string(&log).unwrap();
        assert!(log_txt.contains("--cd"));
        assert!(log_txt.contains(workdir.to_string_lossy().as_ref()));
        assert!(log_txt.contains("-s workspace-write"));
        assert!(log_txt.contains("-a never"));
        assert!(log_txt.contains("--skip-git-repo-check"));
        assert!(log_txt.contains("--model gpt-5.2-codex"));
        assert!(log_txt.contains("model_reasoning_effort=\"xhigh\""));
        assert!(log_txt.contains("resume sess-prev"));
    }
}
