use anyhow::{Context, Result, anyhow, bail};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, stdin, stdout};
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
        launch_args: &[String],
    ) -> Result<()> {
        run_tmux_checked(
            new_session_command(session_name, directory, launch_args),
            format!("start tmux session '{session_name}'"),
        )?;

        Ok(())
    }

    pub fn attach_session(&self, session_name: &str) -> Result<()> {
        if stdin().is_terminal() && stdout().is_terminal() {
            if env::var_os("TMUX").is_some() {
                bail!("tmux attach requires running oc from outside tmux");
            }

            run_tmux_interactive_checked(
                attach_session_command(session_name),
                format!("attach to tmux session '{session_name}'"),
            )?;
            return Ok(());
        }

        run_tmux_checked(
            attach_session_with_pty_command(session_name),
            format!("attach to tmux session '{session_name}'"),
        )?;

        Ok(())
    }

    pub fn session_exists(&self, session_name: &str) -> Result<bool> {
        let exact_session_name = exact_session_target(session_name);
        let output = Command::new("tmux")
            .arg("has-session")
            .arg("-t")
            .arg(&exact_session_name)
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
        command
            .arg("kill-session")
            .arg("-t")
            .arg(exact_session_target(session_name));

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

    pub fn restart_session(
        &self,
        session_name: &str,
        directory: &Path,
        launch_args: &[String],
    ) -> Result<()> {
        self.graceful_stop(session_name)?;

        wait_for_session_exit(session_name, std::time::Duration::from_secs(10))?;

        self.launch_opencode_session(session_name, directory, launch_args)?;
        wait_for_pane(session_name, std::time::Duration::from_secs(10))?;
        self.send_keys(session_name, &["continue", "Enter"])?;

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
            .map(enrich_runtime)
            .collect())
    }

    pub fn pane_pid(&self, session_name: &str) -> Result<Option<u32>> {
        pane_pid(session_name)
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
            .arg(exact_pane_target(session_name))
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

fn run_tmux_interactive_checked(mut command: Command, description: String) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("failed to {description}"))?;

    if !status.success() {
        return Err(anyhow!("failed to {description} (exit status: {status})"));
    }

    Ok(())
}

fn wait_for_session_exit(session_name: &str, timeout: std::time::Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    let exact_session_name = exact_session_target(session_name);

    while std::time::Instant::now() < deadline {
        let output = Command::new("tmux")
            .arg("has-session")
            .arg("-t")
            .arg(&exact_session_name)
            .output()
            .with_context(|| {
                format!("failed to check whether tmux session '{session_name}' still exists")
            })?;

        if output.status.success() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if is_tmux_missing_session_error(&stderr) || is_tmux_server_unavailable_error(&stderr) {
            return Ok(());
        }
    }

    bail!("Session '{session_name}' did not stop before restart timeout")
}

