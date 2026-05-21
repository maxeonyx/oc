use clap::{CommandFactory, Parser, Subcommand, ValueEnum, ValueHint};
use clap_complete::{Generator, Shell};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestedAction {
    Completion {
        shell: CompletionShell,
    },
    New {
        name: String,
        dir: Option<PathBuf>,
        launch_args: Vec<String>,
    },
    Alias {
        name: String,
        dir: Option<PathBuf>,
    },
    Unalias {
        name: String,
    },
    Rm {
        target: String,
    },
    Stop {
        target: String,
    },
    Restart {
        target: String,
    },
    Move {
        target: String,
        new_dir: PathBuf,
    },
    Migrate,
    AttachTarget {
        target: String,
    },
    Default,
    List {
        json: bool,
    },
    DbPath,
    DumpSessionList,
    DumpRuntimeConfig,
    ParseMemoryStatus {
        path: PathBuf,
    },
}

#[derive(Debug, Parser)]
#[command(
    name = "oc",
    version,
    about = "Interactive TUI session manager for OpenCode"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(value_hint = ValueHint::Other)]
    pub target: Option<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Fish,
    Zsh,
}

impl From<CompletionShell> for Shell {
    fn from(value: CompletionShell) -> Self {
        match value {
            CompletionShell::Bash => Shell::Bash,
            CompletionShell::Fish => Shell::Fish,
            CompletionShell::Zsh => Shell::Zsh,
        }
    }
}

impl Cli {
    pub fn requested_action(self) -> RequestedAction {
        match (self.command, self.target) {
            (Some(Command::Completion { shell }), None) => RequestedAction::Completion { shell },
            (
                Some(Command::New {
                    name,
                    dir,
                    launch_args,
                }),
                None,
            ) => RequestedAction::New {
                name,
                dir,
                launch_args,
            },
            (Some(Command::Alias { name, dir }), None) => RequestedAction::Alias { name, dir },
            (Some(Command::Unalias { name }), None) => RequestedAction::Unalias { name },
            (Some(Command::Rm { target }), None) => RequestedAction::Rm { target },
            (Some(Command::Stop { target }), None) => RequestedAction::Stop { target },
            (Some(Command::Restart { target }), None) => RequestedAction::Restart { target },
            (Some(Command::Move { target, new_dir }), None) => {
                RequestedAction::Move { target, new_dir }
            }
            (Some(Command::Migrate), None) => RequestedAction::Migrate,
            (Some(Command::List { json }), None) => RequestedAction::List { json },
            (Some(Command::DbPath), None) => RequestedAction::DbPath,
            (Some(Command::DumpSessionList), None) => RequestedAction::DumpSessionList,
            (Some(Command::DumpRuntimeConfig), None) => RequestedAction::DumpRuntimeConfig,
            (Some(Command::ParseMemoryStatus { path }), None) => {
                RequestedAction::ParseMemoryStatus { path }
            }
            (None, Some(target)) => RequestedAction::AttachTarget { target },
            (None, None) => RequestedAction::Default,
            (Some(_), Some(target)) => {
                panic!("clap should not accept both a subcommand and bare target: {target}")
            }
        }
    }
}

pub fn print_completion(shell: CompletionShell) {
    let mut command = Cli::command();
    let bin_name = command.get_name().to_string();
    let generator: Shell = shell.into();
    let mut output = Vec::new();
    generate(generator, &mut command, bin_name, &mut output);

    if shell == CompletionShell::Fish {
        let output = String::from_utf8(output).expect("generated fish completion should be utf-8");
        let output = sanitize_fish_completion(output);
        io::stdout()
            .write_all(output.as_bytes())
            .expect("failed to write fish completion");
    } else {
        io::stdout()
            .write_all(&output)
            .expect("failed to write completion");
    }
}

fn sanitize_fish_completion(output: String) -> String {
    const HIDDEN_SUBCOMMANDS: [&str; 3] = [
        "__dump-session-list",
        "__dump-runtime-config",
        "__parse-memory-status",
    ];

    output
        .lines()
        .filter(|line| {
            !HIDDEN_SUBCOMMANDS.iter().any(|subcommand| {
                line.contains(&format!("-a \"{subcommand}\""))
                    || line.contains(&format!("__fish_oc_using_subcommand {subcommand}"))
            })
        })
        .map(|line| {
            HIDDEN_SUBCOMMANDS
                .iter()
                .fold(line.to_string(), |line, subcommand| {
                    line.replace(&format!(" {subcommand}"), "")
                })
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn generate(
    generator: impl Generator,
    command: &mut clap::Command,
    bin_name: String,
    output: &mut dyn io::Write,
) {
    clap_complete::generate(generator, command, bin_name, output);
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Generate shell completion scripts")]
    Completion { shell: CompletionShell },
    #[command(visible_alias = "n", about = "Create a new OpenCode session")]
    New {
        #[arg(value_hint = ValueHint::Other)]
        name: String,
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
        #[arg(last = true)]
        launch_args: Vec<String>,
    },
    #[command(about = "Create an alias for a directory")]
    Alias {
        #[arg(value_hint = ValueHint::Other)]
        name: String,
        #[arg(value_hint = ValueHint::DirPath)]
        dir: Option<PathBuf>,
    },
    #[command(about = "Remove a saved directory alias")]
    Unalias {
        #[arg(value_hint = ValueHint::Other)]
        name: String,
    },
    #[command(name = "rm", visible_aliases = ["delete", "d"], about = "Remove a session from the database")]
    Rm {
        #[arg(value_hint = ValueHint::Other)]
        target: String,
    },
    #[command(about = "Stop a running session")]
    Stop {
        #[arg(value_hint = ValueHint::Other)]
        target: String,
    },
    #[command(about = "Restart a session")]
    Restart {
        #[arg(value_hint = ValueHint::Other)]
        target: String,
    },
    #[command(name = "mv", about = "Move a session to a new directory")]
    Move {
        #[arg(value_hint = ValueHint::Other)]
        target: String,
        #[arg(value_hint = ValueHint::DirPath)]
        new_dir: PathBuf,
    },
    #[command(about = "Migrate legacy aliases into the database")]
    Migrate,
    #[command(about = "List tracked sessions")]
    List {
        #[arg(long)]
        json: bool,
    },
    #[command(name = "db-path", about = "Print the database path")]
    DbPath,
    #[command(name = "__dump-session-list", hide = true)]
    DumpSessionList,
    #[command(name = "__dump-runtime-config", hide = true)]
    DumpRuntimeConfig,
    #[command(name = "__parse-memory-status", hide = true)]
    ParseMemoryStatus {
        #[arg(value_hint = ValueHint::FilePath)]
        path: PathBuf,
    },
}
