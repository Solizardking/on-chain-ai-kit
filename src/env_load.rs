//! Multi-path dotenv loader for local kit development and the `kit` HTTP binary.
//!
//! Loads the first matching files (later files fill only missing keys via
//! `dotenv`, which does not override already-set variables):
//! - process CWD: `.env`, `.env.local`, `src/.env.local`
//! - crate root (`CARGO_MANIFEST_DIR`): same names
//! - `CLAWD_ENV_FILE` if set (single explicit path, loaded first)

use std::path::{Path, PathBuf};

/// Relative env filenames searched under each root.
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

/// Human-readable help when Privy (or other) env is still missing after load.
pub fn env_load_hint() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!(
        "Missing env after loading dotenv files.\n\
         Put secrets in one of:\n\
           {manifest}/.env\n\
           {manifest}/.env.local\n\
           {manifest}/src/.env.local\n\
           ./.env  (current directory)\n\
         Or: export CLAWD_ENV_FILE=/absolute/path/to/env\n\
         Template: cp .env.example .env\n\
         Docs: docs/configuration.md · docs/http_service.md · docs/authentication.md\n\
         Required for `cargo run --features full --bin kit`:\n\
           PRIVY_APP_ID, PRIVY_APP_SECRET, PRIVY_VERIFICATION_KEY\n\
         Optional: ANTHROPIC_API_KEY, SOLANA_RPC_URL\n\
         One-shot: npm install && npm run kit   ·   npx openclawd-solana-kit start"
    )
}

/// True if the three Privy vars required by the HTTP binary are set (non-empty).
pub fn privy_env_ready() -> bool {
    ["PRIVY_APP_ID", "PRIVY_APP_SECRET", "PRIVY_VERIFICATION_KEY"]
        .iter()
        .all(|k| std::env::var(k).map(|v| !v.trim().is_empty()).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_dotenv_files_is_idempotent() {
        let a = load_dotenv_files();
        let b = load_dotenv_files();
        // Second call still returns the same set of existing paths (or empty).
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn env_load_hint_mentions_privy_and_example() {
        let h = env_load_hint();
        assert!(h.contains("PRIVY_APP_ID"));
        assert!(h.contains(".env.example"));
        assert!(h.contains("configuration.md"));
    }
}
