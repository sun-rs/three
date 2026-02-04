# Role-only config + embedded adapter catalog Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Embed the adapter catalog in Rust, remove external adapter.json, and replace `brains` with `roles` everywhere.

**Architecture:** The server owns adapter capabilities (embedded catalog). `config.json` only exposes `roles` and model/persona/capabilities. Validation is per role; unsupported capabilities fail the role before execution.

**Tech Stack:** Rust (three crate), serde, minijinja, tokio, cargo tests, docs updates.

---

### Task 1: Add embedded adapter catalog and remove external adapter loading

**Files:**
- Modify: `three/src/config.rs`
- Create: `three/src/adapter_catalog.rs` (or similar)
- Modify: `three/src/lib.rs` (module export)
- Modify: `three/src/test_utils.rs`

**Step 1: Write the failing test**

Add a unit test that expects the default config load to have adapters without an external adapter path.

```rust
#[test]
fn loads_embedded_adapter_catalog() {
    let (cfg_path, _adapter_path) = crate::test_utils::example_config_paths();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let loader = ConfigLoader::new(Some(cfg_path));
    let cfg = loader.load_for_repo(&repo).unwrap().unwrap();
    let codex = cfg.backend.get("codex").unwrap();
    assert!(codex.adapter.is_some());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p three config::tests::loads_embedded_adapter_catalog`
Expected: FAIL (adapter is None).

**Step 3: Write minimal implementation**

- Add a new module (e.g., `three/src/adapter_catalog.rs`) with a function that returns the embedded catalog.
- In `ConfigLoader::load_for_repo`, replace `load_adapter_catalog(...)` with `embedded_adapter_catalog()`.
- Remove `ConfigLoader.adapter_path` and any file-based adapter loading.

Example skeleton:

```rust
// three/src/adapter_catalog.rs
use crate::config::{AdapterCatalog, AdapterConfig, OutputParserConfig, OutputPick, FilesystemCapability};
use std::collections::BTreeMap;

pub fn embedded_adapter_catalog() -> AdapterCatalog {
    let mut adapters = BTreeMap::new();
    // fill adapters.insert("codex".to_string(), AdapterConfig { ... });
    AdapterCatalog { adapters }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p three config::tests::loads_embedded_adapter_catalog`
Expected: PASS.

**Step 5: Commit**

```bash
git add three/src/adapter_catalog.rs three/src/lib.rs three/src/config.rs three/src/test_utils.rs
git commit -m "feat: embed adapter catalog"
```

---

### Task 2: Replace brains with roles in config parsing and validation

**Files:**
- Modify: `three/src/config.rs`
- Modify: `examples/config.json`

**Step 1: Write the failing test**

Update or add a test to expect `roles` instead of `brains`:

```rust
#[test]
fn rejects_missing_roles_key() {
    let raw = r#"{"backend": {}}"#;
    let err = VibeConfig::from_json_str(raw).unwrap_err();
    assert!(err.to_string().contains("missing 'roles'"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p three config::tests::rejects_missing_roles_key`
Expected: FAIL (still expects brains).

**Step 3: Write minimal implementation**

- Rename `brains` -> `roles` in `VibeConfig` and related structs.
- Replace `BrainConfig` / `BrainProfile` / `brain_id` with `RoleConfig` / `RoleProfile` / `role_id`.
- Rename `parse_brain_model_ref` to `parse_role_model_ref` and update error messages to say `role`.
- Update `examples/config.json` to use `roles` key.

Example struct rename:

```rust
pub struct VibeConfig {
    pub backend: BTreeMap<String, BackendConfig>,
    pub roles: BTreeMap<String, RoleConfig>,
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p three config::tests::rejects_missing_roles_key`
Expected: PASS.

**Step 5: Commit**

```bash
git add three/src/config.rs examples/config.json
git commit -m "feat: rename brains to roles"
```

---

### Task 3: Remove brain parameters from server API and session store

**Files:**
- Modify: `three/src/server.rs`
- Modify: `three/src/session_store.rs`
- Modify: `plugins/claude-code/three/commands/*.md`
- Modify: `plugins/claude-code/three/skills/three-routing/SKILL.md`

**Step 1: Write the failing test**

Update a server test to only use role and remove brain fields from inputs.

```rust
let args = VibeArgs {
    prompt: "ping".to_string(),
    cd: repo.display().to_string(),
    role: Some("oracle".to_string()),
    // brain removed
    ..Default::default()
};
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p three server::tests::...`
Expected: FAIL due to struct mismatch.

