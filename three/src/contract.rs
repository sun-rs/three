use anyhow::{Context, Result};
use std::path::Path;
use std::process::Stdio;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchFormat {
    UnifiedDiff,
    SearchReplace,
    Unknown,
    None,
}

#[derive(Debug, Clone)]
pub struct ContractCheck {
    pub has_patch: bool,
    pub has_citations: bool,
    pub patch_format: PatchFormat,
    pub extracted_patch: Option<String>,
    pub apply_check: Option<ApplyCheck>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ApplyCheck {
    pub ok: bool,
    pub output: String,
}

pub fn check_patch_with_citations(text: &str) -> ContractCheck {
    let citations = has_citations(text);
    let patch = detect_patch_format(text);
    let (has_patch, extracted) = extract_patch(text, patch);

    let mut errors = Vec::new();
    if !has_patch {
        errors.push("missing PATCH".to_string());
    }
    if !citations {
        errors.push("missing CITATIONS".to_string());
    }

    ContractCheck {
        has_patch,
        has_citations: citations,
        patch_format: patch,
        extracted_patch: extracted,
        apply_check: None,
        errors,
    }
}

pub fn validate_git_apply_check(repo_root: &Path, patch: &str) -> Result<ApplyCheck> {
    // Validate we are inside a git repo.
    let mut rev = std::process::Command::new("git");
    rev.arg("rev-parse");
    rev.arg("--is-inside-work-tree");
    rev.current_dir(repo_root);
    let rev_out = rev.output().context("failed to run git rev-parse")?;
    if !rev_out.status.success() {
        return Ok(ApplyCheck {
            ok: false,
            output: "not a git repository (git rev-parse failed)".to_string(),
        });
    }

    let mut cmd = std::process::Command::new("git");
    cmd.arg("apply");
    cmd.arg("--check");
    cmd.arg("--whitespace=nowarn");
    cmd.arg("-");
    cmd.current_dir(repo_root);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().context("failed to spawn git apply --check")?;
    {
        let stdin = child.stdin.as_mut().context("missing git stdin")?;
        use std::io::Write;
        stdin.write_all(patch.as_bytes())?;
        // `git apply` can report "corrupt patch" if the input doesn't end with a newline.
        if !patch.ends_with('\n') {
            stdin.write_all(b"\n")?;
        }
    }

    let out = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{}{}", stdout, stderr).trim().to_string();
    Ok(ApplyCheck {
        ok: out.status.success(),
        output: combined,
    })
}

fn has_citations(text: &str) -> bool {
    // Intentionally conservative: allow a few common citation conventions.
    // - Markdown: "> Source: path:line" or "CITATIONS:" section
    // - Inline: "[cite:path:line]"
    let lower = text.to_ascii_lowercase();
    if lower.contains("citations:") {
        return true;
    }
    if lower.contains("> source:") {
        return true;
    }
    if lower.contains("[cite:") {
        return true;
    }
    false
}

fn detect_patch_format(text: &str) -> PatchFormat {
    if text.contains("diff --git ") || (text.contains("--- a/") && text.contains("+++ b/")) {
        return PatchFormat::UnifiedDiff;
    }
    if text.contains("<<<<<<< SEARCH") && text.contains(">>>>>>> REPLACE") {
        return PatchFormat::SearchReplace;
    }
    if text.trim().is_empty() {
        return PatchFormat::None;
    }
    PatchFormat::Unknown
}

fn extract_patch(text: &str, format: PatchFormat) -> (bool, Option<String>) {
    match format {
        PatchFormat::UnifiedDiff => {
            // Prefer fenced code block ```diff ...```
            if let Some(p) = extract_fenced(text, "diff") {
                let has =
                    p.contains("diff --git ") || (p.contains("--- a/") && p.contains("+++ b/"));
                return (has, Some(p));
            }
            // Fallback: try from first diff marker to end.
            if let Some(idx) = text.find("diff --git ") {
                return (true, Some(text[idx..].trim().to_string()));
            }
            if let Some(idx) = text.find("--- a/") {
                return (true, Some(text[idx..].trim().to_string()));
            }
            (false, None)
        }
        PatchFormat::SearchReplace => {
            // We don't extract; it's not git-applicable.
            (true, None)
        }
        PatchFormat::Unknown => (false, None),
        PatchFormat::None => (false, None),
    }
}

fn extract_fenced(text: &str, info: &str) -> Option<String> {
    let start = format!("```{}", info);
    let mut rest = text;
    while let Some(i) = rest.find(&start) {
        let after = &rest[i + start.len()..];
        // consume the rest of the start line
        let after = after
            .strip_prefix('\n')
            .or_else(|| after.strip_prefix("\r\n"))
            .unwrap_or(after);
        if let Some(end) = after.find("```") {
            let block = after[..end].trim().to_string();
            if !block.is_empty() {
                return Some(block);
            }
        }
        rest = &after;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn detects_citations() {
        assert!(check_patch_with_citations("CITATIONS:\n- a.rs:1").has_citations);
        assert!(check_patch_with_citations("> Source: a.rs:1").has_citations);
        assert!(check_patch_with_citations("[cite:a.rs:1]").has_citations);
        assert!(!check_patch_with_citations("no refs").has_citations);
    }

    #[test]
    fn extracts_unified_diff_from_fence() {
        let s =
            "PATCH\n```diff\ndiff --git a/a b/a\n--- a/a\n+++ b/a\n@@\n-1\n+2\n```\nCITATIONS: a:1";
        let c = check_patch_with_citations(s);
        assert_eq!(c.patch_format, PatchFormat::UnifiedDiff);
        assert!(c.extracted_patch.unwrap().contains("diff --git"));
    }

    #[test]
    fn git_apply_check_accepts_valid_patch() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path();

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(repo)
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

        // Create baseline commit.
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

        // Make a change and capture patch.
        std::fs::write(repo.join("hello.txt"), "hello\n").unwrap();
        let patch_out = run(&["diff"]);
        let patch = String::from_utf8_lossy(&patch_out.stdout).to_string();
        assert!(patch.contains("diff --git"));

        // Restore to baseline so apply-check should succeed.
        run(&["checkout", "--", "hello.txt"]);

        let res = validate_git_apply_check(repo, &patch).unwrap();
        assert!(res.ok, "apply-check failed: {}", res.output);
    }
}
