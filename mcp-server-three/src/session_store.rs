use crate::config::Backend;
use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub repo_root: String,
    pub role: String,
    pub role_id: String,
    pub backend: Backend,
    pub backend_session_id: String,
    /// For MCP sampling-based backends (e.g. Claude), we persist a short conversation history
    /// to approximate "session" reuse.
    #[serde(default)]
    pub sampling_history: Vec<SamplingHistoryMessage>,
    pub updated_at_unix_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingHistoryMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionFile {
    version: u32,
    records: BTreeMap<String, SessionRecord>,
}

impl Default for SessionFile {
    fn default() -> Self {
        Self {
            version: 1,
            records: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    path: PathBuf,
    locks_dir: PathBuf,
}

impl SessionStore {
    pub fn new(path: PathBuf) -> Self {
        let locks_dir = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("locks");
        Self { path, locks_dir }
    }

    pub fn default_path() -> PathBuf {
        // Prefer XDG-style data layout:
        // - $XDG_DATA_HOME/three/sessions.json
        // - ~/.local/share/three/sessions.json
        if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(base).join("three").join("sessions.json");
        }
        if cfg!(windows) {
            if let Some(dir) = dirs::data_dir() {
                return dir.join("three").join("sessions.json");
            }
        }
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".local")
            .join("share")
            .join("three")
            .join("sessions.json")
    }

    pub fn compute_key(repo_root: &Path, role: &str, role_id: &str) -> String {
        Self::compute_key_with_scope(repo_root, role, role_id, None, None)
    }

    pub fn compute_key_with_scope(
        repo_root: &Path,
        role: &str,
        role_id: &str,
        client: Option<&str>,
        conversation_id: Option<&str>,
    ) -> String {
        let mut h = Sha256::new();
        h.update(repo_root.to_string_lossy().as_bytes());
        h.update(b"\n");
        h.update(role.as_bytes());
        h.update(b"\n");
        h.update(role_id.as_bytes());
        h.update(b"\n");
        h.update(client.unwrap_or("-").as_bytes());
        h.update(b"\n");
        h.update(conversation_id.unwrap_or("-").as_bytes());
        hex::encode(h.finalize())
    }

    pub fn acquire_key_lock(&self, key: &str) -> Result<KeyLock> {
        std::fs::create_dir_all(&self.locks_dir)
            .with_context(|| format!("failed to create locks dir: {}", self.locks_dir.display()))?;
        let lock_path = self.locks_dir.join(format!("{}.lock", key));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)
            .with_context(|| format!("failed to open lock file: {}", lock_path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("failed to lock: {}", lock_path.display()))?;
        Ok(KeyLock { file })
    }

    pub fn get(&self, key: &str) -> Result<Option<SessionRecord>> {
        self.with_store(|sf| Ok(sf.records.get(key).cloned()))
    }

    pub fn put(&self, key: &str, record: SessionRecord) -> Result<()> {
        self.with_store(|sf| {
            sf.records.insert(key.to_string(), record);
            Ok(())
        })
    }

    fn with_store<T>(&self, f: impl FnOnce(&mut SessionFile) -> Result<T>) -> Result<T> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create store dir: {}", parent.display()))?;
        }

        let lock_path = self.path.with_extension("lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)
            .with_context(|| format!("failed to open session lock: {}", lock_path.display()))?;
        lock_file
            .lock_exclusive()
            .with_context(|| format!("failed to lock session store: {}", self.path.display()))?;

        let raw = match std::fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                lock_file.unlock().ok();
                return Err(err).context("failed to read session store");
            }
        };

        let mut sf: SessionFile = if raw.trim().is_empty() {
            SessionFile::default()
        } else {
            match serde_json::from_str(&raw) {
                Ok(parsed) => parsed,
                Err(err) => {
                    let backup_path = self.backup_corrupt_store();
                    if let Err(backup_err) = backup_path {
                        eprintln!(
                            "warning: failed to backup corrupt session store {}: {}",
                            self.path.display(),
                            backup_err
                        );
                    }
                    eprintln!(
                        "warning: session store JSON invalid ({}), resetting to empty",
                        err
                    );
                    SessionFile::default()
                }
            }
        };

        let out = f(&mut sf)?;

        let bytes = serde_json::to_vec_pretty(&sf).context("failed to serialize session store")?;
        self.write_atomic(&bytes)?;

        lock_file.unlock().ok();
        Ok(out)
    }
}