**Step 3: Write minimal implementation**

- Remove `brain` from `VibeArgs`, `RoundtableParticipant`, `RoundtableModerator`.
- Update info output to list `roles` instead of `brains`.
- Update `SessionStore` to compute key from `role` and `role_id` only (rename fields).
- Update any error messages to mention roles.
- Update plugin docs to use role terminology.

**Step 4: Run test to verify it passes**

Run: `cargo test -p three server::tests::...`
Expected: PASS.

**Step 5: Commit**

```bash
git add three/src/server.rs three/src/session_store.rs plugins/claude-code/three/commands plugins/claude-code/three/skills/three-routing/SKILL.md
git commit -m "feat: role-only server API"
```

---

### Task 4: Update backend rendering tests and integration tests to roles + embedded adapter

**Files:**
- Modify: `three/src/backend.rs`
- Modify: `three/src/test_utils.rs`
- Modify: `three/tests/*.rs`

**Step 1: Write the failing test**

Update test helpers to use `role` and no adapter path. For example:

```rust
fn render_args_for_role(cfg_path: &Path, repo: &Path, role: &str) -> Vec<String> {
    let loader = ConfigLoader::new(Some(cfg_path.to_path_buf()));
    let cfg = loader.load_for_repo(repo).unwrap().unwrap();
    let rp = cfg.resolve_profile(Some(role)).unwrap();
    // ...
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p three backend::tests::cfgtest_render_gemini_reader_args_match_example`
Expected: FAIL (old signatures and adapter path).

**Step 3: Write minimal implementation**

- Replace all `render_args_for_brain` helpers with `render_args_for_role`.
- Remove `adapter_path` usage in tests.
- Update integration tests to call `run_role(...)` and pass `role` only.

**Step 4: Run test to verify it passes**

Run: `cargo test -p three backend::tests::cfgtest_render_gemini_reader_args_match_example`
Expected: PASS.

**Step 5: Commit**

```bash
git add three/src/backend.rs three/src/test_utils.rs three/tests
git commit -m "test: update role-based render and e2e"
```

---

### Task 5: Remove adapter.json and update documentation

**Files:**
- Delete: `examples/adapter.json`
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Modify: `docs/config-schema.md`
- Modify: `docs/cli-*.md`

**Step 1: Write the failing test**

Add a simple doc test or schema check if present (otherwise skip and rely on cargo test).

**Step 2: Run test to verify it fails**

Run: `cargo test -p three`
Expected: FAIL (references to adapter.json / brains in docs or tests).

**Step 3: Write minimal implementation**

- Remove examples/adapter.json and update docs to state adapter is embedded.
- Replace “brains” with “roles” throughout docs.
- Add migration snippet: rename `brains` -> `roles`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p three`
Expected: PASS.

**Step 5: Commit**

```bash
git add README.md README.zh-CN.md docs/config-schema.md docs/cli-*.md examples/config.json
git rm examples/adapter.json
git commit -m "docs: role-only config and embedded adapter"
```

---

### Task 6: Add role capability rejection e2e

**Files:**
- Modify: `three/tests/opencode.rs` (or dedicated test file)

**Step 1: Write the failing test**

Add a role with `filesystem: read-only` for a backend that only supports read-write and assert the run fails.

```rust
#[tokio::test]
async fn e2e_role_capability_rejected() {
    let out = run_role(&cfg_path, &repo, "opencode_reader").await;
    assert!(out.stderr.contains("unsupported filesystem capability"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p three --test opencode e2e_role_capability_rejected`
Expected: FAIL until validation is wired to roles.

**Step 3: Write minimal implementation**

Ensure validation is performed per role during `resolve_profile` and errors are surfaced by the runner.

**Step 4: Run test to verify it passes**

Run: `cargo test -p three --test opencode e2e_role_capability_rejected`
Expected: PASS.

**Step 5: Commit**

```bash
git add three/tests/opencode.rs
git commit -m "test: role capability rejection e2e"
```

---

### Task 7: Full verification

**Step 1: Run unit/integration tests**

Run: `cargo test -p three`
Expected: PASS.

**Step 2: Run ignored e2e tests**

Run: `cargo test -p three -- --ignored --test-threads=1`
Expected: PASS.

---

Plan complete and saved to `docs/plans/2026-02-04-role-only-implementation-plan.md`.
Two execution options:

1. Subagent-Driven (this session) - I dispatch a fresh subagent per task, review between tasks
2. Parallel Session (separate) - Open a new session with executing-plans, batch execution with checkpoints

Which approach?
