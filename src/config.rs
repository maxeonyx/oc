use anyhow::{Result, anyhow};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    session_db_path: PathBuf,
    tmux_prefix: String,
    opencode_db: Option<PathBuf>,
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self> {
        let home_dir = home_dir()?;

        Ok(Self {
            session_db_path: env_path("OC_ALIASES_FILE")
                .unwrap_or_else(|| home_dir.join(".config/oc/oc.db")),
            tmux_prefix: env::var("OC_TMUX_PREFIX").unwrap_or_else(|_| String::from("oc-")),
            opencode_db: env_path("OC_OPENCODE_DB"),
        })
    }

    pub fn session_db_path(&self) -> &PathBuf {
        &self.session_db_path
    }

    pub fn write_debug_dump(&self) {
        println!("aliases_file={}", self.session_db_path.display());
        println!("tmux_prefix={}", self.tmux_prefix);

        match &self.opencode_db {
            Some(path) => println!("opencode_db={}", path.display()),
            None => println!("opencode_db="),
        }
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
