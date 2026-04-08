use anyhow::{Result, bail};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSession {
    pub id: i64,
    pub name: String,
    pub directory: PathBuf,
    pub opencode_session_id: Option<String>,
    pub opencode_args_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSessionAlias {
    pub name: String,
    pub directory: PathBuf,
    pub opencode_args: Vec<String>,
}

impl NewSessionAlias {
    pub fn new(name: String, directory: PathBuf, opencode_args: Vec<String>) -> Result<Self> {
        validate_session_name(&name)?;

        Ok(Self {
            name,
            directory,
            opencode_args,
        })
    }
}

fn validate_session_name(name: &str) -> Result<()> {
    if name.parse::<u64>().is_ok() {
        bail!(
            "Session name '{name}' cannot be a plain number because numeric IDs and names must stay unambiguous"
        );
    }

    Ok(())
}
