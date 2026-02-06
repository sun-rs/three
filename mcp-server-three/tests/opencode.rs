use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use mcp_server_three::{
    backend,
    config::ConfigLoader,
    server::{VibeArgs, VibeServer},
    session_store::SessionStore,
    test_utils::example_config_path,
};
use regex::Regex;
use tokio::sync::Mutex;

fn opencode_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn example_loader() -> std::path::PathBuf {
    example_config_path()
}

fn render_args_for_role(cfg_path: &Path, repo: &Path, role: &str, prompt: &str) -> Vec<String> {
    let loader = ConfigLoader::new(Some(cfg_path.to_path_buf()));
    let cfg = loader.load_for_repo(repo).unwrap().unwrap();
    let rp = cfg.resolve_profile(Some(role)).unwrap();
    backend::render_args(&backend::GenericOptions {
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

fn print_rendered_command(cfg_path: &Path, repo: &Path, role: &str, prompt: &str) {
    let args = render_args_for_role(cfg_path, repo, role, prompt);
    let command = "opencode";
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
    let server = VibeServer::new(ConfigLoader::new(Some(cfg_path.to_path_buf())), store);

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
                client: None,

                conversation_id: None,
            },
        )
        .await
        .unwrap()
}

fn prompt_date() -> String {
    "Return exactly DATE:YYYY-MM-DD for today's date. No other text.".to_string()
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{nanos}")
}

fn prompt_create_file(path: &Path) -> String {
    format!(
        "Attempt to create a file at {path} with content 'hello'. Reply with exactly RESULT:true if the file is created, otherwise RESULT:false.",
        path = path.display()
    )
}

fn write_opencode_config(path: &Path) {
    let cfg = r#"{
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
    }
  }
}"#;
    std::fs::write(path, cfg).unwrap();
}

fn write_opencode_writer_config(path: &Path) {
    let cfg = r#"{
  "backend": {
    "opencode": {
      "models": { "opencode-gpt-5": {} }
    }
  },
  "roles": {
    "writer": {
      "model": "opencode/opencode-gpt-5",
      "personas": { "description": "d", "prompt": "p" },
      "capabilities": { "filesystem": "read-write", "shell": "allow", "network": "allow", "tools": ["*"] }
    }
  }
}"#;
    std::fs::write(path, cfg).unwrap();
}

struct ScopedPath {
    prev: Option<String>,
}

impl ScopedPath {
    fn new(bin_dir: &Path) -> Self {
        let prev = env::var("PATH").ok();
        let mut new_path = bin_dir.to_string_lossy().to_string();
        if let Some(prev_val) = prev.as_deref() {
            if !prev_val.is_empty() {
                new_path.push(':');
                new_path.push_str(prev_val);
            }
        }
        env::set_var("PATH", new_path);
        Self { prev }
    }
}

impl Drop for ScopedPath {
    fn drop(&mut self) {
        match self.prev.as_deref() {
            Some(val) => env::set_var("PATH", val),
            None => env::remove_var("PATH"),
        }
    }
}

fn write_fake_opencode(bin_dir: &Path, log: &Path) -> PathBuf {
    let bin_path = bin_dir.join("opencode");
    let script = format!(
        "#!/bin/sh\nset -e\n\necho \"ARGS: $@\" >> \"{}\"\n\nprintf '%s\\n' '{{\"type\":\"text\",\"part\":{{\"sessionID\":\"sess-1\",\"text\":\"first\"}}}}'\nprintf '%s\\n' '{{\"type\":\"tool_use\",\"part\":{{\"sessionID\":\"sess-1\"}}}}'\nprintf '%s\\n' '{{\"type\":\"text\",\"part\":{{\"text\":\"final\"}}}}'\n",
        log.display()
    );
    std::fs::write(&bin_path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bin_path, perms).unwrap();
    }
    bin_path
}

#[tokio::test]
async fn e2e_role_capability_rejected() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let cfg_path = td.path().join("config.json");
    write_opencode_config(&cfg_path);

    let store = SessionStore::new(repo.join("sessions.json"));
    let server = VibeServer::new(ConfigLoader::new(Some(cfg_path)), store);
    let err = server
        .run_vibe_internal(
            None,
            VibeArgs {
                prompt: "ping".to_string(),
                cd: repo.to_string_lossy().to_string(),
                role: Some("reader".to_string()),
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
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("filesystem capability") && msg.contains("opencode"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn e2e_opencode_picks_last_text_event() {
    let _lock = opencode_test_lock().lock().await;
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let cfg_path = td.path().join("config.json");
    write_opencode_writer_config(&cfg_path);

    let bin_dir = td.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let log = td.path().join("opencode.log");
    write_fake_opencode(&bin_dir, &log);
    let _path_guard = ScopedPath::new(&bin_dir);

    let store = SessionStore::new(repo.join("sessions.json"));
    let server = VibeServer::new(ConfigLoader::new(Some(cfg_path)), store);
    let out = server
        .run_vibe_internal(
            None,
            VibeArgs {
                prompt: "ping".to_string(),
                cd: repo.to_string_lossy().to_string(),
                role: Some("writer".to_string()),
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

    assert_eq!(out.agent_messages, "final");
    assert_eq!(out.backend_session_id, "sess-1");
    let log_txt = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(log_txt.contains("--format json"), "log={log_txt}");
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_opencode_smoke() {
    let _lock = opencode_test_lock().lock().await;
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let cfg_path = example_loader();
    let prompt = prompt_date();
    print_rendered_command(&cfg_path, &repo, "opencode_writer", &prompt);
    let out = run_role(&cfg_path, &repo, "opencode_writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    let re = Regex::new(r"DATE:\d{4}-\d{2}-\d{2}").unwrap();
    assert!(
        re.is_match(&out.agent_messages),
        "msg={}",
        out.agent_messages
    );
    assert_ne!(
        out.backend_session_id.trim(),
        "stateless",
        "session={}",
        out.backend_session_id
    );
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_opencode_readwrite_create_file() {
    let _lock = opencode_test_lock().lock().await;
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let filename = format!("write-{}.txt", unique_suffix());
    let target = repo.join(filename);
    if target.exists() {
        std::fs::remove_file(&target).unwrap();
    }

    let cfg_path = example_loader();
    let prompt = prompt_create_file(&target);
    print_rendered_command(&cfg_path, &repo, "opencode_writer", &prompt);
    let out = run_role(&cfg_path, &repo, "opencode_writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    assert!(
        out.agent_messages.contains("RESULT:true"),
        "msg={}",
        out.agent_messages
    );
    assert!(target.exists(), "expected file to be created");
    let content = std::fs::read_to_string(&target).unwrap_or_default();
    assert!(content.contains("hello"), "content={}", content);
}
