use anyhow::{Context, Result, anyhow, bail};
use std::env;
use std::ffi::OsString;
use std::io::{IsTerminal, stdin, stdout};
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::session::ManagedSessionRuntime;

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

    pub fn managed_session_prefix(&self) -> &str {
        &self.prefix
    }

    pub fn launch_opencode_session(
        &self,
        session_name: &str,
        directory: &Path,
        opencode_args: &[String],
    ) -> Result<()> {
        run_tmux_checked(
            new_session_command(session_name, directory, opencode_args),
            format!("start tmux session '{session_name}'"),
        )?;

        Ok(())
    }

    pub fn attach_session(&self, session_name: &str) -> Result<()> {
        if stdin().is_terminal() && stdout().is_terminal() {
            let command = attach_session_command(session_name);
            run_tmux_checked(command, format!("attach to tmux session '{session_name}'"))?;
            return Ok(());
        }

        run_tmux_checked(
            attach_session_with_pty_command(session_name),
            format!("attach to tmux session '{session_name}'"),
        )?;

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
        if is_tmux_missing_session_error(&stderr) || is_tmux_server_unavailable_error(&stderr) {
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

        self.send_keys_if_running(session_name, &["C-c"])?;
        self.send_keys_if_running(session_name, &["C-d"])?;

        Ok(())
    }

    pub fn list_managed_sessions(&self) -> Result<Vec<ManagedSessionRuntime>> {
        let output = Command::new("tmux")
            .args([
                "list-sessions",
                "-F",
                "#{session_name}\t#{session_attached}",
            ])
            .output()
            .context("failed to list tmux sessions")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if is_tmux_server_unavailable_error(&stderr) {
                return Ok(Vec::new());
            }

            return Err(anyhow!(
                "failed to list tmux sessions\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                stderr,
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| parse_managed_session_line(line, &self.prefix))
            .collect())
    }

    fn send_keys_if_running(&self, session_name: &str, keys: &[&str]) -> Result<()> {
        if !self.session_exists(session_name)? {
            return Ok(());
        }

        match self.send_keys(session_name, keys) {
            Ok(()) => Ok(()),
            Err(_error) if !self.session_exists(session_name)? => Ok(()),
            Err(error) => Err(error),
        }
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

fn new_session_command(session_name: &str, directory: &Path, opencode_args: &[String]) -> Command {
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

    command
}

fn attach_session_command(session_name: &str) -> Command {
    let mut command = Command::new("tmux");
    command.arg("attach-session").arg("-t").arg(session_name);
    command
}

fn attach_session_with_pty_command(session_name: &str) -> Command {
    let mut command = Command::new("python3");
    command
        .arg("-c")
        .arg(
            "import os, pty, sys; pid, _ = pty.fork();\nif pid == 0: os.execvp('tmux', ['tmux', 'attach-session', '-t', sys.argv[1]]);\n_, status = os.waitpid(pid, 0); raise SystemExit(os.waitstatus_to_exitcode(status))",
        )
        .arg(session_name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn parse_managed_session_line(line: &str, prefix: &str) -> Option<ManagedSessionRuntime> {
    let (session_name, attached_count) = line.split_once('\t')?;
    if !session_name.starts_with(prefix) {
        return None;
    }

    Some(ManagedSessionRuntime {
        tmux_session_name: String::from(session_name),
        attached_count: attached_count.parse().ok()?,
    })
}

pub fn is_tmux_server_unavailable_error(stderr: &str) -> bool {
    stderr.contains("no server running")
        || (stderr.contains("error connecting to") && stderr.contains("No such file or directory"))
        || stderr.contains("server exited unexpectedly")
}

fn is_tmux_missing_session_error(stderr: &str) -> bool {
    stderr.contains("can't find session")
}