fn wait_for_pane(session_name: &str, timeout: std::time::Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    let exact_pane_name = exact_pane_target(session_name);

    while std::time::Instant::now() < deadline {
        let output = Command::new("tmux")
            .args([
                "display-message",
                "-p",
                "-t",
                &exact_pane_name,
                "#{pane_id}",
            ])
            .output()
            .with_context(|| format!("failed to wait for tmux pane in session '{session_name}'"))?;

        if output.status.success() {
            return Ok(());
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    bail!("Session '{session_name}' did not expose a pane before restart timeout")
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

fn new_session_command(session_name: &str, directory: &Path, launch_args: &[String]) -> Command {
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
        .args(launch_args);

    command
}

fn attach_session_command(session_name: &str) -> Command {
    let mut command = Command::new("tmux");
    command
        .arg("attach-session")
        .arg("-t")
        .arg(exact_session_target(session_name));
    command
}

fn attach_session_with_pty_command(session_name: &str) -> Command {
    let mut command = Command::new("python3");
    command
        .arg("-c")
        .arg(
            "import os, pty, sys; pid, _ = pty.fork();\nif pid == 0: os.execvp('tmux', ['tmux', 'attach-session', '-t', sys.argv[1]]);\n_, status = os.waitpid(pid, 0); raise SystemExit(os.waitstatus_to_exitcode(status))",
        )
        .arg(exact_session_target(session_name))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if env::var_os("TERM").is_none() {
        command.env("TERM", "screen");
    }

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
        pane_pid: None,
        memory_bytes: None,
        tree_memory_bytes: None,
    })
}

fn enrich_runtime(mut runtime: ManagedSessionRuntime) -> ManagedSessionRuntime {
    runtime.pane_pid = pane_pid(&runtime.tmux_session_name).ok().flatten();
    runtime.memory_bytes = runtime.pane_pid.and_then(|pid| {
        read_process_memory_bytes(Path::new(&format!("/proc/{pid}/status")))
            .ok()
            .flatten()
    });
    runtime.tree_memory_bytes = runtime.pane_pid.and_then(|pid| {
        read_process_tree_memory_bytes(Path::new("/proc"), pid)
            .ok()
            .flatten()
    });
    runtime
}

fn pane_pid(session_name: &str) -> Result<Option<u32>> {
    let output = Command::new("tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            &exact_pane_target(session_name),
            "#{pane_pid}",
        ])
        .output()
        .with_context(|| format!("failed to read pane pid for tmux session '{session_name}'"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let pid = String::from_utf8_lossy(&output.stdout).trim().parse().ok();
    Ok(pid)
}

pub fn read_process_memory_bytes(status_path: &Path) -> Result<Option<u64>> {
    let status = fs::read_to_string(status_path).with_context(|| {
        format!(
            "failed to read process status file {}",
            status_path.display()
        )
    })?;

    Ok(parse_memory_status(&status))
}

pub fn read_process_tree_memory_bytes(proc_root: &Path, root_pid: u32) -> Result<Option<u64>> {
    let root_status_path = proc_root.join(root_pid.to_string()).join("status");
    let Some(root_memory_bytes) = read_process_memory_bytes_if_exists(&root_status_path)? else {
        return Ok(None);
    };

    let child_map = read_process_children(proc_root)?;
    let mut total_bytes = root_memory_bytes;
    let mut visited = HashSet::from([root_pid]);
    let mut stack = child_map.get(&root_pid).cloned().unwrap_or_default();

    while let Some(pid) = stack.pop() {
        if !visited.insert(pid) {
            continue;
        }

        if let Some(memory_bytes) =
            read_process_memory_bytes_if_exists(&proc_root.join(pid.to_string()).join("status"))?
        {
            total_bytes += memory_bytes;
        }

        if let Some(children) = child_map.get(&pid) {
            stack.extend(children.iter().copied());
        }
    }

    Ok(Some(total_bytes))
}

pub fn parse_memory_status(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let value = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(value * 1024)
}

fn read_process_memory_bytes_if_exists(status_path: &Path) -> Result<Option<u64>> {
    match read_process_memory_bytes(status_path) {
        Ok(memory_bytes) => Ok(memory_bytes),
        Err(error) if is_not_found_error(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_process_children(proc_root: &Path) -> Result<HashMap<u32, Vec<u32>>> {
    let mut child_map = HashMap::<u32, Vec<u32>>::new();

    for entry in fs::read_dir(proc_root)
        .with_context(|| format!("failed to list proc directory {}", proc_root.display()))?
    {
        let entry = entry.with_context(|| {
            format!("failed to read entry in proc directory {}", proc_root.display())
        })?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|file_name| file_name.parse::<u32>().ok())
        else {
            continue;
        };

        let stat_path = entry.path().join("stat");
        let stat = match fs::read_to_string(&stat_path) {
            Ok(stat) => stat,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read {}", stat_path.display()));
            }
        };

        let Some(parent_pid) = parse_process_parent_pid(&stat) else {
            continue;
        };

        child_map.entry(parent_pid).or_default().push(pid);
    }

    Ok(child_map)
}

fn parse_process_parent_pid(stat: &str) -> Option<u32> {
    let (_, fields) = stat.rsplit_once(") ")?;
    fields.split_whitespace().nth(1)?.parse::<u32>().ok()
}

fn is_not_found_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|io_error| io_error.kind() == io::ErrorKind::NotFound)
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

fn exact_session_target(session_name: &str) -> String {
    format!("={session_name}")
}

fn exact_pane_target(session_name: &str) -> String {
    format!("={session_name}:")
}
