
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use mcp_server_three::{
    backend,
    config::ConfigLoader,
    server::{VibeArgs, VibeServer},
    session_store::SessionStore,
    test_utils::example_config_path,
};

fn example_loader() -> std::path::PathBuf {
    example_config_path()
}

fn resolve_test_command(backend_id: &str) -> String {
    match backend_id {
        "codex" => std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string()),
        "gemini" => std::env::var("GEMINI_BIN").unwrap_or_else(|_| "gemini".to_string()),
        _ => backend_id.to_string(),
    }
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
        resume: false,
        model: rp.profile.model.clone(),
        options: rp.profile.options.clone(),
        capabilities: rp.profile.capabilities.clone(),
        timeout_secs: 5,
    })
    .unwrap()
}

fn print_rendered_command(cfg_path: &Path, repo: &Path, role: &str, prompt: &str) {
    let args = render_args_for_role(cfg_path, repo, role, prompt);
    let command = resolve_test_command("gemini");
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

#[test]
fn cfgtest_render_gemini_include_directories_handles_extensionless_file() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let outside = td.path().join("README");
    std::fs::write(&outside, "x").unwrap();

    let cfg_path = example_loader();
    let prompt = format!("Read {}", outside.display());
    let args = render_args_for_role(&cfg_path, &repo, "gemini_reader", &prompt);

    let include_idx = args.iter().position(|v| v == "--include-directories").unwrap();
    let include_val = args.get(include_idx + 1).expect("include value");
    let includes: Vec<&str> = include_val.split(',').collect();
    assert!(includes.iter().any(|v| *v == td.path().to_string_lossy()));
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_gemini_3flash_smoke() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let cfg_path = example_loader();
    let prompt = prompt_date();
    print_rendered_command(&cfg_path, &repo, "gemini_writer", &prompt);
    let out = run_role(&cfg_path, &repo, "gemini_writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    let re = Regex::new(r"DATE:\d{4}-\d{2}-\d{2}").unwrap();
    assert!(re.is_match(&out.agent_messages), "msg={}", out.agent_messages);
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_gemini_3pro_smoke() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let cfg_path = example_loader();
    let prompt = prompt_date();
    print_rendered_command(&cfg_path, &repo, "gemini_reader", &prompt);
    let out = run_role(&cfg_path, &repo, "gemini_reader", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    let re = Regex::new(r"DATE:\d{4}-\d{2}-\d{2}").unwrap();
    assert!(re.is_match(&out.agent_messages), "msg={}", out.agent_messages);
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_gemini_readonly_create_file() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let filename = format!("readonly-{}.txt", unique_suffix());
    let target = repo.join(filename);
    if target.exists() {
        std::fs::remove_file(&target).unwrap();
    }

    let cfg_path = example_loader();
    let prompt = prompt_create_file(&target);
    print_rendered_command(&cfg_path, &repo, "gemini_reader", &prompt);
    let out = run_role(&cfg_path, &repo, "gemini_reader", prompt).await;

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
async fn cfgtest_real_gemini_readwrite_create_file() {
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
    print_rendered_command(&cfg_path, &repo, "gemini_writer", &prompt);
    let out = run_role(&cfg_path, &repo, "gemini_writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    assert!(out.agent_messages.contains("RESULT:true"), "msg={}", out.agent_messages);
    assert!(target.exists(), "expected file to be created");
    let content = std::fs::read_to_string(&target).unwrap_or_default();
    assert!(content.contains("hello"), "content={}", content);
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_gemini_include_directories_reads_external_file() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let external_dir = td.path().join("external");
    std::fs::create_dir_all(&external_dir).unwrap();
    let external_file = external_dir.join("hello.json");
    let content = format!("hello-{}", unique_suffix());
    std::fs::write(&external_file, &content).unwrap();

    let cfg_path = example_loader();
    let prompt = format!(
        "Read the file at {external} and reply with exactly CONTENT:{expected}.",
        external = external_file.display(),
        expected = content
    );
    let args = render_args_for_role(&cfg_path, &repo, "gemini_writer", &prompt);
    let include_idx = args.iter().position(|v| v == "--include-directories").unwrap();
    let include_val = args.get(include_idx + 1).expect("include value");
    let includes: Vec<&str> = include_val.split(',').collect();
    assert!(includes.iter().any(|v| *v == external_dir.to_string_lossy()));
    print_rendered_command(&cfg_path, &repo, "gemini_writer", &prompt);
    let out = run_role(&cfg_path, &repo, "gemini_writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    assert!(out.agent_messages.contains(&content), "msg={}", out.agent_messages);
}

#[tokio::test]
#[ignore]
async fn cfgtest_real_gemini_include_directories_reads_multiple_external_files() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let external_dir_a = td.path().join("external-a");
    let external_dir_b = td.path().join("external-b");
    std::fs::create_dir_all(&external_dir_a).unwrap();
    std::fs::create_dir_all(&external_dir_b).unwrap();

    let external_file_a = external_dir_a.join("alpha.json");
    let external_file_b = external_dir_b.join("beta.json");
    let content_a = format!("alpha-{}", unique_suffix());
    let content_b = format!("beta-{}", unique_suffix());
    std::fs::write(&external_file_a, &content_a).unwrap();
    std::fs::write(&external_file_b, &content_b).unwrap();

    let cfg_path = example_loader();
    let prompt = format!(
        "Read the files at {external_a} and {external_b} and reply with exactly CONTENT_A:{expected_a} CONTENT_B:{expected_b}.",
        external_a = external_file_a.display(),
        external_b = external_file_b.display(),
        expected_a = content_a,
        expected_b = content_b
    );

    let args = render_args_for_role(&cfg_path, &repo, "gemini_writer", &prompt);
    let include_idx = args.iter().position(|v| v == "--include-directories").unwrap();
    let include_val = args.get(include_idx + 1).expect("include value");
    let includes: Vec<&str> = include_val.split(',').collect();
    assert!(includes.iter().any(|v| *v == external_dir_a.to_string_lossy()));
    assert!(includes.iter().any(|v| *v == external_dir_b.to_string_lossy()));

    print_rendered_command(&cfg_path, &repo, "gemini_writer", &prompt);
    let out = run_role(&cfg_path, &repo, "gemini_writer", prompt).await;

    assert!(out.success, "error={:?}", out.error);
    assert!(out.agent_messages.contains(&content_a), "msg={}", out.agent_messages);
    assert!(out.agent_messages.contains(&content_b), "msg={}", out.agent_messages);
}
