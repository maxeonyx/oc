use clap::{Parser, Subcommand};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "oc",
    version,
    about = "Interactive TUI session manager for OpenCode"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(name = "__dump-runtime-config", hide = true)]
    DumpRuntimeConfig,
}

#[derive(Debug)]
struct RuntimeConfig {
    aliases_file: PathBuf,
    tmux_prefix: String,
    opencode_db: Option<PathBuf>,
}

impl RuntimeConfig {
    fn from_env() -> Self {
        let home_dir = home_dir();

        Self {
            aliases_file: env_path("OC_ALIASES_FILE")
                .unwrap_or_else(|| home_dir.join(".config/oc/aliases")),
            tmux_prefix: env::var("OC_TMUX_PREFIX").unwrap_or_else(|_| String::from("oc-")),
            opencode_db: env_path("OC_OPENCODE_DB"),
        }
    }

    fn write_debug_dump(&self) {
        println!("aliases_file={}", self.aliases_file.display());
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

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("HOME must be set so oc can resolve default paths"))
}

fn main() {
    let cli = Cli::parse();

    if let Some(Command::DumpRuntimeConfig) = cli.command {
        RuntimeConfig::from_env().write_debug_dump();
    }
}
