//! Multi-path dotenv loader for local kit development and the `kit` HTTP binary.
//!
//! Loads files (dotenv does not override already-set variables):
//! - `CLAWD_ENV_FILE` if set
//! - process CWD: `.env`, `.env.local`, `src/.env.local`
//! - crate root (`CARGO_MANIFEST_DIR`): same names

use std::path::{Path, PathBuf};

const ENV_NAMES: &[&str] = &[".env", ".env.local", "src/.env.local"];

/// Load kit environment files. Safe to call multiple times.
/// Returns unique paths that were successfully loaded.
pub fn load_dotenv_files() -> Vec<PathBuf> {
    let mut loaded = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut try_once = |p: PathBuf| {
        let key = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        if !seen.insert(key) {
            return;
        }
        if try_load(&p) {
            loaded.push(p);
        }
    };

    if let Ok(custom) = std::env::var("CLAWD_ENV_FILE") {
        if !custom.is_empty() {
            try_once(PathBuf::from(custom));
        }
    }

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    roots.sort();
    roots.dedup();

    for root in roots {
        for name in ENV_NAMES {
            try_once(root.join(name));
        }
    }

    loaded
}

fn try_load(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    dotenv::from_path(path).is_ok()
}

/// Help text when kit cannot start.
pub fn env_load_hint() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!(
        "Put secrets in one of:\n\
           {manifest}/.env\n\
           {manifest}/.env.local\n\
           {manifest}/src/.env.local\n\
         Or: export CLAWD_ENV_FILE=/absolute/path/to/env\n\
         Template: cp .env.example .env\n\
         \n\
         Default HTTP mode is LOCAL (no Privy):\n\
           SOLANA_PRIVATE_KEY=...   # required for signing\n\
           SOLANA_RPC_URL=...       # recommended\n\
           KIT_AUTH_MODE=local      # default\n\
         LLM defaults to Clawd mesh (no Anthropic key needed):\n\
           CLAWD_MESH_BASE_URL=https://clawd-inference-mesh.fly.dev/v1\n\
           CLAWD_MESH_MODEL=zkrouter/auto\n\
           (alias: https://mesh.x402.wtf/v1)\n\
         \n\
         Optional multi-user Privy mode:\n\
           KIT_AUTH_MODE=privy\n\
           PRIVY_APP_ID / PRIVY_APP_SECRET / PRIVY_VERIFICATION_KEY\n\
         \n\
         Docs: docs/configuration.md · docs/http_service.md\n\
         One-shot: npm run kit"
    )
}

/// True if Privy env is fully set (optional path only).
pub fn privy_env_ready() -> bool {
    ["PRIVY_APP_ID", "PRIVY_APP_SECRET", "PRIVY_VERIFICATION_KEY"]
        .iter()
        .all(|k| std::env::var(k).map(|v| !v.trim().is_empty()).unwrap_or(false))
}

/// True if local kit mode can start.
pub fn local_env_ready() -> bool {
    std::env::var("SOLANA_PRIVATE_KEY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_dotenv_files_is_idempotent() {
        let a = load_dotenv_files();
        let b = load_dotenv_files();
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn env_load_hint_mentions_local_default() {
        let h = env_load_hint();
        assert!(h.contains("SOLANA_PRIVATE_KEY"));
        assert!(h.contains("KIT_AUTH_MODE"));
        assert!(h.contains("local"));
    }
}
