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
    AttachTarget {
        target: String,
    },
    Default,
    DumpSessionList,
    DumpRuntimeConfig,
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
            (Some(Command::DumpSessionList), None) => RequestedAction::DumpSessionList,
            (Some(Command::DumpRuntimeConfig), None) => RequestedAction::DumpRuntimeConfig,
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
    #[command(name = "__dump-session-list", hide = true)]
    DumpSessionList,
    #[command(name = "__dump-runtime-config", hide = true)]
    DumpRuntimeConfig,
}
