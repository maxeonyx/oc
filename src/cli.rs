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
    Alias {
        name: String,
        dir: Option<PathBuf>,
        #[arg(last = true)]
        opencode_args: Vec<String>,
    },
    Unalias {
        name: String,
    },
    #[command(name = "__dump-runtime-config", hide = true)]
    DumpRuntimeConfig,
}
