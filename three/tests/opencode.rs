use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use tokio::sync::Mutex;
use three::{
    backend,
    config::ConfigLoader,
    server::{VibeArgs, VibeServer},
    session_store::SessionStore,
    test_utils::example_config_paths,
};

fn opencode_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn example_loader() -> (std::path::PathBuf, std::path::PathBuf) {
    example_config_paths()
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
    let command = "opencode";
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

#[tokio::test]
#[ignore]
async fn cfgtest_real_opencode_smoke() {
    let _lock = opencode_test_lock().lock().await;
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let (cfg_path, adapter_path) = example_loader();
    let prompt = prompt_date();
    print_rendered_command(&cfg_path, &adapter_path, &repo, "opencode_writer", &prompt);
    let out = run_role(&cfg_path, &adapter_path, &repo, "opencode_writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    let re = Regex::new(r"DATE:\d{4}-\d{2}-\d{2}").unwrap();
    assert!(re.is_match(&out.agent_messages), "msg={}", out.agent_messages);
    assert_ne!(out.backend_session_id.trim(), "stateless", "session={}", out.backend_session_id);
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

    let (cfg_path, adapter_path) = example_loader();
    let prompt = prompt_create_file(&target);
    print_rendered_command(&cfg_path, &adapter_path, &repo, "opencode_writer", &prompt);
    let out = run_role(&cfg_path, &adapter_path, &repo, "opencode_writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    assert!(out.agent_messages.contains("RESULT:true"), "msg={}", out.agent_messages);
    assert!(target.exists(), "expected file to be created");
    let content = std::fs::read_to_string(&target).unwrap_or_default();
    assert!(content.contains("hello"), "content={}", content);
}
