use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use three::{
    backend,
    config::ConfigLoader,
    server::{VibeArgs, VibeServer},
    session_store::SessionStore,
    test_utils::example_config_paths,
};

fn resolve_test_command(backend_id: &str) -> String {
    match backend_id {
        "codex" => std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string()),
        _ => backend_id.to_string(),
    }
}

fn write_codex_config(path: &Path) {
    let cfg = r#"{
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
}"#;
    std::fs::write(path, cfg).unwrap();
}

fn render_args_for_role(
    cfg_path: &Path,
    adapter_path: &Path,
    repo: &Path,
    role: &str,
    prompt: &str,
) -> Vec<String> {
    let loader =
        ConfigLoader::new(Some(cfg_path.to_path_buf()));
    let cfg = loader.load_for_repo(repo).unwrap().unwrap();
    let rp = cfg.resolve_profile(Some(role)).unwrap();
    backend::render_args(&backend::GenericOptions {
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

fn print_rendered_command(cfg_path: &Path, adapter_path: &Path, repo: &Path, role: &str, prompt: &str) {
    let args = render_args_for_role(cfg_path, adapter_path, repo, role, prompt);
    let command = resolve_test_command("codex");
    eprintln!("cfgtest command for role '{role}':");
    eprintln!("  cmd: {command}");
    eprintln!("  args: {args:?}");
}

async fn run_role(
    cfg_path: &Path,
    adapter_path: &Path,
    repo: &Path,
    role: &str,
    prompt: String,
) -> three::server::VibeOutput {
    let store = SessionStore::new(repo.join("sessions.json"));
    let server = VibeServer::new(
        ConfigLoader::new(Some(cfg_path.to_path_buf())),
        store,
    );

    server
        .run_vibe_internal(
            None,
            VibeArgs {
                prompt,
                cd: repo.to_string_lossy().to_string(),
                role: Some(role.to_string()),
                backend: None,
                model: None,
                reasoning_effort: None,
                session_id: None,
                force_new_session: true,
                session_key: None,
                timeout_secs: Some(300),
                contract: None,
                validate_patch: false,
            },
        )
        .await
        .unwrap()
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{nanos}")
}

fn prompt_date() -> String {
    "Return exactly DATE:YYYY-MM-DD for today's date. No other text.".to_string()
}

fn prompt_create_file(path: &Path) -> String {
    format!(
        "Attempt to create a file at {path} with content 'hello'. Reply with exactly RESULT:true if the file is created, otherwise RESULT:false.",
        path = path.display()
    )
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_codex_smoke() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let cfg_path = td.path().join("config.json");
    write_codex_config(&cfg_path);
    let (_, adapter_path) = example_config_paths();

    let prompt = prompt_date();
    print_rendered_command(&cfg_path, &adapter_path, &repo, "reader", &prompt);
    let out = run_role(&cfg_path, &adapter_path, &repo, "reader", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    let re = Regex::new(r"DATE:\d{4}-\d{2}-\d{2}").unwrap();
    assert!(re.is_match(&out.agent_messages), "msg={}", out.agent_messages);
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_codex_readonly_create_file() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let filename = format!("readonly-{}.txt", unique_suffix());
    let target = repo.join(filename);
    if target.exists() {
        std::fs::remove_file(&target).unwrap();
    }

    let cfg_path = td.path().join("config.json");
    write_codex_config(&cfg_path);
    let (_, adapter_path) = example_config_paths();

    let prompt = prompt_create_file(&target);
    print_rendered_command(&cfg_path, &adapter_path, &repo, "reader", &prompt);
    let out = run_role(&cfg_path, &adapter_path, &repo, "reader", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    let reported_true = out.agent_messages.contains("RESULT:true");
    let reported_false = out.agent_messages.contains("RESULT:false");
    assert!(reported_true || reported_false, "msg={}", out.agent_messages);
    let exists = target.exists();
    assert_eq!(exists, reported_true, "msg={}", out.agent_messages);
    assert!(!exists, "read-only should not create files");
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_codex_readwrite_create_file() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let filename = format!("write-{}.txt", unique_suffix());
    let target = repo.join(filename);
    if target.exists() {
        std::fs::remove_file(&target).unwrap();
    }

    let cfg_path = td.path().join("config.json");
    write_codex_config(&cfg_path);
    let (_, adapter_path) = example_config_paths();

    let prompt = prompt_create_file(&target);
    print_rendered_command(&cfg_path, &adapter_path, &repo, "writer", &prompt);
    let out = run_role(&cfg_path, &adapter_path, &repo, "writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    assert!(out.agent_messages.contains("RESULT:true"), "msg={}", out.agent_messages);
    assert!(target.exists(), "expected file to be created");
    let content = std::fs::read_to_string(&target).unwrap_or_default();
    assert!(content.contains("hello"), "content={}", content);
}
