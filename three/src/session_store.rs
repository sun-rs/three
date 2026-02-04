use crate::config::Backend;
use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
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
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".local")
            .join("share")
            .join("three")
            .join("sessions.json")
    }

    pub fn compute_key(repo_root: &Path, role: &str, role_id: &str) -> String {
        let mut h = Sha256::new();
        h.update(repo_root.to_string_lossy().as_bytes());
        h.update(b"\n");
        h.update(role.as_bytes());
        h.update(b"\n");
        h.update(role_id.as_bytes());
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

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&self.path)
            .with_context(|| format!("failed to open session store: {}", self.path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("failed to lock session store: {}", self.path.display()))?;

        // Read existing JSON
        file.seek(SeekFrom::Start(0))
            .context("failed to seek to start")?;
        let mut raw = String::new();
        file.read_to_string(&mut raw)
            .context("failed to read session store")?;
        let mut sf: SessionFile = if raw.trim().is_empty() {
            SessionFile::default()
        } else {
            serde_json::from_str(&raw).context("failed to parse session store JSON")?
        };

        let out = f(&mut sf)?;

        // Persist
        let bytes = serde_json::to_vec_pretty(&sf).context("failed to serialize session store")?;
        file.set_len(0)
            .context("failed to truncate session store")?;
        file.seek(SeekFrom::Start(0))
            .context("failed to seek before write")?;
        file.write_all(&bytes)
            .context("failed to write session store")?;
        file.write_all(b"\n").ok();
        file.flush().ok();

        file.unlock().ok();
        Ok(out)
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

    #[test]
    fn compute_key_is_stable() {
        let repo = PathBuf::from("/tmp/repo");
        let k1 = SessionStore::compute_key(&repo, "role", "role");
        let k2 = SessionStore::compute_key(&repo, "role", "role");
        assert_eq!(k1, k2);
        assert_ne!(k1, SessionStore::compute_key(&repo, "role", "role2"));
    }

    #[test]
    fn put_and_get_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        let store_path = td.path().join("sessions.json");
        let store = SessionStore::new(store_path);
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let key = SessionStore::compute_key(&repo, "impl", "codex:default:default");
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
}
