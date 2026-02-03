use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone)]
pub struct GeminiOptions {
    pub prompt: String,
    pub workdir: PathBuf,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct GeminiResult {
    pub session_id: String,
    pub agent_messages: String,
    pub warnings: Option<String>,
}

pub async fn run(opts: GeminiOptions) -> Result<GeminiResult> {
    let timeout_duration = Duration::from_secs(opts.timeout_secs);
    timeout(timeout_duration, run_internal(opts))
        .await
        .context("gemini command timed out")?
}

async fn run_internal(opts: GeminiOptions) -> Result<GeminiResult> {
    let gemini_bin = std::env::var("GEMINI_BIN").unwrap_or_else(|_| {
        if cfg!(windows) {
            "gemini.cmd".to_string()
        } else {
            "gemini".to_string()
        }
    });

    #[cfg(windows)]
    let mut cmd = {
        let lower = gemini_bin.to_ascii_lowercase();
        let needs_cmd = lower.ends_with(".cmd") || lower.ends_with(".bat");
        if needs_cmd {
            let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
            let mut c = Command::new(comspec);
            c.arg("/d");
            c.arg("/s");
            c.arg("/c");
            c.arg(&gemini_bin);
            c
        } else {
            Command::new(&gemini_bin)
        }
    };
    #[cfg(not(windows))]
    let mut cmd = Command::new(&gemini_bin);

    cmd.current_dir(&opts.workdir);
    cmd.arg("--prompt");
    cmd.arg(&opts.prompt);
    cmd.arg("-o");
    cmd.arg("stream-json");

    // Ensure workspace includes the repo root even if CLI uses a different project root heuristic.
    cmd.arg("--include-directories");
    cmd.arg(opts.workdir.to_string_lossy().as_ref());

    if let Some(model) = opts.model.as_deref() {
        if !model.trim().is_empty() {
            cmd.args(["--model", model]);
        }
    }
    if let Some(session_id) = opts.session_id.as_deref() {
        if !session_id.trim().is_empty() {
            cmd.args(["--resume", session_id]);
        }
    }

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().context("failed to spawn gemini")?;
    let stdout = child.stdout.take().context("missing gemini stdout")?;
    let stderr = child.stderr.take().context("missing gemini stderr")?;

    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = String::new();
        let mut out = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf).await {
                Ok(0) => break,
                Ok(_) => {
                    if out.len() < 100_000 {
                        out.push_str(&buf);
                    }
                }
                Err(_) => break,
            }
        }
        out
    });

    let mut result = GeminiResult {
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
            .with_context(|| format!("failed to parse gemini json line: {}", preview(trimmed)))?;

        if let Some(session_id) = v.get("session_id").and_then(|x| x.as_str()) {
            if !session_id.is_empty() {
                result.session_id = session_id.to_string();
            }
        }

        // Stream-json emits message objects with {type:"message", role:"assistant", content:"..."}
        let item_type = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("");
        if item_type == "message" && role == "assistant" {
            if let Some(content) = v.get("content").and_then(|x| x.as_str()) {
                if !result.agent_messages.is_empty() && !content.is_empty() {
                    result.agent_messages.push('\n');
                }
                result.agent_messages.push_str(content);
            }
        }

        // Error surfaces
        if let Some(t) = v.get("type").and_then(|x| x.as_str()) {
            let lower = t.to_ascii_lowercase();
            if lower.contains("fail") || lower.contains("error") {
                if let Some(msg) = v.get("message").and_then(|x| x.as_str()) {
                    anyhow::bail!("gemini error: {msg}");
                }
                anyhow::bail!("gemini error event: {t}");
            }
        }
        if v.get("error").is_some() {
            anyhow::bail!("gemini returned error object: {}", preview(trimmed));
        }
    }

    let status = child.wait().await?;
    let stderr_out = stderr_handle.await.unwrap_or_default();
    if !status.success() {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("gemini exited with status {code}. stderr: {stderr_out}");
    }
    if !stderr_out.trim().is_empty() {
        result.warnings = Some(stderr_out);
    }

    if result.session_id.is_empty() {
        anyhow::bail!("failed to get session_id from gemini output");
    }

    Ok(result)
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
    async fn gemini_uses_workdir_model_and_resume() {
        let td = tempfile::tempdir().unwrap();
        let bin = td.path().join("fake-gemini.sh");
        let log = td.path().join("log.txt");
        let workdir = td.path().join("repo");
        std::fs::create_dir_all(&workdir).unwrap();

        let script = format!(
            "#!/bin/sh\nset -e\n( pwd; printf '\\nARGS:'; printf ' %s' \"$@\"; printf '\\n' ) > \"{}\"\n\n# Emit minimal stream-json\necho '{{\"type\":\"init\",\"session_id\":\"g-sess-1\"}}'\necho '{{\"type\":\"message\",\"role\":\"assistant\",\"content\":\"hi\"}}'\n\nexit 0\n",
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

        let _env = crate::test_support::scoped_gemini_bin(bin.to_string_lossy().as_ref());

        let res = run(GeminiOptions {
            prompt: "read files".to_string(),
            workdir: workdir.clone(),
            session_id: Some("prev".to_string()),
            model: Some("gemini-pro".to_string()),
            timeout_secs: 5,
        })
        .await
        .unwrap();

        assert_eq!(res.session_id, "g-sess-1");
        assert!(res.agent_messages.contains("hi"));

        let log_txt = std::fs::read_to_string(&log).unwrap();
        // first line is pwd
        assert!(log_txt.lines().next().unwrap().contains(workdir.to_string_lossy().as_ref()));
        assert!(log_txt.contains("--model gemini-pro"));
        assert!(log_txt.contains("--resume prev"));
        assert!(log_txt.contains("--include-directories"));
    }
}