impl SessionStore {
    fn backup_corrupt_store(&self) -> Result<PathBuf> {
        if !self.path.exists() {
            return Ok(self.path.clone());
        }
        let file_name = self
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let backup_name = format!("{}.bak.{}", file_name, now_unix_secs());
        let backup_path = self.path.with_file_name(backup_name);
        std::fs::rename(&self.path, &backup_path).with_context(|| {
            format!(
                "failed to backup corrupt store to {}",
                backup_path.display()
            )
        })?;
        Ok(backup_path)
    }

    fn write_atomic(&self, bytes: &[u8]) -> Result<()> {
        let tmp_path = self.path.with_extension("tmp");
        {
            let mut tmp = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .with_context(|| format!("failed to open temp store: {}", tmp_path.display()))?;
            tmp.write_all(bytes).context("failed to write temp store")?;
            tmp.write_all(b"\n").ok();
            tmp.flush().ok();
            tmp.sync_all().ok();
        }

        std::fs::rename(&tmp_path, &self.path)
            .with_context(|| format!("failed to replace store: {}", self.path.display()))?;

        if let Some(parent) = self.path.parent() {
            let _ = OpenOptions::new()
                .read(true)
                .open(parent)
                .and_then(|dir| dir.sync_all());
        }
        Ok(())
    }
}

pub struct KeyLock {
    file: std::fs::File,
}

impl Drop for KeyLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn compute_key_is_stable() {
        let repo = PathBuf::from("/tmp/repo");
        let k1 = SessionStore::compute_key_with_scope(&repo, "role", "role", None, None);
        let k2 = SessionStore::compute_key_with_scope(&repo, "role", "role", None, None);
        assert_eq!(k1, k2);
        assert_ne!(
            k1,
            SessionStore::compute_key_with_scope(&repo, "role", "role2", None, None)
        );
    }

    #[test]
    fn compute_key_scopes_client_and_conversation() {
        let repo = PathBuf::from("/tmp/repo");
        let base = SessionStore::compute_key_with_scope(&repo, "oracle", "oracle", None, None);
        let by_client =
            SessionStore::compute_key_with_scope(&repo, "oracle", "oracle", Some("claude"), None);
        let by_conversation = SessionStore::compute_key_with_scope(
            &repo,
            "oracle",
            "oracle",
            Some("claude"),
            Some("conv-a"),
        );

        assert_ne!(base, by_client);
        assert_ne!(by_client, by_conversation);
    }

    #[test]
    fn put_and_get_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let key = SessionStore::compute_key_with_scope(
            &repo,
            "impl",
            "codex:default:default",
            None,
            None,
        );
        store
            .put(
                &key,
                SessionRecord {
                    repo_root: repo.to_string_lossy().to_string(),
                    role: "impl".to_string(),
                    role_id: "codex:default:default".to_string(),
                    backend: Backend::Codex,
                    backend_session_id: "sess-1".to_string(),
                    sampling_history: Vec::new(),
                    updated_at_unix_secs: 1,
                },
            )
            .unwrap();

        let rec = store.get(&key).unwrap().unwrap();
        assert_eq!(rec.backend_session_id, "sess-1");
        assert_eq!(rec.backend, Backend::Codex);
    }

    #[test]
    fn corrupt_store_is_backed_up_and_reset() {
        let td = tempfile::tempdir().unwrap();
        let store_path = td.path().join("sessions.json");
        fs::write(&store_path, "{").unwrap();

        let store = SessionStore::new(store_path.clone());
        let res = store.get("missing");
        assert!(res.is_ok());

        let raw = fs::read_to_string(&store_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["version"], 1);
        assert!(v["records"].is_object());

        let mut backups = 0;
        for entry in fs::read_dir(td.path()).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("sessions.json.bak.") {
                backups += 1;
            }
        }
        assert!(backups >= 1);
    }
}
