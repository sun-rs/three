use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use tokio::sync::Mutex;
use mcp_server_three::{
    backend,
    config::ConfigLoader,
    server::{VibeArgs, VibeServer},
    session_store::SessionStore,
};

fn kimi_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn resolve_test_command() -> String {
    std::env::var("KIMI_BIN").unwrap_or_else(|_| "kimi".to_string())
}

fn write_kimi_config(path: &Path) {
    let cfg = r#"{
  "backend": {
    "kimi": {
      "models": {}
    }
  },
  "roles": {
    "reader": {
      "model": "kimi/default",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "deny", "network": "deny", "tools": ["read"] }
    },
    "writer": {
      "model": "kimi/default",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    }
  }
}"#;
    std::fs::write(path, cfg).unwrap();
}

fn render_args_for_role(
    cfg_path: &Path,
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

fn print_rendered_command(cfg_path: &Path, repo: &Path, role: &str, prompt: &str) {
    let args = render_args_for_role(cfg_path, repo, role, prompt);
    let command = resolve_test_command();
    eprintln!("cfgtest command for role '{role}':");
    eprintln!("  cmd: {command}");
    eprintln!("  args: {args:?}");
}

async fn run_role(
    cfg_path: &Path,
    repo: &Path,
    role: &str,
    prompt: String,
) -> mcp_server_three::server::VibeOutput {
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
async fn cfgtest_real_kimi_smoke() {
    let _lock = kimi_test_lock().lock().await;
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let cfg_path = td.path().join("config.json");
    write_kimi_config(&cfg_path);

    let prompt = prompt_date();
    print_rendered_command(&cfg_path, &repo, "reader", &prompt);
    let out = run_role(&cfg_path, &repo, "reader", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    let re = Regex::new(r"DATE:\d{4}-\d{2}-\d{2}").unwrap();
    assert!(re.is_match(&out.agent_messages), "msg={}", out.agent_messages);
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_kimi_reader_create_file() {
    let _lock = kimi_test_lock().lock().await;
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let filename = format!("readonly-{}.txt", unique_suffix());
    let target = repo.join(filename);
    if target.exists() {
        std::fs::remove_file(&target).unwrap();
    }

    let cfg_path = td.path().join("config.json");
    write_kimi_config(&cfg_path);

    let prompt = prompt_create_file(&target);
    print_rendered_command(&cfg_path, &repo, "reader", &prompt);
    let out = run_role(&cfg_path, &repo, "reader", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    assert!(out.agent_messages.contains("RESULT:true"), "msg={}", out.agent_messages);
    assert!(target.exists(), "expected file to be created");
    let content = std::fs::read_to_string(&target).unwrap_or_default();
    assert!(content.contains("hello"), "content={}", content);
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_kimi_readwrite_create_file() {
    let _lock = kimi_test_lock().lock().await;
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let filename = format!("write-{}.txt", unique_suffix());
    let target = repo.join(filename);
    if target.exists() {
        std::fs::remove_file(&target).unwrap();
    }

    let cfg_path = td.path().join("config.json");
    write_kimi_config(&cfg_path);

    let prompt = prompt_create_file(&target);
    print_rendered_command(&cfg_path, &repo, "writer", &prompt);
    let out = run_role(&cfg_path, &repo, "writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    assert!(out.agent_messages.contains("RESULT:true"), "msg={}", out.agent_messages);
    assert!(target.exists(), "expected file to be created");
    let content = std::fs::read_to_string(&target).unwrap_or_default();
    assert!(content.contains("hello"), "content={}", content);
}
