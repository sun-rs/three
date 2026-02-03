use crate::config::VibeConfig;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ConfigLoader {
    user_config_path: Option<PathBuf>,
}

impl ConfigLoader {
    pub fn new(user_config_path: Option<PathBuf>) -> Self {
        Self { user_config_path }
    }

    pub fn user_config_path(&self) -> Option<&Path> {
        self.user_config_path.as_deref()
    }

    pub fn project_config_paths(repo_root: &Path) -> [PathBuf; 2] {
        // Prefer a dedicated config directory.
        let a = repo_root.join(".three").join("config.json");
        // Back-compat / convenience for small repos.
        let b = repo_root.join(".three.json");
        [a, b]
    }

    /// Load config for a repo, merging user-level config with a project override.
    ///
    /// Precedence: project overrides user. If neither exists, returns None.
    pub fn load_for_repo(&self, repo_root: &Path) -> Result<Option<VibeConfig>> {
        let user_cfg = match self.user_config_path() {
            Some(p) if p.exists() => Some(VibeConfig::load(p)?),
            _ => None,
        };

        let mut project_cfg: Option<VibeConfig> = None;
        for p in Self::project_config_paths(repo_root) {
            if p.exists() {
                project_cfg =
                    Some(VibeConfig::load(&p).with_context(|| {
                        format!("failed to load project config: {}", p.display())
                    })?);
                break;
            }
        }

        Ok(match (user_cfg, project_cfg) {
            (None, None) => None,
            (Some(u), None) => Some(u),
            (None, Some(p)) => Some(p),
            (Some(u), Some(p)) => Some(merge_config(u, p)),
        })
    }
}

fn merge_config(mut base: VibeConfig, overlay: VibeConfig) -> VibeConfig {
    // Maps are merged by key; project overrides user on conflicts.
    base.brains.extend(overlay.brains);
    base.roles.extend(overlay.roles);
    base.personas.extend(overlay.personas);
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_user_and_project_configs() {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let user_path = td.path().join("user.json");
        std::fs::write(
            &user_path,
            r#"{
  "backend": {
    "codex": {
      "type": "codex-cli",
      "models": {
        "m1": {"id":"gpt-5.2", "options": {"reasoningEffort":"high"}}
      }
    }
  },
  "roles": {
    "oracle": {"model":"codex.m1"}
  }
}"#,
        )
        .unwrap();

        let proj_dir = repo.join(".three");
        std::fs::create_dir_all(&proj_dir).unwrap();
        std::fs::write(
            proj_dir.join("config.json"),
            r#"{
  "backend": {
    "codex": {
      "type": "codex-cli",
      "models": {
        "m2": {"id":"gpt-5.2-codex", "options": {"reasoningEffort":"xhigh"}}
      }
    }
  },
  "roles": {
    "oracle": {"model":"codex.m2"}
  }
}"#,
        )
        .unwrap();

        let loader = ConfigLoader::new(Some(user_path));
        let cfg = loader.load_for_repo(&repo).unwrap().unwrap();
        assert!(cfg.brains.contains_key("codex.m1"));
        assert!(cfg.brains.contains_key("codex.m2"));
        assert_eq!(cfg.roles.get("oracle").unwrap().brain, "codex.m2");
    }
}
