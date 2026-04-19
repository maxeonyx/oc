use anyhow::{Result, anyhow};
use std::env;
use std::path::{Path, PathBuf};

const LEGACY_ALIASES_ENV_VAR: &str = "OC_LEGACY_ALIASES_FILE";
const SESSION_DB_ENV_VAR: &str = "OC_ALIASES_FILE";
const OPENCODE_DB_ENV_VAR: &str = "OC_OPENCODE_DB";
const DEFAULT_LEGACY_ALIASES_PATH: &str = ".config/oc/aliases";
const DEFAULT_SESSION_DB_PATH: &str = ".config/oc/oc.db";
const DEFAULT_OPENCODE_DB_PATH: &str = ".local/share/opencode/opencode.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    legacy_aliases_path: PathBuf,
    session_db_path: PathBuf,
    tmux_prefix: String,
    opencode_db_path: PathBuf,
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self> {
        let home_dir = home_dir()?;

        Ok(Self {
            legacy_aliases_path: env_path(LEGACY_ALIASES_ENV_VAR)
                .unwrap_or_else(|| home_dir.join(DEFAULT_LEGACY_ALIASES_PATH)),
            session_db_path: env_path(SESSION_DB_ENV_VAR)
                .unwrap_or_else(|| home_dir.join(DEFAULT_SESSION_DB_PATH)),
            tmux_prefix: env::var("OC_TMUX_PREFIX").unwrap_or_else(|_| String::from("oc-")),
            opencode_db_path: env_path(OPENCODE_DB_ENV_VAR)
                .unwrap_or_else(|| home_dir.join(DEFAULT_OPENCODE_DB_PATH)),
        })
    }

    pub fn legacy_aliases_path(&self) -> &Path {
        &self.legacy_aliases_path
    }

    pub fn session_db_path(&self) -> &Path {
        &self.session_db_path
    }

    pub fn tmux_prefix(&self) -> &str {
        &self.tmux_prefix
    }

    pub fn opencode_db_path(&self) -> &Path {
        &self.opencode_db_path
    }

    pub fn write_debug_dump(&self) {
        // Keep the historical key name for the hidden test/debug command while the
        // compatibility environment variable remains OC_ALIASES_FILE.
        println!("legacy_aliases_file={}", self.legacy_aliases_path.display());
        println!("aliases_file={}", self.session_db_path.display());
        println!("tmux_prefix={}", self.tmux_prefix);
        println!("opencode_db={}", self.opencode_db_path.display());
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).map(PathBuf::from)
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME must be set so oc can resolve default paths"))
}
