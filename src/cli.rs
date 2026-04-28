use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestedAction {
    New {
        name: String,
        dir: Option<PathBuf>,
        opencode_args: Vec<String>,
    },
    Alias {
        name: String,
        dir: Option<PathBuf>,
        opencode_args: Vec<String>,
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

    pub target: Option<String>,
}

impl Cli {
    pub fn requested_action(self) -> RequestedAction {
        match (self.command, self.target) {
            (
                Some(Command::New {
                    name,
                    dir,
                    opencode_args,
                }),
                None,
            ) => RequestedAction::New {
                name,
                dir,
                opencode_args,
            },
            (
                Some(Command::Alias {
                    name,
                    dir,
                    opencode_args,
                }),
                None,
            ) => RequestedAction::Alias {
                name,
                dir,
                opencode_args,
            },
            (Some(Command::Unalias { name }), None) => RequestedAction::Unalias { name },
            (Some(Command::Rm { target }), None) => RequestedAction::Rm { target },
            (Some(Command::Stop { target }), None) => RequestedAction::Stop { target },
            (Some(Command::Restart { target }), None) => RequestedAction::Restart { target },
            (Some(Command::Move { target, new_dir }), None) => {
                RequestedAction::Move { target, new_dir }
            }
            (Some(Command::Migrate), None) => RequestedAction::Migrate,
            (Some(Command::List { json }), None) => RequestedAction::List { json },
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

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(visible_alias = "n")]
    New {
        name: String,
        dir: Option<PathBuf>,
        #[arg(last = true)]
        opencode_args: Vec<String>,
    },
    Alias {
        name: String,
        dir: Option<PathBuf>,
        #[arg(last = true)]
        opencode_args: Vec<String>,
    },
    Unalias {
        name: String,
    },
    #[command(name = "rm", visible_aliases = ["delete", "d"])]
    Rm {
        target: String,
    },
    Stop {
        target: String,
    },
    Restart {
        target: String,
    },
    #[command(name = "mv")]
    Move {
        target: String,
        new_dir: PathBuf,
    },
    Migrate,
    List {
        #[arg(long)]
        json: bool,
    },
    #[command(name = "__dump-session-list", hide = true)]
    DumpSessionList,
    #[command(name = "__dump-runtime-config", hide = true)]
    DumpRuntimeConfig,
    #[command(name = "__parse-memory-status", hide = true)]
    ParseMemoryStatus {
        path: PathBuf,
    },
}
