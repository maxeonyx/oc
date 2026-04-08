use anyhow::{Context, Result, anyhow, bail};
use std::env;
use std::ffi::OsString;
use std::io::{IsTerminal, stdin, stdout};
use std::path::Path;
use std::process::{Command, Output};

pub struct Tmux {
    prefix: String,
}

impl Tmux {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    pub fn managed_session_name(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name)
    }

    pub fn create_detached_session(
        &self,
        session_name: &str,
        directory: &Path,
        opencode_args: &[String],
    ) -> Result<()> {
        let mut command = Command::new("tmux");
        command
            .arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(session_name)
            .arg("-c")
            .arg(directory)
            .arg("env")
            .args(current_environment_args())
            .arg("opencode")
            .args(opencode_args);

        run_tmux_checked(command, format!("create tmux session '{session_name}'"))?;

        Ok(())
    }

    pub fn attach_session(&self, session_name: &str) -> Result<()> {
        if !stdin().is_terminal() || !stdout().is_terminal() {
            return Ok(());
        }

        let mut command = Command::new("tmux");
        command.arg("attach-session").arg("-t").arg(session_name);

        run_tmux_checked(command, format!("attach to tmux session '{session_name}'"))?;

        Ok(())
    }

    pub fn session_exists(&self, session_name: &str) -> Result<bool> {
        let output = Command::new("tmux")
            .arg("has-session")
            .arg("-t")
            .arg(session_name)
            .output()
            .with_context(|| {
                format!("failed to check whether tmux session '{session_name}' exists")
            })?;

        if output.status.success() {
            return Ok(true);
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("can't find session") || stderr.contains("no server running") {
            return Ok(false);
        }

        Err(anyhow!(
            "failed to check whether tmux session '{session_name}' exists\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            stderr,
        ))
    }

    pub fn kill_session_if_exists(&self, session_name: &str) -> Result<()> {
        if !self.session_exists(session_name)? {
            return Ok(());
        }

        let mut command = Command::new("tmux");
        command.arg("kill-session").arg("-t").arg(session_name);

        run_tmux_checked(command, format!("kill tmux session '{session_name}'"))?;

        Ok(())
    }

    pub fn graceful_stop(&self, session_name: &str) -> Result<()> {
        if !self.session_exists(session_name)? {
            bail!("Session '{session_name}' is not running in tmux");
        }

        self.send_keys(session_name, &["C-c"])?;

        if !self.session_exists(session_name)? {
            return Ok(());
        }

        match self.send_keys(session_name, &["C-d"]) {
            Ok(()) => Ok(()),
            Err(_error) if !self.session_exists(session_name)? => Ok(()),
            Err(error) => Err(error),
        }?;

        Ok(())
    }

    fn send_keys(&self, session_name: &str, keys: &[&str]) -> Result<()> {
        let mut command = Command::new("tmux");
        command
            .arg("send-keys")
            .arg("-t")
            .arg(session_name)
            .args(keys);

        run_tmux_checked(
            command,
            format!("send keys to tmux session '{session_name}'"),
        )?;

        Ok(())
    }
}

fn run_tmux_checked(mut command: Command, description: String) -> Result<Output> {
    let output = command
        .output()
        .with_context(|| format!("failed to {description}"))?;

    if !output.status.success() {
        return Err(anyhow!(
            "failed to {description}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    Ok(output)
}

fn current_environment_args() -> Vec<OsString> {
    env::vars_os()
        .map(|(key, value)| {
            let mut assignment = OsString::new();
            assignment.push(key);
            assignment.push("=");
            assignment.push(value);
            assignment
        })
        .collect()
}
