use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "oc",
    version,
    about = "Interactive TUI session manager for OpenCode"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
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
    #[command(name = "__dump-runtime-config", hide = true)]
    DumpRuntimeConfig,
}
