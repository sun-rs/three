# Adapter Capability Validation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enforce per-brain filesystem capability validation using adapter-defined support lists so unsupported read-only/write settings fail only for the selected brain.

**Architecture:** Add an optional `filesystem_capabilities` list to each adapter definition and validate requested `capabilities.filesystem` during `resolve_profile`. Missing lists mean "no enforcement." Update examples and docs to reflect per-brain failure for unsupported capabilities.

**Tech Stack:** Rust (serde), JSON config, MiniJinja templates

---

### Task 1: Write failing tests for adapter-driven capability enforcement

**Files:**
- Modify: `three/src/config.rs`
- Test: `three/src/config.rs`

**Step 1: Write the failing tests**
Add/adjust tests so they depend on adapter capability lists:

```rust
#[test]
fn rejects_readonly_for_opencode_on_resolve_only() {
    let td = tempfile::tempdir().unwrap();
    let repo = td.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let path = td.path().join("cfg.json");
    write_cfg(
        &path,
        r#"{
  \"backend\": { \"opencode\": { \"models\": { \"opencode-gpt-5\": {} } } },
  \"brains\": {
    \"reader\": { \"model\": \"opencode/opencode-gpt-5\", \"personas\": { \"description\": \"d\", \"prompt\": \"p\" },
      \"capabilities\": { \"filesystem\": \"read-only\", \"shell\": \"deny\", \"network\": \"deny\", \"tools\": [\"read\"] } },
    \"writer\": { \"model\": \"opencode/opencode-gpt-5\", \"personas\": { \"description\": \"d\", \"prompt\": \"p\" },
      \"capabilities\": { \"filesystem\": \"read-write\", \"shell\": \"deny\", \"network\": \"deny\", \"tools\": [\"read\"] } }
  }
}"#,
    );

    let (_, adapter_path) = crate::test_utils::example_config_paths();
    let loader = ConfigLoader::new(Some(path)).with_adapter_path(Some(adapter_path));
    let cfg = loader.load_for_repo(&repo).unwrap().unwrap();

    let writer = cfg.resolve_profile(Some("writer"), None).unwrap();
    assert_eq!(writer.profile.backend_id, "opencode");

    let err = cfg.resolve_profile(Some("reader"), None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("filesystem capability") && msg.contains("opencode"), "unexpected error: {msg}");
}
```

Add a similar test for Kimi (same structure, `backend: kimi`).

**Step 2: Run tests to verify failure**
Run:
- `cargo test -p three rejects_readonly_for_opencode_on_resolve_only`
- `cargo test -p three rejects_readonly_for_kimi_on_resolve_only`

Expected: FAIL because adapter capability checks are not implemented yet.

---

### Task 2: Implement adapter capability parsing + per-brain validation

**Files:**
- Modify: `three/src/config.rs`

**Step 1: Implement minimal code**
- Extend `AdapterConfig`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct AdapterConfig {
    pub args_template: Vec<String>,
    pub output_parser: OutputParserConfig,
    #[serde(default)]
    pub filesystem_capabilities: Option<Vec<FilesystemCapability>>,
}
```

- In `resolve_profile`, after `adapter` is loaded and before returning, validate:

```rust
if let Some(list) = adapter.filesystem_capabilities.as_ref() {
    if !list.contains(&brain_cfg.capabilities.filesystem) {
        return Err(anyhow!(
            "unsupported filesystem capability '{}' for backend '{}' (brain '{}')",
            brain_cfg.capabilities.filesystem,
            backend_id,
            brain_id
        ));
    }
}
```

**Step 2: Run tests to verify pass**
Run:
- `cargo test -p three rejects_readonly_for_opencode_on_resolve_only`
- `cargo test -p three rejects_readonly_for_kimi_on_resolve_only`

Expected: PASS.

---

### Task 3: Update adapter examples with capability lists

**Files:**
- Modify: `examples/adapter.json`

**Step 1: Add `filesystem_capabilities` per adapter**
Example:
```json
"opencode": {
  "filesystem_capabilities": ["read-write"],
  "args_template": [ ... ],
  "output_parser": { ... }
}
```
Do the same for `kimi` (read-write only) and for `codex/claude/gemini` (read-only + read-write).

**Step 2: Run adapter-related tests**
Run:
- `cargo test -p three config::` (or the specific tests touching adapter parsing)

Expected: PASS.

---

### Task 4: Update docs for per-brain validation

**Files:**
- Modify: `docs/config-schema.md`
- Modify: `docs/cli-opencode.md`
- Modify: `docs/cli-kimi.md`

**Step 1: Document new adapter field**
Add notes:
- `adapter.filesystem_capabilities` is the authoritative support list.
- Unsupported brain capabilities fail at `resolve_profile` (per-brain), not global config load.

**Step 2: Run doc-related tests (if any)**
No automated doc tests. Manually re-open files for sanity.

---

### Task 5: Full test pass (unit + optional e2e)

**Step 1: Unit test sweep**
Run:
- `cargo test -p three opencode`

Expected: PASS.

**Step 2: Optional ignored e2e**
Run:
- `cargo test -p three cfgtest_real_opencode_smoke -- --ignored`
- `cargo test -p three cfgtest_real_gemini_include_directories_reads_multiple_external_files -- --ignored`

Expected: PASS (requires working provider credentials).

---

### Task 6: Commit (optional, only if requested)

```bash
git add docs/plans/2026-02-04-adapter-capability-validation-plan.md three/src/config.rs examples/adapter.json docs/config-schema.md docs/cli-opencode.md docs/cli-kimi.md

git commit -m "feat: validate filesystem capabilities per adapter"
```
