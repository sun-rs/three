use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

pub struct ScopedEnvVar {
    _lock: MutexGuard<'static, ()>,
    key: &'static str,
    prev: Option<String>,
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match self.prev.as_deref() {
            Some(v) => unsafe {
                std::env::set_var(self.key, v);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
}

fn codex_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn gemini_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn scoped_codex_bin(path: &str) -> ScopedEnvVar {
    let lock = codex_lock().lock().unwrap();
    let prev = std::env::var("CODEX_BIN").ok();
    unsafe {
        std::env::set_var("CODEX_BIN", path);
    }
    ScopedEnvVar {
        _lock: lock,
        key: "CODEX_BIN",
        prev,
    }
}

pub fn scoped_gemini_bin(path: &str) -> ScopedEnvVar {
    let lock = gemini_lock().lock().unwrap();
    let prev = std::env::var("GEMINI_BIN").ok();
    unsafe {
        std::env::set_var("GEMINI_BIN", path);
    }
    ScopedEnvVar {
        _lock: lock,
        key: "GEMINI_BIN",
        prev,
    }
}

pub fn example_config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("config.json")
}
